//! Region analysis (doc sections 3.1-3.7): CFG and blocks consistent with tier 1, natural loops and nests,
//! single-entry check, liveness, the type fixpoint with speculation and peel detection, and the
//! [`RegionPlan`]. Pure functions over [`FuncInfo`]; no VM state.
//!
//! Flow of a promotion: `Analysis::new(info)` (once per function) -> `select_region(hot_pc, policy)` ->
//! `plan(loop, spec, policy)`.

#![allow(clippy::needless_range_loop)] // index-driven graph code: indices are block / pc ids

use super::effects::{self as fx, State, StepCtx};
use super::*;
use rn_core::instruction::Instruction;
use rn_core::ver::active::OpCode;

/// Why a function or nest is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reject {
    /// No code or an invalid opcode / follower layout.
    Malformed,
    /// A jump leaves the code, lands in a follower, or the code falls off its end.
    BadJump,
    /// No loop has a back-edge instruction at the given pc.
    NoSuchLoop,
    /// A jump from outside targets the inside of the nest (not the head).
    SideEntry,
    /// Retreating edge that is not a natural back-edge touches the nest, or the loop is not contiguous /
    /// not properly nested.
    Irreducible,
    /// Larger than [`Policy::max_region`].
    TooLarge,
    /// Contains `GETVARG` / `ERRNNIL`.
    NamedVararg,
    /// The dataflow did not converge within its bound (a bug; reported instead of looping).
    NoConverge,
    /// The first instruction of the region is `Dead` under the entry speculation (region code would exit
    /// immediately, every time): not worth a region.
    DeadEntry,
}

/// A basic block (followers are inside the block of the instruction that owns them).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// First pc.
    pub start: u32,
    /// One past the last pc.
    pub end: u32,
    /// Last normal-role pc.
    pub last: u32,
    /// Successor blocks.
    pub succs: Vec<u32>,
    /// Predecessor blocks (reachable ones only).
    pub preds: Vec<u32>,
    /// Reachable from pc 0?
    pub reachable: bool,
}

/// A natural loop (all back-edges to one header merged).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopInfo {
    /// Header block.
    pub header: u32,
    /// Head pc.
    pub head: u32,
    /// Last back-edge instruction pc `B`.
    pub end: u32,
    /// Latch blocks.
    pub latches: Vec<u32>,
    /// Body blocks (sorted).
    pub body: Vec<u32>,
    /// Parent loop (smallest enclosing extent).
    pub parent: Option<usize>,
    /// Depth (outermost = 0).
    pub depth: u32,
    /// All body blocks lie inside `[head, end]` and no other loop partially overlaps this extent.
    pub well_formed: bool,
}

/// Function-wide analysis: CFG, loops, liveness.
pub struct Analysis<'a> {
    /// The function.
    pub info: &'a FuncInfo<'a>,
    /// Role of each pc.
    pub role: Vec<Role>,
    /// Logical successor pcs of each normal pc.
    pub succ_pc: Vec<Vec<u32>>,
    /// Real jump targets (never fall-through only).
    pub is_target: Vec<bool>,
    /// Blocks in pc order.
    pub blocks: Vec<Block>,
    /// Block index of each pc.
    pub block_of: Vec<u32>,
    /// Natural loops, sorted by (head, end descending): parents precede children.
    pub loops: Vec<LoopInfo>,
    /// Retreating edges `(from block, to block)` that are not natural back-edges.
    pub irreducible: Vec<(u32, u32)>,
    rpo: Vec<u32>,
    live_in: Vec<SlotSet>,
}

fn op_at(info: &FuncInfo<'_>, pc: usize) -> Option<OpCode> {
    Instruction(info.code[pc]).opcode()
}

/// Tier 1 starts a block after these (exit points, calls, loop prologues): `lower.rs` `head[]`.
fn ends_block_in_tier1(op: OpCode) -> bool {
    use OpCode as O;
    matches!(
        op,
        O::Jmp
            | O::Lfalseskip
            | O::Call
            | O::Tailcall
            | O::Return
            | O::Return0
            | O::Return1
            | O::Tforcall
            | O::Varargprep
            | O::Vararg
            | O::Close
            | O::Tbc
            | O::Tforprep
            | O::Forprep
            | O::Forloop
            | O::Tforloop
    )
}

impl<'a> Analysis<'a> {
    /// Build the CFG, loops and liveness of a function.
    pub fn new(info: &'a FuncInfo<'a>) -> Result<Analysis<'a>, Reject> {
        let n = info.code.len();
        if n == 0 || info.max_stack > 256 {
            return Err(Reject::Malformed);
        }
        // roles
        let mut role = vec![Role::Normal; n];
        for pc in 0..n {
            if role[pc] != Role::Normal {
                continue;
            }
            let i = Instruction(info.code[pc]);
            let op = i.opcode().ok_or(Reject::Malformed)?;
            let next = if pc + 1 < n {
                op_at(info, pc + 1)
            } else {
                None
            };
            use OpCode as O;
            if fx::has_mmbin_follower(op) && matches!(next, Some(O::Mmbin | O::Mmbini | O::Mmbink))
            {
                role[pc + 1] = Role::Mmbin;
            } else if fx::is_cond(op) {
                if next != Some(O::Jmp) {
                    return Err(Reject::Malformed);
                }
                role[pc + 1] = Role::JmpFollower;
            } else if fx::has_extra_follower(op, i) {
                if next != Some(O::Extraarg) {
                    return Err(Reject::Malformed);
                }
                role[pc + 1] = Role::Extra;
            }
        }
        for pc in 0..n {
            if role[pc] == Role::Normal
                && matches!(
                    op_at(info, pc),
                    Some(OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink | OpCode::Extraarg)
                )
            {
                return Err(Reject::Malformed);
            }
        }
        // successors
        let skip = |pc: usize| -> u32 {
            let mut q = pc + 1;
            while q < n && role[q].is_follower() && role[q] != Role::JmpFollower {
                q += 1;
            }
            q as u32
        };
        let mut succ_pc: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut fall: Vec<Option<u32>> = vec![None; n]; // the plain fall-through of straight ops
        for pc in 0..n {
            if role[pc] != Role::Normal {
                continue;
            }
            let i = Instruction(info.code[pc]);
            let op = i.opcode().ok_or(Reject::Malformed)?;
            let p = pc as i64;
            use OpCode as O;
            let s: Vec<i64> = match op {
                O::Jmp => vec![p + 1 + i64::from(i.sj())],
                _ if fx::is_cond(op) => {
                    let j = Instruction(info.code[pc + 1]);
                    vec![p + 2 + i64::from(j.sj()), p + 2]
                }
                O::Lfalseskip => vec![p + 2],
                O::Forloop | O::Tforloop => vec![p + 1, p + 1 - i64::from(i.bx())],
                O::Forprep => vec![p + 1, p + i64::from(i.bx()) + 2],
                O::Tforprep => vec![p + 1 + i64::from(i.bx())],
                O::Return | O::Return0 | O::Return1 | O::Tailcall => vec![],
                _ => {
                    let f = i64::from(skip(pc));
                    fall[pc] = Some(f as u32);
                    vec![f]
                }
            };
            for &t in &s {
                if t < 0 || t >= n as i64 || role[t as usize].is_follower() {
                    return Err(Reject::BadJump);
                }
            }
            let mut v: Vec<u32> = s.iter().map(|&t| t as u32).collect();
            v.dedup();
            succ_pc[pc] = v;
        }
        // leaders and jump targets
        let mut is_leader = vec![false; n + 1];
        let mut is_target = vec![false; n];
        is_leader[0] = true;
        for pc in 0..n {
            if role[pc] != Role::Normal {
                continue;
            }
            let op = op_at(info, pc).ok_or(Reject::Malformed)?;
            let straight = fall[pc].is_some();
            if !straight {
                for &t in &succ_pc[pc] {
                    is_leader[t as usize] = true;
                    is_target[t as usize] = true;
                }
            }
            if ends_block_in_tier1(op)
                || fx::classify(op, pc as u32, &Policy::default()) == InsClass::Call
            {
                let f = skip(pc).max(pc as u32 + 1) as usize;
                if f < n {
                    is_leader[f] = true;
                }
            }
        }
        for (pc, &h) in info.extra_heads.iter().enumerate().take(n) {
            if h && role[pc] == Role::Normal {
                is_leader[pc] = true;
            }
        }
        // blocks
        let mut blocks: Vec<Block> = Vec::new();
        let mut block_of = vec![0u32; n];
        let mut pc = 0;
        while pc < n {
            let start = pc;
            pc += 1;
            while pc < n && !is_leader[pc] {
                pc += 1;
            }
            let end = pc;
            let last = (start..end)
                .rev()
                .find(|&q| role[q] == Role::Normal)
                .ok_or(Reject::Malformed)?;
            for b in &mut block_of[start..end] {
                *b = blocks.len() as u32;
            }
            blocks.push(Block {
                start: start as u32,
                end: end as u32,
                last: last as u32,
                succs: Vec::new(),
                preds: Vec::new(),
                reachable: false,
            });
        }
        for bi in 0..blocks.len() {
            let last = blocks[bi].last as usize;
            let mut ss: Vec<u32> = Vec::new();
            for &t in &succ_pc[last] {
                if !is_leader[t as usize] {
                    return Err(Reject::BadJump);
                }
                let tb = block_of[t as usize];
                if !ss.contains(&tb) {
                    ss.push(tb);
                }
            }
            blocks[bi].succs = ss;
        }
        // a block whose last normal op is straight must fall into the next block (checked above via leaders)
        // reachability, RPO, dominators
        let nb = blocks.len();
        let mut order: Vec<u32> = Vec::new(); // postorder
        let mut seen = vec![false; nb];
        let mut stack: Vec<(u32, usize)> = vec![(0, 0)];
        seen[0] = true;
        while let Some(&mut (b, ref mut k)) = stack.last_mut() {
            if let Some(&s) = blocks[b as usize].succs.get(*k) {
                *k += 1;
                if !seen[s as usize] {
                    seen[s as usize] = true;
                    stack.push((s, 0));
                }
            } else {
                order.push(b);
                stack.pop();
            }
        }
        let rpo: Vec<u32> = order.iter().rev().copied().collect();
        for (bi, b) in blocks.iter_mut().enumerate() {
            b.reachable = seen[bi];
        }
        for bi in 0..nb {
            if !blocks[bi].reachable {
                continue;
            }
            for s in blocks[bi].succs.clone() {
                blocks[s as usize].preds.push(bi as u32);
            }
        }
        let mut rpo_num = vec![u32::MAX; nb];
        for (k, &b) in rpo.iter().enumerate() {
            rpo_num[b as usize] = k as u32;
        }
        let mut idom = vec![u32::MAX; nb];
        idom[0] = 0;
        let mut changed = true;
        while changed {
            changed = false;
            for &b in rpo.iter().skip(1) {
                let mut ni = u32::MAX;
                for &p in &blocks[b as usize].preds {
                    if idom[p as usize] == u32::MAX {
                        continue;
                    }
                    ni = if ni == u32::MAX {
                        p
                    } else {
                        let (mut x, mut y) = (p, ni);
                        while x != y {
                            while rpo_num[x as usize] > rpo_num[y as usize] {
                                x = idom[x as usize];
                            }
                            while rpo_num[y as usize] > rpo_num[x as usize] {
                                y = idom[y as usize];
                            }
                        }
                        x
                    };
                }
                if ni != idom[b as usize] {
                    idom[b as usize] = ni;
                    changed = true;
                }
            }
        }
        let dominates = |d: u32, mut x: u32| -> bool {
            loop {
                if x == d {
                    return true;
                }
                if x == 0 {
                    return false;
                }
                x = idom[x as usize];
            }
        };
        // back-edges and loops
        let mut irreducible = Vec::new();
        let mut by_header: Vec<(u32, Vec<u32>)> = Vec::new();
        for u in 0..nb as u32 {
            if !blocks[u as usize].reachable {
                continue;
            }
            for &v in &blocks[u as usize].succs {
                if dominates(v, u) {
                    match by_header.iter_mut().find(|(h, _)| *h == v) {
                        Some((_, l)) => l.push(u),
                        None => by_header.push((v, vec![u])),
                    }
                } else if blocks[v as usize].start <= blocks[u as usize].start {
                    irreducible.push((u, v));
                }
            }
        }
        let mut loops: Vec<LoopInfo> = by_header
            .into_iter()
            .map(|(h, latches)| {
                let mut body = vec![h];
                let mut work: Vec<u32> = latches.iter().copied().filter(|&l| l != h).collect();
                body.extend(work.iter().copied());
                while let Some(x) = work.pop() {
                    for &p in &blocks[x as usize].preds {
                        if !body.contains(&p) {
                            body.push(p);
                            work.push(p);
                        }
                    }
                }
                body.sort_unstable();
                let head = blocks[h as usize].start;
                let end = latches
                    .iter()
                    .map(|&l| blocks[l as usize].end - 1)
                    .max()
                    .unwrap_or(head);
                let well_formed = body.iter().all(|&b| {
                    blocks[b as usize].start >= head && blocks[b as usize].end - 1 <= end
                });
                LoopInfo {
                    header: h,
                    head,
                    end,
                    latches,
                    body,
                    parent: None,
                    depth: 0,
                    well_formed,
                }
            })
            .collect();
        loops.sort_by_key(|l| (l.head, std::cmp::Reverse(l.end)));
        for i in 0..loops.len() {
            let (h, e) = (loops[i].head, loops[i].end);
            let mut par = None;
            for j in 0..i {
                let (jh, je) = (loops[j].head, loops[j].end);
                if jh <= h && e <= je && (jh, je) != (h, e) {
                    par = Some(j); // later j = smaller enclosing extent (sorted by head, then larger end first)
                }
                if jh < h && h <= je && je < e {
                    loops[i].well_formed = false; // partial overlap
                    loops[j].well_formed = false;
                }
            }
            loops[i].parent = par;
            loops[i].depth = par.map_or(0, |p| loops[p].depth + 1);
        }
        let mut an = Analysis {
            info,
            role,
            succ_pc,
            is_target,
            blocks,
            block_of,
            loops,
            irreducible,
            rpo,
            live_in: Vec::new(),
        };
        an.compute_liveness();
        Ok(an)
    }

    /// Real (function-wide) liveness: `live_out(exit) = {}`.
    fn compute_liveness(&mut self) {
        let nb = self.blocks.len();
        let mut gen_kill: Vec<(SlotSet, SlotSet)> = vec![(SlotSet::EMPTY, SlotSet::EMPTY); nb];
        for b in 0..nb {
            let (mut gen, mut kill) = (SlotSet::EMPTY, SlotSet::EMPTY);
            for pc in (self.blocks[b].start..self.blocks[b].end).rev() {
                if self.role[pc as usize] != Role::Normal {
                    continue;
                }
                let e = fx::effects(self.info, pc as usize);
                gen = gen.minus(&e.defs);
                gen.union_with(&e.uses);
                kill.union_with(&e.defs);
            }
            gen_kill[b] = (gen, kill);
        }
        let mut live_in = vec![SlotSet::EMPTY; nb];
        let mut changed = true;
        while changed {
            changed = false;
            for &b in self.rpo.iter().rev() {
                let mut out = SlotSet::EMPTY;
                for &s in &self.blocks[b as usize].succs {
                    out.union_with(&live_in[s as usize]);
                }
                let mut li = out.minus(&gen_kill[b as usize].1);
                li.union_with(&gen_kill[b as usize].0);
                if li != live_in[b as usize] {
                    live_in[b as usize] = li;
                    changed = true;
                }
            }
        }
        self.live_in = live_in;
    }

    /// Function-wide live-in slots at the start of the block containing `pc` (must be a block start).
    pub fn live_in_block(&self, pc: u32) -> SlotSet {
        self.live_in[self.block_of[pc as usize] as usize]
    }

    /// Index of the innermost loop with a back-edge instruction at `pc`.
    pub fn loop_with_backedge(&self, pc: u32) -> Option<usize> {
        self.loops
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                l.latches
                    .iter()
                    .any(|&b| self.blocks[b as usize].end - 1 == pc)
            })
            .max_by_key(|(_, l)| l.depth)
            .map(|(i, _)| i)
    }

    /// Would a jump from outside `[head, end]` reach the inside other than at `head`?
    pub fn single_entry(&self, head: u32, end: u32) -> bool {
        for b in &self.blocks {
            if !b.reachable || (b.start >= head && b.start <= end) {
                continue;
            }
            for &s in &b.succs {
                let t = self.blocks[s as usize].start;
                if t > head && t <= end {
                    return false;
                }
            }
        }
        true
    }

    fn contains_named_vararg(&self, head: u32, end: u32) -> bool {
        #[cfg(not(rustynail_lua54))]
        for pc in head..=end {
            if matches!(
                op_at(self.info, pc as usize),
                Some(OpCode::Getvarg | OpCode::Errnnil)
            ) {
                return true;
            }
        }
        let _ = (head, end);
        false
    }

    fn region_ok(&self, l: usize, policy: &Policy) -> Result<(), Reject> {
        let lp = &self.loops[l];
        if !lp.well_formed {
            return Err(Reject::Irreducible);
        }
        if (lp.end - lp.head + 1) as usize > policy.max_region {
            return Err(Reject::TooLarge);
        }
        let touches = |b: u32| {
            let s = self.blocks[b as usize].start;
            s >= lp.head && s <= lp.end
        };
        if self
            .irreducible
            .iter()
            .any(|&(u, v)| touches(u) || touches(v))
        {
            return Err(Reject::Irreducible);
        }
        if !self.single_entry(lp.head, lp.end) {
            return Err(Reject::SideEntry);
        }
        if policy.refuse_named_vararg && self.contains_named_vararg(lp.head, lp.end) {
            return Err(Reject::NamedVararg);
        }
        Ok(())
    }

    /// Region selection (doc 3.1): the innermost loop whose back-edge instruction is `hot_pc`, grown to the
    /// outermost enclosing loop that stays within the size cap, single-entry and free of refused ops.
    /// (Decision: growth also stops before a parent that contains a refused op, so an eligible inner nest
    /// is not lost to a refusing outer loop.)
    pub fn select_region(&self, hot_pc: u32, policy: &Policy) -> Result<usize, Reject> {
        let l = self.loop_with_backedge(hot_pc).ok_or(Reject::NoSuchLoop)?;
        self.grow_region(l, policy)
    }

    /// Region of a nest `[head, end]` as `scan_nests` reports it: the loop headed at `head` whose back-edge is `end`, or,
    /// when `end` is not the latch of a reachable loop (a `while true do ... break ... end` whose closing `JMP` follows
    /// the `break`s and is dead code; a `repeat ... until true` tail), the loop headed at `head` with the last
    /// reachable latch inside the nest (T7, nest selection: heapsort's `sift`).
    pub fn select_nest(&self, head: u32, end: u32, policy: &Policy) -> Result<usize, Reject> {
        match self.select_region(end, policy) {
            Err(Reject::NoSuchLoop) => {}
            r => return r,
        }
        let l = self
            .loops
            .iter()
            .enumerate()
            .filter(|(_, l)| l.head == head && l.end <= end)
            .max_by_key(|(_, l)| (l.end, std::cmp::Reverse(l.depth)))
            .map(|(i, _)| i)
            .ok_or(Reject::NoSuchLoop)?;
        self.grow_region(l, policy)
    }

    fn grow_region(&self, mut l: usize, policy: &Policy) -> Result<usize, Reject> {
        while let Some(p) = self.loops[l].parent {
            if self.region_ok(p, policy).is_err() {
                break;
            }
            l = p;
        }
        self.region_ok(l, policy)?;
        Ok(l)
    }

    /// Region loops that are not nested in another loop.
    pub fn outermost_loops(&self) -> Vec<usize> {
        (0..self.loops.len())
            .filter(|&l| self.loops[l].parent.is_none())
            .collect()
    }

    fn const_int_def(&self, pc: u32, slot: u32) -> Option<i64> {
        let mut p = pc;
        let mut guard = 0;
        while p > 0 && guard < 16 && !self.is_target[p as usize] {
            p -= 1;
            guard += 1;
            if self.role[p as usize] != Role::Normal {
                return None;
            }
            let i = Instruction(self.info.code[p as usize]);
            let e = fx::effects(self.info, p as usize);
            if e.may_defs.contains(slot) {
                return None;
            }
            if e.defs.contains(slot) {
                return match i.opcode()? {
                    OpCode::Loadi if i.a() == slot => Some(i64::from(i.sbx())),
                    OpCode::Loadk if i.a() == slot => *self.info.k_int.get(i.bx() as usize)?,
                    _ => None,
                };
            }
        }
        None
    }

    /// Compute the plan of loop `l` (doc 3.4): speculation, fixpoint, peel detection, classes.
    pub fn plan(
        &self,
        l: usize,
        spec: &Speculation<'_>,
        policy: &Policy,
    ) -> Result<RegionPlan, Reject> {
        self.region_ok(l, policy)?;
        let info = self.info;
        let lp = &self.loops[l];
        let (head, end) = (lp.head, lp.end);
        let nslots = info.max_stack as usize;
        let npc = (end - head + 1) as usize;

        // region blocks: reachable from the header through edges that stay in [head, end]
        let inside = |b: u32| {
            let s = self.blocks[b as usize].start;
            s >= head && s <= end
        };
        let mut post: Vec<u32> = Vec::new();
        let mut seen = vec![false; self.blocks.len()];
        let mut stack: Vec<(u32, usize)> = vec![(lp.header, 0)];
        seen[lp.header as usize] = true;
        while let Some(&mut (b, ref mut k)) = stack.last_mut() {
            let succs = &self.blocks[b as usize].succs;
            if let Some(&s) = succs.get(*k) {
                *k += 1;
                if inside(s) && !seen[s as usize] {
                    seen[s as usize] = true;
                    stack.push((s, 0));
                }
            } else {
                post.push(b);
                stack.pop();
            }
        }
        let order: Vec<u32> = post.iter().rev().copied().collect(); // RPO of the region subgraph
        let ridx = |b: u32| order.iter().position(|&x| x == b);

        // classes of the region's pcs
        let class_of: Vec<InsClass> = (0..npc)
            .map(|k| {
                let pc = head + k as u32;
                if self.role[pc as usize].is_follower() {
                    InsClass::Follower
                } else {
                    fx::classify(op_at(info, pc as usize).expect("validated"), pc, policy)
                }
            })
            .collect();
        let class = |pc: u32| class_of[(pc - head) as usize];

        // guard liveness (B5): upward-exposed typed uses, exits have empty live-out. With `cut`, barrier ops
        // and exits also end the exposure (the stub after a barrier re-guards what is read later): that is the
        // set a stub at the pc must check. Without `cut` it is "read by region code before redefinition on
        // some path, whatever barriers lie between": the slots whose observed type is worth speculating.
        let guard_uses = |pc: u32| -> SlotSet {
            match class(pc) {
                InsClass::Typed | InsClass::Table => fx::effects(info, pc as usize).uses,
                InsClass::Intr => {
                    fx::intr_uses(Instruction(info.code[pc as usize]), info.max_stack)
                }
                _ => SlotSet::EMPTY,
            }
        };
        let exposure = |cut: bool| -> Vec<SlotSet> {
            let mut gl_in: Vec<SlotSet> = vec![SlotSet::EMPTY; npc];
            let mut bl_in: Vec<SlotSet> = vec![SlotSet::EMPTY; order.len()];
            loop {
                let mut changed = false;
                for (ri, &b) in order.iter().enumerate().rev() {
                    let blk = &self.blocks[b as usize];
                    let mut live = SlotSet::EMPTY;
                    for &s in &blk.succs {
                        if let Some(si) = ridx(s) {
                            live.union_with(&bl_in[si]);
                        }
                    }
                    for pc in (blk.start..blk.end).rev() {
                        if self.role[pc as usize] != Role::Normal {
                            continue;
                        }
                        let e = fx::effects(info, pc as usize);
                        let cls = class(pc);
                        live = if cls == InsClass::Exit || (cut && cls.is_barrier()) {
                            SlotSet::EMPTY
                        } else {
                            let mut x = live.minus(&e.defs);
                            x.union_with(&guard_uses(pc));
                            x
                        };
                        gl_in[(pc - head) as usize] = live;
                    }
                    if live != bl_in[ri] {
                        bl_in[ri] = live;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            gl_in
        };
        let gl_in = exposure(true);
        let spec_head = exposure(false)[0];
        let gl_fn = |pc: u32| -> SlotSet {
            if pc >= head && pc <= end {
                gl_in[(pc - head) as usize]
            } else {
                SlotSet::EMPTY
            }
        };
        let ctx = StepCtx {
            info,
            spec,
            policy,
            guard_live: &gl_fn,
        };

        // heads of the loops nested in the region: OSR resume points (entered from outside the region)
        let inner_heads: Vec<u32> = self
            .loops
            .iter()
            .filter(|x| x.head > head && x.head <= end && x.end <= end && x.head <= x.end)
            .map(|x| x.head)
            .collect();

        // entry state and the peel / widen rounds
        let exposed_h = gl_fn(head); // guarded by stub(H)
        let obs_set = |s: u32| -> TagSet {
            let mut t = match spec.obs.get(s as usize) {
                Some(&b) if b != 0xFF => TagSet::of_tag(b),
                _ => TagSet::ANY,
            };
            for &(w, ws) in spec.entry_widen {
                if u32::from(w) == s {
                    t = t.join(ws);
                }
            }
            t
        };
        let mut entry_t: Vec<TagSet> = (0..nslots as u32)
            .map(|s| {
                if spec_head.contains(s) {
                    widen_entry(obs_set(s))
                } else {
                    TagSet::ANY
                }
            })
            .collect();
        let mut peeled = vec![false; nslots];
        let mut stats = PlanStats::default();
        let cap = 16 + 14 * nslots + policy.extra_sweeps;
        let solved = loop {
            stats.rounds += 1;
            let entry = entry_state(&entry_t, exposed_h);
            let sol = self.solve(&order, &ctx, &entry, &inner_heads, cap)?;
            stats.sweeps = stats.sweeps.max(sol.sweeps);
            // back-edge types at the header
            let mut back = vec![TagSet::EMPTY; nslots];
            for &p in &self.blocks[lp.header as usize].preds {
                if let Some(pi) = ridx(p) {
                    if let Some(es) =
                        sol.edge_state(self, pi, self.blocks[lp.header as usize].start)
                    {
                        for (b, t) in back.iter_mut().zip(&es.t) {
                            *b = b.join(*t);
                        }
                    }
                }
            }
            let mut changed = false;
            for s in spec_head.iter() {
                let s = s as usize;
                if s >= nslots || back[s].subset(entry_t[s]) {
                    continue;
                }
                changed = true;
                entry_t[s] = if !peeled[s] && back[s].is_singleton() && entry_t[s].is_singleton() {
                    peeled[s] = true;
                    back[s]
                } else {
                    TagSet::ANY
                };
            }
            if !changed {
                break sol;
            }
            if stats.rounds as usize > 4 * nslots + 8 {
                return Err(Reject::NoConverge);
            }
        };

        // recording pass
        let mut facts = vec![TagSet::EMPTY; npc * nslots];
        let mut mem = vec![SlotSet::EMPTY; npc];
        let mut disp = vec![Disp::Opaque; npc];
        let mut gdef: Vec<Option<TagSet>> = vec![None; npc];
        let mut reached = vec![false; npc];
        let mut fors: Vec<ForInfo> = Vec::new();
        for (ri, &b) in order.iter().enumerate() {
            let Some(inn) = solved.ins[ri].clone() else {
                continue;
            };
            let blk = &self.blocks[b as usize];
            let mut st = inn;
            let mut live = true;
            for pc in blk.start..blk.end {
                let k = (pc - head) as usize;
                if !live {
                    break;
                }
                facts[k * nslots..(k + 1) * nslots].copy_from_slice(&st.t);
                mem[k] = st.mem;
                reached[k] = true;
                if self.role[pc as usize] != Role::Normal {
                    continue;
                }
                let cls = class(pc);
                let out = fx::step(&ctx, pc, cls, &mut st);
                disp[k] = out.disp;
                gdef[k] = out.guard_def;
                if cls == InsClass::Exit {
                    live = false;
                }
                if op_at(info, pc as usize) == Some(OpCode::Forprep) {
                    let a = Instruction(info.code[pc as usize]).a();
                    fors.push(ForInfo {
                        pc,
                        a,
                        ctl: st.t.get(a as usize + 2).copied().unwrap_or(TagSet::ANY),
                        step_const: self.const_int_def(pc, a + 2),
                    });
                }
            }
        }
        fors.sort_by_key(|f| f.pc);
        if reached[0] && disp[0] == Disp::Dead {
            return Err(Reject::DeadEntry);
        }

        // per-instruction plans
        let mut written = SlotSet::EMPTY;
        let mut has_setupval = false;
        let mut insns = Vec::with_capacity(npc);
        for k in 0..npc {
            let pc = head + k as u32;
            let role = self.role[pc as usize];
            let (e, cls) = if role == Role::Normal {
                (fx::effects(info, pc as usize), class_of[k])
            } else {
                (fx::Effects::default(), InsClass::Follower)
            };
            if role == Role::Normal {
                written.union_with(&e.defs);
                written.union_with(&e.may_defs);
                has_setupval |= op_at(info, pc as usize) == Some(OpCode::Setupval);
            }
            insns.push(InsnPlan {
                pc,
                role,
                class: cls,
                disp: if role == Role::Normal {
                    disp[k]
                } else {
                    Disp::Opaque
                },
                reached: reached[k],
                barrier: cls.is_barrier() || disp[k] == Disp::LocalBarrier,
                uses: e.uses,
                defs: e.defs,
                may_defs: e.may_defs,
                guard_def: gdef[k],
                guard_live_in: if role == Role::Normal {
                    gl_in[k]
                } else {
                    SlotSet::EMPTY
                },
                mem_known: mem[k],
            });
        }
        let invariant = SlotSet::first(info.max_stack)
            .minus(&written)
            .minus(&info.escaped);

        // loops, blocks, exits, resume points, entry facts
        let mut idxs: Vec<usize> = (0..self.loops.len())
            .filter(|&j| {
                // (a loop whose latch lies before its header is not a region loop: no head, no resume point)
                self.loops[j].head >= head
                    && self.loops[j].end <= end
                    && self.loops[j].head <= self.loops[j].end
            })
            .collect();
        idxs.sort_by_key(|&j| (self.loops[j].head, std::cmp::Reverse(self.loops[j].end)));
        let base_depth = self.loops[l].depth;
        let ploops: Vec<RegionLoop> = idxs
            .iter()
            .map(|&j| RegionLoop {
                head: self.loops[j].head,
                end: self.loops[j].end,
                depth: self.loops[j].depth - base_depth,
                parent: self.loops[j]
                    .parent
                    .and_then(|p| idxs.iter().position(|&x| x == p)),
            })
            .collect();
        let mut rblocks: Vec<RegionBlock> = Vec::new();
        let mut exits = Vec::new();
        for (ri, &b) in order.iter().enumerate() {
            if solved.ins[ri].is_none() {
                continue;
            }
            let blk = &self.blocks[b as usize];
            let mut succs = Vec::new();
            for &s in &self.succ_pc[blk.last as usize] {
                if s >= head && s <= end {
                    succs.push(s);
                } else {
                    exits.push((blk.last, s));
                }
            }
            let depth = ploops
                .iter()
                .filter(|x| x.head <= blk.start && blk.start <= x.end)
                .count() as u32;
            rblocks.push(RegionBlock {
                start: blk.start,
                end: blk.end,
                succs,
                depth,
            });
        }
        rblocks.sort_by_key(|b| b.start);
        exits.sort_unstable();
        let mut resume_points = vec![head];
        for x in &ploops {
            resume_points.push(x.head);
        }
        for k in 0..npc {
            let pc = head + k as u32;
            if reached[k]
                && (class_of[k].is_barrier()
                    || class_of[k] == InsClass::Intr
                    || disp[k] == Disp::LocalBarrier)
            {
                // the continuation of a barrier op (a leaving op has no successor, no stub)
                if let [f] = self.succ_pc[pc as usize][..] {
                    if f >= head && f <= end && reached[(f - head) as usize] {
                        resume_points.push(f);
                    }
                }
            }
        }
        resume_points.sort_unstable();
        resume_points.dedup();
        let entry_facts: Vec<EntryFact> = spec_head
            .iter()
            .filter(|&s| (s as usize) < nslots && entry_t[s as usize].is_singleton())
            .map(|s| EntryFact {
                slot: s,
                set: entry_t[s as usize],
                peel: peeled[s as usize],
                guarded: exposed_h.contains(s),
            })
            .collect();

        Ok(RegionPlan {
            head,
            end,
            nslots: nslots as u32,
            loops: ploops,
            blocks: rblocks,
            exits,
            entry: entry_facts,
            insns,
            written,
            escaped: info.escaped,
            invariant,
            resume_points,
            writeback_all: SlotSet::first(info.max_stack),
            has_setupval,
            intr: policy
                .intr
                .iter()
                .copied()
                .filter(|&(p, _)| p >= head && p <= end)
                .collect(),
            fors,
            stats,
            facts,
        })
    }

    /// Forward dataflow over the region subgraph `order` (RPO, header first) from `entry`.
    fn solve(
        &self,
        order: &[u32],
        ctx: &StepCtx<'_>,
        entry: &State,
        inner_heads: &[u32],
        cap: usize,
    ) -> Result<Solved, Reject> {
        let n = order.len();
        let ridx = |b: u32| order.iter().position(|&x| x == b);
        let mut sol = Solved {
            ins: vec![None; n],
            outs: vec![None; n],
            pres: vec![None; n],
            lastbar: vec![false; n],
            blocks: order.to_vec(),
            sweeps: 0,
            last_pc: order
                .iter()
                .map(|&b| self.blocks[b as usize].last)
                .collect(),
            forprep: order
                .iter()
                .map(|&b| {
                    let last = self.blocks[b as usize].last as usize;
                    (op_at(self.info, last) == Some(OpCode::Forprep))
                        .then(|| self.succ_pc[last].get(1).copied())
                        .flatten()
                })
                .collect(),
            nfor: 3,
            fa: order
                .iter()
                .map(|&b| Instruction(self.info.code[self.blocks[b as usize].last as usize]).a())
                .collect(),
        };
        loop {
            sol.sweeps += 1;
            if sol.sweeps as usize > cap {
                return Err(Reject::NoConverge);
            }
            let mut changed = false;
            for ri in 0..n {
                let b = order[ri];
                let mut inn: Option<State> = (ri == 0).then(|| entry.clone());
                for &p in &self.blocks[b as usize].preds {
                    let Some(pi) = ridx(p) else { continue };
                    if let Some(es) = sol.edge_state(self, pi, self.blocks[b as usize].start) {
                        match &mut inn {
                            None => inn = Some(es),
                            Some(x) => {
                                x.join_from(&es);
                            }
                        }
                    }
                }
                let Some(inn) = inn else { continue };
                // an inner loop head is a resume point (OSR at its back-edge enters `stub(J)` from outside the
                // region): only the stub's guard set is reliable there
                let head_pc = self.blocks[b as usize].start;
                // resume points that start a block: inner loop heads, and continuations of barrier ops
                // (`stub(P+1)` is entered from the resume table while other region paths also flow into `P+1`)
                let is_resume = ri != 0
                    && (inner_heads.contains(&head_pc)
                        || self.blocks[b as usize].preds.iter().any(|&p| {
                            ridx(p).is_some_and(|pi| {
                                sol.lastbar[pi]
                                    && fx::continuation(self.info, self.blocks[p as usize].last)
                                        == head_pc
                            })
                        }));
                let g = (ctx.guard_live)(head_pc);
                let clamp = |st: &mut State| {
                    let checked = st.clamp_to_guards(&g);
                    st.mem = st.mem.meet(&checked);
                };
                let ch = match &mut sol.ins[ri] {
                    None => {
                        let mut inn = inn;
                        if is_resume {
                            clamp(&mut inn);
                        }
                        sol.ins[ri] = Some(inn);
                        true
                    }
                    Some(old) => {
                        let mut c = old.join_from(&inn);
                        if is_resume {
                            let before = old.clone();
                            clamp(old);
                            c |= *old != before;
                        }
                        c
                    }
                };
                if ch || sol.outs[ri].is_none() {
                    if ch {
                        changed = true;
                    }
                    let (out, pre, bar) = self.run_block(ctx, b, sol.ins[ri].clone().expect("set"));
                    sol.lastbar[ri] = bar;
                    if sol.outs[ri] != out {
                        changed = true;
                    }
                    sol.outs[ri] = out;
                    sol.pres[ri] = pre;
                }
            }
            if !changed {
                return Ok(sol);
            }
        }
    }

    /// Transfer a block; `None` out = control never reaches the end (an exit instruction).
    fn run_block(
        &self,
        ctx: &StepCtx<'_>,
        b: u32,
        mut st: State,
    ) -> (Option<State>, Option<State>, bool) {
        let blk = &self.blocks[b as usize];
        let mut pre = None;
        let mut bar = false;
        for pc in blk.start..blk.end {
            if self.role[pc as usize] != Role::Normal {
                continue;
            }
            let op = op_at(self.info, pc as usize).expect("validated");
            let cls = fx::classify(op, pc, ctx.policy);
            if op == OpCode::Forprep {
                pre = Some(st.clone());
            }
            let out = fx::step(ctx, pc, cls, &mut st);
            // (an intrinsic call is a resume point too: its slow path may be a flat Lua call that returns through the
            // resume table into `stub(P+1)`)
            bar = cls.is_barrier() || cls == InsClass::Intr || out.disp == Disp::LocalBarrier;
            if cls == InsClass::Exit {
                return (None, pre, bar);
            }
        }
        (Some(st), pre, bar)
    }
}

/// Entry facts are singletons or `Any` (doc 3.4: other sets are "boxed").
fn widen_entry(t: TagSet) -> TagSet {
    if t.is_singleton() {
        t
    } else {
        TagSet::ANY
    }
}

fn entry_state(t: &[TagSet], exposed: SlotSet) -> State {
    let mut mem = SlotSet::EMPTY;
    for s in exposed.iter() {
        if t.get(s as usize).is_some_and(|x| x.is_singleton()) {
            mem.insert(s);
        }
    }
    State { t: t.to_vec(), mem }
}

struct Solved {
    ins: Vec<Option<State>>,
    outs: Vec<Option<State>>,
    pres: Vec<Option<State>>,
    /// The block's last instruction is a barrier op (class G / Call / `LocalBarrier`).
    lastbar: Vec<bool>,
    #[allow(dead_code)]
    blocks: Vec<u32>,
    sweeps: u32,
    #[allow(dead_code)]
    last_pc: Vec<u32>,
    /// For blocks ending in `FORPREP`: the skip target pc.
    forprep: Vec<Option<u32>>,
    nfor: u32,
    fa: Vec<u32>,
}

impl Solved {
    /// State flowing along the edge from region block `pi` to the block starting at `target`.
    fn edge_state(&self, _an: &Analysis<'_>, pi: usize, target: u32) -> Option<State> {
        let out = self.outs[pi].as_ref()?;
        if self.forprep[pi] == Some(target) {
            // loop skipped: the internal registers keep their pre-FORPREP types
            let pre = self.pres[pi].as_ref()?;
            let mut s = out.clone();
            let a = self.fa[pi];
            for k in 0..self.nfor {
                let i = (a + k) as usize;
                if i < s.t.len() {
                    s.t[i] = pre.t[i];
                    if pre.mem.contains(i as u32) {
                        s.mem.insert(i as u32);
                    } else {
                        s.mem.remove(i as u32);
                    }
                }
            }
            #[cfg(rustynail_lua54)]
            {
                // 5.4 writes the control variable copy `A+3 = int(init)` before it decides to skip (integer
                // loops): after a skip it is the previous value or an integer
                let i = (a + 3) as usize;
                if i < s.t.len() {
                    s.t[i] = pre.t[i].join(TagSet::INT);
                    s.mem.remove(i as u32);
                }
            }
            return Some(s);
        }
        Some(out.clone())
    }
}
