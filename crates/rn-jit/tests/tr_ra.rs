//! J5 P5a (docs/JIT_TRACE.md 9.1 item 3, section 7): the pure register allocator `tr::ra`.
//!
//! 1. property tests: the allocation of random IR (P2 generator, plus shared snapshots) is lowered by a small
//!    test-only reference lowering to straight-line register-machine semantics (register file, spill slots, the
//!    allocation's reloads / stores / parallel moves) and compared with `tr::interp` (exits, snapshot values, final
//!    model), for the default allocation and for `+randra` seeds;
//! 2. the parallel-move sequentialiser against a simultaneous-assignment oracle;
//! 3. a validity checker (value identities per location: no clobbered operand, snapshots restorable from the
//!    recorded locations at every guard, reloads read a stored slot, two-address contract, loop iterations).
//!
//! Miri-runnable (pure Rust, case counts shrink under `cfg(miri)`). Mutants (feature `tr_mutants`,
//! `RUSTYNAIL_JIT_MUT=name`): `phi_skip_move` (property + checker + `swap_cycles`), `phi_swap` (property + move
//! oracle + `swap_cycles`), `spill_no_reload` (property + checker + `pressure`), `exit_rename_ignored` (property +
//! checker, needs the shared-snapshot cases), `phi_before_guard` (property + `loop_slice`).
#![cfg(feature = "trace")]

use rn_jit::tr::interp::gen::{random_case, GenCfg, Rng};
use rn_jit::tr::interp::{kbits, run, Model, Outcome, Val};
use rn_jit::tr::ir::*;
use rn_jit::tr::ra::*;
use std::collections::HashMap;

const CASES: u64 = if cfg!(miri) { 2 } else { 150 };
const SIZE: usize = if cfg!(miri) { 14 } else { 40 };
const SEEDS: u64 = if cfg!(miri) { 3 } else { 1000 };
const MOVE_CASES: u64 = if cfg!(miri) { 40 } else { 20000 };

fn idx(r: Ref) -> usize {
    usize::from(r - REF_BIAS)
}

// ---- shared snapshots ---------------------------------------------------------------------------------------------

/// Makes some consecutive guards of one segment (pre-roll / body) share the snapshot of the first one (as the
/// guards of one bytecode instruction do), so the TR27 machinery has multi-guard snapshots with allocation activity
/// between the guards.
fn share_snapshots(ir: &mut TraceIr, rng: &mut Rng) {
    let l = ir.loop_at.map(idx).unwrap_or(usize::MAX);
    let mut cur: Option<(u32, bool)> = None; // (snapshot, in body)
    let mut left = 0;
    for i in 0..ir.ins.len() {
        if ir.ins[i].flags & F_GUARD == 0 {
            continue;
        }
        let body = i > l;
        let s = aux_snap(ir.ins[i].aux);
        let own_entries = !ir.snap_entries(s).is_empty();
        match cur {
            Some((c, b)) if left > 0 && b == body && ir.ins[i].op != TOp::Sload => {
                let bits = aux_bits(ir.ins[i].aux);
                ir.ins[i].aux = guard_aux(c, bits);
                left -= 1;
            }
            _ => {
                if own_entries && rng.chance(2, 3) {
                    cur = Some((s, body));
                    left = 1 + rng.below(3);
                } else {
                    cur = None;
                    left = 0;
                }
            }
        }
    }
}

// ---- the reference lowering -----------------------------------------------------------------------------------------

struct Mach {
    g: [u64; 11],
    x: [u64; 16],
    spill: Vec<u64>,
}

impl Mach {
    fn new() -> Mach {
        let mut m = Mach {
            g: [0; 11],
            x: [0; 16],
            spill: vec![0; 256],
        };
        for (i, v) in m.g.iter_mut().enumerate() {
            *v = 0xBAD0_0000_0000_0000 | i as u64;
        }
        for (i, v) in m.x.iter_mut().enumerate() {
            *v = 0xBAD1_0000_0000_0000 | i as u64;
        }
        for (i, v) in m.spill.iter_mut().enumerate() {
            *v = 0xBAD2_0000_0000_0000 | i as u64;
        }
        m
    }
    fn rd(&self, ir: &TraceIr, l: Loc) -> u64 {
        match l {
            Loc::None => 0,
            Loc::Const(r) => kbits(ir.k[usize::from(r) - 1]),
            Loc::G(n) => self.g[usize::from(n)],
            Loc::X(n) => self.x[usize::from(n)],
            Loc::Spill(n) => self.spill[usize::from(n)],
        }
    }
    fn wr(&mut self, l: Loc, v: u64) {
        match l {
            Loc::None => {}
            Loc::G(n) => self.g[usize::from(n)] = v,
            Loc::X(n) => self.x[usize::from(n)] = v,
            Loc::Spill(n) => self.spill[usize::from(n)] = v,
            Loc::Const(_) => panic!("write to a constant"),
        }
    }
    fn moves(&mut self, ir: &TraceIr, seq: &[Move]) {
        for m in seq {
            let v = self.rd(ir, m.src);
            self.wr(m.dst, v);
        }
    }
}

enum Step {
    Val(u64),
    Exit(u32),
}

/// Executes one non-control instruction on the model through a one-instruction trace (the exact semantics of
/// `tr::interp`), with the operand values read from the machine. The result goes through a scratch closure.
fn exec(
    ir: &TraceIr,
    ins: &Ins,
    refs: [Ref; 3],
    vals: [u64; 3],
    fused_addr: bool,
    m: &mut Model,
) -> Result<Step, String> {
    let mut t = TraceIr::default();
    t.snaps.push(Snap::default());
    let kc = |t: &mut TraceIr, r: Ref, bits: u64| -> Ref {
        if ir.ty_of(r) == Ty::Num {
            t.konst(KVal::Num(bits))
        } else {
            t.konst(KVal::Int(bits as i64))
        }
    };
    let (ra, rb) = ins.op.ref_operands();
    let (a, b) = if fused_addr {
        let addr = vals[0] << 32 | (vals[1] & 0xFFFF_FFFF);
        let a = t.konst(KVal::Int(addr as i64));
        let b = if refs[2] != NOREF {
            kc(&mut t, refs[2], vals[2])
        } else {
            NOREF
        };
        (a, b)
    } else {
        let a = if ra && refs[0] != NOREF {
            kc(&mut t, refs[0], vals[0])
        } else {
            ins.a
        };
        let b = if rb && refs[1] != NOREF {
            kc(&mut t, refs[1], vals[1])
        } else if rb {
            NOREF
        } else {
            ins.b
        };
        (a, b)
    };
    let aux = if ins.flags & F_GUARD != 0 {
        guard_aux(0, aux_bits(ins.aux))
    } else {
        ins.aux
    };
    t.emit(Ins {
        op: ins.op,
        t: ins.t,
        flags: ins.flags,
        a,
        b,
        prev: NOREF,
        aux,
    });
    let defines = !matches!(
        ins.op,
        TOp::Eq
            | TOp::Ne
            | TOp::Lt
            | TOp::Le
            | TOp::Gt
            | TOp::Ge
            | TOp::Ult
            | TOp::Uge
            | TOp::Fcmp
            | TOp::Eqref
            | TOp::Astore
            | TOp::Ustore
            | TOp::Tbar
    );
    let h = m.closures.len() as u64 + 1;
    if defines {
        m.closures.push(vec![Val::NIL]);
        let cl = t.konst(KVal::Ptr(h, PtrKind::LClosure));
        let z = t.kint(0);
        let u = t.emit(Ins {
            op: TOp::Uref,
            t: Ty::P64,
            flags: 0,
            a: cl,
            b: z,
            prev: NOREF,
            aux: 0,
        });
        t.emit(Ins {
            op: TOp::Ustore,
            t: Ty::Nil,
            flags: 0,
            a: u,
            b: REF_BIAS,
            prev: NOREF,
            aux: 0,
        });
    }
    let (mi, mp) = (m.max_iters, m.poll_exit);
    m.max_iters = 0;
    m.poll_exit = None;
    let r = run(&t, m);
    m.max_iters = mi;
    m.poll_exit = mp;
    let res = if defines {
        let v = m.closures.pop().unwrap()[0].bits;
        Some(v)
    } else {
        None
    };
    match r {
        Err(e) => Err(format!("{e:?}")),
        Ok(Outcome::Exit { .. }) => Ok(Step::Exit(aux_snap(ins.aux))),
        Ok(Outcome::Done { .. }) => Ok(Step::Val(res.unwrap_or(0))),
    }
}

fn resolve(ir: &TraceIr, al: &Allocation, mc: &Mach, s: u32) -> Vec<(u16, u8, Val)> {
    ir.snap_entries(s)
        .iter()
        .zip(&al.snap_locs[s as usize])
        .map(|(e, &l)| {
            let t = if e.r == NOREF { Ty::Nil } else { ir.ty_of(e.r) };
            let bits = if e.r == NOREF { 0 } else { mc.rd(ir, l) };
            (e.slot, e.flags, Val { t, bits })
        })
        .collect()
}

/// Runs the allocation as a register machine. Returns the outcome like `interp::run`; `ends` collects the values at
/// `end_locs` the first time the end of the body is reached.
fn lower_run(ir: &TraceIr, al: &Allocation, m: &mut Model) -> Result<Outcome, String> {
    let n = ir.ins.len();
    let l = ir.loop_at.map(idx);
    let mut mc = Mach::new();
    let (mut iters, mut polls, mut pc) = (0u32, 0u32, 0usize);
    macro_rules! exit {
        ($s:expr) => {{
            let s = $s;
            return Ok(Outcome::Exit {
                snap: s,
                slots: resolve(ir, al, &mc, s),
            });
        }};
    }
    loop {
        if l.is_some() && pc == al.backedge_after {
            mc.moves(ir, &al.backedge_seq);
        }
        if pc == n {
            match l {
                Some(l) if iters < m.max_iters => {
                    iters += 1;
                    pc = l + 1;
                    continue;
                }
                _ => return Ok(Outcome::Done { iters }),
            }
        }
        if Some(pc) == l {
            mc.moves(ir, &al.entry_seq);
        }
        let ins = ir.ins[pc];
        let ia = &al.ins[pc];
        for r in &ia.pre {
            let v = mc.spill[usize::from(r.slot)];
            mc.wr(r.reg, v);
        }
        let refs = operand_refs(ir, &al.fused, pc);
        let mut vals = [0u64; 3];
        for k in 0..3 {
            if refs[k] != NOREF {
                vals[k] = mc.rd(ir, ia.ops[k]);
            }
        }
        let snap = aux_snap(ins.aux);
        let result: Option<u64> = match ins.op {
            TOp::Loop | TOp::Phi => None,
            TOp::Aref | TOp::Uref if al.fused[pc] => None,
            TOp::Poll => {
                polls += 1;
                if m.poll_exit == Some(polls) {
                    exit!(snap);
                }
                None
            }
            TOp::Budget => {
                if m.budget < i64::from(ins.a) {
                    exit!(snap);
                }
                m.budget -= i64::from(ins.a);
                None
            }
            _ => {
                let fused_addr =
                    matches!(ins.op, TOp::Aload | TOp::Astore | TOp::Uload | TOp::Ustore)
                        && TraceIr::is_ins(ins.a)
                        && al.fused[idx(ins.a)];
                match exec(ir, &ins, refs, vals, fused_addr, m)? {
                    Step::Exit(s) => exit!(s),
                    Step::Val(v) => Some(v),
                }
            }
        };
        if let Some(v) = result {
            if ia.dst != Loc::None {
                mc.wr(ia.dst, v);
            }
        }
        if ins.op == TOp::Phi || result.is_some() {
            for &s in &ia.stores {
                let v = mc.rd(ir, ia.dst);
                mc.spill[usize::from(s)] = v;
            }
        }
        for r in &ia.post {
            let v = mc.spill[usize::from(r.slot)];
            mc.wr(r.reg, v);
        }
        pc += 1;
    }
}

// ---- the validity checker -------------------------------------------------------------------------------------------

struct Contents {
    g: [Option<Ref>; 11],
    x: [Option<Ref>; 16],
    s: Vec<Option<Ref>>,
}

impl Contents {
    fn get(&self, l: Loc) -> Option<Ref> {
        match l {
            Loc::G(n) => self.g[usize::from(n)],
            Loc::X(n) => self.x[usize::from(n)],
            Loc::Spill(n) => self.s[usize::from(n)],
            _ => None,
        }
    }
    fn set(&mut self, l: Loc, v: Option<Ref>) {
        match l {
            Loc::G(n) => self.g[usize::from(n)] = v,
            Loc::X(n) => self.x[usize::from(n)] = v,
            Loc::Spill(n) => self.s[usize::from(n)] = v,
            _ => {}
        }
    }
}

fn check_loc(ir: &TraceIr, l: Loc, r: Ref, what: &str) -> Result<(), String> {
    let x = ir.ty_of(r) == Ty::Num;
    match l {
        Loc::G(n) if !x && n < 11 && n != 7 => Ok(()),
        Loc::X(n) if x && n < 16 => Ok(()),
        Loc::Spill(n) if n < SCRATCH_SPILL => Ok(()),
        _ => Err(format!("{what}: bad location {l:?} for ref {r}")),
    }
}

/// Validity of an allocation, independent of the allocator's internals: simulates value identities through the
/// locations (two iterations of the loop).
/// A PHI whose value is also kept in extra spill slots (a TR27 rename of a pinned PHI) has one extra move per slot with
/// the same source as the move into its register / home. After the moves those slots hold the PHI too (the only
/// reader is a snapshot entry that names the PHI).
fn secondary_homes(
    c: &mut Contents,
    moves: &[Move],
    phis: &[usize],
    al: &Allocation,
    p: usize,
    src: Loc,
) {
    for m in moves {
        let primary = phis.iter().any(|&q| al.ins[q].dst == m.dst);
        if m.src == src && !primary && matches!(m.dst, Loc::Spill(_)) {
            c.set(m.dst, Some(rf(p)));
        }
    }
}

fn check(ir: &TraceIr, al: &Allocation, end_uses: &[Ref]) -> Result<(), String> {
    let n = ir.ins.len();
    let l = ir.loop_at.map(idx);
    let mut c = Contents {
        g: [None; 11],
        x: [None; 16],
        s: vec![None; 256],
    };
    // trace-wide register exclusions
    let has = |f: &dyn Fn(&Ins) -> bool| ir.ins.iter().any(f);
    let idiv = has(&|i| matches!(i.op, TOp::Idiv | TOp::Mod));
    let vshift = has(&|i| {
        matches!(i.op, TOp::Shl | TOp::Shr) && !(TraceIr::is_const(i.b) && ir.as_int(i.b).is_some())
    });
    let fx = has(&|i| matches!(i.op, TOp::Fneg | TOp::Fabs | TOp::F2iExact));
    let bad_reg = |l: Loc| match l {
        Loc::G(8) => idiv,
        Loc::G(6) => vshift,
        Loc::X(0) | Loc::X(1) => fx,
        _ => false,
    };
    for pass in 0..2 {
        if pass == 1 && l.is_none() {
            break;
        }
        let start = if pass == 0 { 0 } else { l.unwrap() + 1 };
        let mut pc = start;
        loop {
            if l.is_some() && pc == al.backedge_after && pass == 1 {
                // (only the legitimate placement is meaningful for the checker)
            }
            if pc == n {
                break;
            }
            if pass == 0 && Some(pc) == l {
                // entry moves: pre-roll values into the PHI locations
                let before = Contents {
                    g: c.g,
                    x: c.x,
                    s: c.s.clone(),
                };
                for m in &al.entry_seq {
                    let v = c.get(m.src);
                    c.set(m.dst, v);
                }
                let phis: Vec<usize> = (pc + 1..n)
                    .take_while(|&k| ir.ins[k].op == TOp::Phi)
                    .collect();
                for &p in &phis {
                    let d = al.ins[p].dst;
                    if d == Loc::None {
                        continue;
                    }
                    // the move sequence must have delivered the pre-roll value
                    let a = entry_ref(ir, p);
                    let src = al
                        .entry_moves
                        .iter()
                        .find(|m| m.dst == d)
                        .map(|m| m.src)
                        .unwrap_or(d);
                    let ok = if a < REF_BIAS {
                        matches!(src, Loc::Const(k) if k == a)
                    } else {
                        before.get(src) == Some(a)
                    };
                    if !ok {
                        return Err(format!(
                            "entry move into phi {p}: source {src:?} does not hold ref {a}"
                        ));
                    }
                    c.set(d, Some(rf(p)));
                    secondary_homes(&mut c, &al.entry_moves, &phis, al, p, src);
                }
            }
            let ins = ir.ins[pc];
            let ia = &al.ins[pc];
            for r in &ia.pre {
                if c.s[usize::from(r.slot)] != Some(r.r) {
                    return Err(format!(
                        "{pc}: pre reload of ref {} from unwritten slot {}",
                        r.r, r.slot
                    ));
                }
                c.set(r.reg, Some(r.r));
            }
            let refs = operand_refs(ir, &al.fused, pc);
            for (k, &r) in refs.iter().enumerate() {
                if r == NOREF {
                    if ia.ops[k] != Loc::None {
                        return Err(format!("{pc}: operand {k} located but absent"));
                    }
                    continue;
                }
                if r < REF_BIAS {
                    if ia.ops[k] != Loc::Const(r) {
                        return Err(format!("{pc}: constant operand {k} not Const"));
                    }
                    continue;
                }
                let loc = ia.ops[k];
                if !loc.is_reg() {
                    return Err(format!(
                        "{pc}: operand {k} (ref {r}) not in a register: {loc:?}"
                    ));
                }
                check_loc(ir, loc, r, "operand")?;
                if bad_reg(loc) {
                    return Err(format!("{pc}: operand in an excluded register {loc:?}"));
                }
                if c.get(loc) != Some(r) {
                    return Err(format!(
                        "{pc}: operand {k} ref {} expected in {loc:?}, found {:?}",
                        r,
                        c.get(loc)
                    ));
                }
            }
            if refs[0] != NOREF
                && refs[1] != NOREF
                && refs[0] != refs[1]
                && ia.dst.is_reg()
                && ia.ops[1] == ia.dst
            {
                return Err(format!(
                    "{pc}: second operand shares the destination register"
                ));
            }
            if ins.flags & F_GUARD != 0 {
                let s = aux_snap(ins.aux);
                for (e, &loc) in ir.snap_entries(s).iter().zip(&al.snap_locs[s as usize]) {
                    if e.r == NOREF {
                        continue;
                    }
                    if e.r < REF_BIAS {
                        if loc != Loc::Const(e.r) {
                            return Err(format!("{pc}: snapshot {s} constant entry not Const"));
                        }
                        continue;
                    }
                    check_loc(ir, loc, e.r, "snapshot")?;
                    if c.get(loc) != Some(e.r) {
                        return Err(format!(
                            "{pc}: snapshot {s} slot {} ref {} expected at {loc:?}, found {:?}",
                            e.slot,
                            e.r,
                            c.get(loc)
                        ));
                    }
                }
            }
            let defines = ia.dst != Loc::None;
            if ins.op == TOp::Phi {
                if ia.dst.is_reg() {
                    c.set(ia.dst, Some(rf(pc)));
                    check_loc(ir, ia.dst, rf(pc), "phi")?;
                }
                // a spilled PHI lives in its slot (set by the moves); stores copy the register
                for &s in &ia.stores {
                    c.s[usize::from(s)] = Some(rf(pc));
                }
            } else if defines {
                check_loc(ir, ia.dst, rf(pc), "dst")?;
                if !ia.dst.is_reg() || bad_reg(ia.dst) {
                    return Err(format!("{pc}: bad destination {:?}", ia.dst));
                }
                c.set(ia.dst, Some(rf(pc)));
                for &s in &ia.stores {
                    c.s[usize::from(s)] = Some(rf(pc));
                }
            } else if !ia.stores.is_empty() {
                return Err(format!("{pc}: stores without a value"));
            }
            for r in &ia.post {
                if c.s[usize::from(r.slot)] != Some(r.r) {
                    return Err(format!(
                        "{pc}: post reload of ref {} from unwritten slot {}",
                        r.r, r.slot
                    ));
                }
                c.set(r.reg, Some(r.r));
            }
            if matches!(ins.op, TOp::Calln | TOp::Tbar) {
                let cs = al.calls.iter().find(|s| s.at == pc);
                let Some(cs) = cs else {
                    return Err(format!("{pc}: call without a call site"));
                };
                if let Loc::G(d) = ia.dst {
                    if cs.live.gpr >> d & 1 != 0 {
                        return Err(format!("{pc}: call saves its own result register"));
                    }
                }
                if let Loc::X(d) = ia.dst {
                    if cs.live.xmm >> d & 1 != 0 {
                        return Err(format!("{pc}: call saves its own result register"));
                    }
                }
            } else if al.calls.iter().any(|s| s.at == pc) {
                return Err(format!("{pc}: call site on a non-call"));
            }
            pc += 1;
        }
        // end of the body
        for (&r, &loc) in end_uses.iter().zip(&al.end_locs) {
            if r < REF_BIAS {
                continue;
            }
            if c.get(loc) != Some(r) {
                return Err(format!(
                    "end use ref {r} expected at {loc:?}, found {:?}",
                    c.get(loc)
                ));
            }
        }
        if let Some(lp) = l {
            // back-edge: body values into the PHI locations; everything but the pre-roll values and the PHIs dies
            let phis: Vec<usize> = (lp + 1..n)
                .take_while(|&k| ir.ins[k].op == TOp::Phi)
                .collect();
            let before = Contents {
                g: c.g,
                x: c.x,
                s: c.s.clone(),
            };
            for m in &al.backedge_seq {
                let v = c.get(m.src);
                c.set(m.dst, v);
            }
            for &p in &phis {
                let b = ir.ins[p].b;
                let d = al.ins[p].dst;
                if d == Loc::None {
                    continue;
                }
                let src = al
                    .backedge_moves
                    .iter()
                    .find(|m| m.dst == d)
                    .map(|m| m.src)
                    .unwrap_or(d);
                let ok = if b < REF_BIAS {
                    matches!(src, Loc::Const(k) if k == b)
                } else {
                    before.get(src) == Some(b)
                };
                if !ok {
                    return Err(format!(
                        "back-edge move into phi {p}: source {src:?} does not hold ref {b}"
                    ));
                }
                c.set(d, Some(rf(p)));
                secondary_homes(&mut c, &al.backedge_moves, &phis, al, p, src);
            }
            for r in c.g.iter_mut().chain(c.x.iter_mut()).chain(c.s.iter_mut()) {
                if let Some(v) = *r {
                    let phi = v >= REF_BIAS && ir.ins[idx(v)].op == TOp::Phi;
                    if v >= REF_BIAS && idx(v) > lp && !phi {
                        *r = None;
                    }
                }
            }
        }
    }
    // the move sequences equal their simultaneous semantics
    if l.is_some() {
        for (par, seq) in [
            (&al.entry_moves, &al.entry_seq),
            (&al.backedge_moves, &al.backedge_seq),
        ] {
            check_moves(par, seq)?;
        }
    }
    Ok(())
}

fn rf(i: usize) -> Ref {
    REF_BIAS + i as Ref
}

// ---- the parallel-move oracle ---------------------------------------------------------------------------------------

fn locs_universe() -> Vec<Loc> {
    let mut v: Vec<Loc> = [0u8, 1, 2, 3, 4, 5, 6, 8]
        .iter()
        .map(|&n| Loc::G(n))
        .collect();
    v.extend((0..16).map(Loc::X));
    v.extend((0..10).map(Loc::Spill));
    v
}

/// Runs `seq` and compares with the simultaneous assignment `par`. `V7` and the scratch slot are free; steps that
/// go through `V7` in the assembler (memory to memory, constant into a double register or a slot) clobber it.
fn check_moves(par: &[Move], seq: &[Move]) -> Result<(), String> {
    // Q1: a double register that only the sequence names is the cycle scratch (free at the loop moves)
    let scr = seq.iter().find_map(|m| {
        [m.dst, m.src]
            .into_iter()
            .find(|l| matches!(l, Loc::X(_)) && !par.iter().any(|p| p.dst == *l || p.src == *l))
    });
    check_moves_scr(par, seq, scr)
}

/// [`check_moves`] where `scr` (a free double register) is clobbered.
fn check_moves_scr(par: &[Move], seq: &[Move], scr: Option<Loc>) -> Result<(), String> {
    let mut st: HashMap<Loc, u64> = HashMap::new();
    let mut next = 100u64;
    let mut all: Vec<Loc> = locs_universe();
    all.push(Loc::G(SCRATCH_GPR));
    all.push(Loc::Spill(SCRATCH_SPILL));
    for m in par.iter().chain(seq) {
        for l in [m.dst, m.src] {
            if !all.contains(&l) && !matches!(l, Loc::Const(_) | Loc::None) {
                all.push(l);
            }
        }
    }
    for &l in &all {
        st.insert(l, next);
        next += 1;
    }
    st.insert(Loc::G(SCRATCH_GPR), 0xAAAA);
    st.insert(Loc::Spill(SCRATCH_SPILL), 0xBBBB);
    let kval = |l: Loc| match l {
        Loc::Const(r) => 5000 + u64::from(r),
        _ => unreachable!(),
    };
    let rd = |st: &HashMap<Loc, u64>, l: Loc| match l {
        Loc::Const(_) => kval(l),
        _ => st[&l],
    };
    let mut want = st.clone();
    for m in par {
        if m.dst == Loc::None || m.src == Loc::None || m.dst == m.src {
            continue;
        }
        want.insert(m.dst, rd(&st, m.src));
    }
    let mut got = st.clone();
    for m in seq {
        if m.dst == Loc::None || m.src == Loc::None {
            return Err(format!("illegal step {m:?}"));
        }
        let v = rd(&got, m.src);
        got.insert(m.dst, v);
        let via_v7 = matches!(
            (m.dst, m.src),
            (Loc::Spill(_), Loc::Spill(_)) | (Loc::X(_) | Loc::Spill(_), Loc::Const(_))
        );
        if via_v7 {
            got.insert(Loc::G(SCRATCH_GPR), 0xDEAD);
        }
    }
    for &l in &all {
        if l == Loc::G(SCRATCH_GPR) || l == Loc::Spill(SCRATCH_SPILL) || Some(l) == scr {
            continue;
        }
        if got[&l] != want[&l] {
            return Err(format!(
                "{l:?}: got {}, want {} (par {par:?} seq {seq:?})",
                got[&l], want[&l]
            ));
        }
    }
    Ok(())
}

#[test]
fn parallel_moves_match_simultaneous_assignment() {
    let u = locs_universe();
    let mut rng = Rng::new(42);
    for case in 0..MOVE_CASES {
        let mut dsts: Vec<Loc> = Vec::new();
        let k = 1 + rng.below(12) as usize;
        while dsts.len() < k {
            let d = u[rng.below(u.len() as u64) as usize];
            if !dsts.contains(&d) {
                dsts.push(d);
            }
        }
        let mut moves: Vec<Move> = Vec::new();
        let shape = rng.below(4);
        if shape == 0 {
            // a permutation (cycles) of the destinations
            let mut srcs = dsts.clone();
            for i in (1..srcs.len()).rev() {
                let j = rng.below(i as u64 + 1) as usize;
                srcs.swap(i, j);
            }
            for (d, s) in dsts.iter().zip(&srcs) {
                moves.push(Move { dst: *d, src: *s });
            }
        } else {
            for d in &dsts {
                let r = rng.below(10);
                let src = if r < 5 {
                    dsts[rng.below(dsts.len() as u64) as usize] // duplicates and cycles
                } else if r < 8 {
                    u[rng.below(u.len() as u64) as usize]
                } else {
                    Loc::Const(1 + rng.below(4) as Ref)
                };
                moves.push(Move { dst: *d, src });
            }
        }
        let seq = sequentialise(&moves, SCRATCH_SPILL);
        if let Err(e) = check_moves(&moves, &seq) {
            panic!("case {case}: {e}");
        }
        // a general-register cycle without memory-to-memory moves uses V7, never the scratch slot
        if moves
            .iter()
            .all(|m| matches!(m.dst, Loc::G(_)) && matches!(m.src, Loc::G(_) | Loc::Const(_)))
        {
            assert!(
                !seq.iter().any(|m| m.dst == Loc::Spill(SCRATCH_SPILL)),
                "{moves:?} -> {seq:?}"
            );
        }
    }
}

#[test]
fn swap_cycles() {
    // swap two general registers, two double registers, and two slots
    for pair in [
        (Loc::G(0), Loc::G(1)),
        (Loc::X(2), Loc::X(3)),
        (Loc::Spill(0), Loc::Spill(1)),
    ] {
        let m = [
            Move {
                dst: pair.0,
                src: pair.1,
            },
            Move {
                dst: pair.1,
                src: pair.0,
            },
        ];
        let seq = sequentialise(&m, SCRATCH_SPILL);
        check_moves(&m, &seq).unwrap();
        assert!(seq.len() >= 3);
    }
    // a three-cycle plus a duplicate source and a constant
    let m = [
        Move {
            dst: Loc::G(0),
            src: Loc::G(1),
        },
        Move {
            dst: Loc::G(1),
            src: Loc::G(2),
        },
        Move {
            dst: Loc::G(2),
            src: Loc::G(0),
        },
        Move {
            dst: Loc::G(3),
            src: Loc::G(0),
        },
        Move {
            dst: Loc::G(4),
            src: Loc::Const(1),
        },
    ];
    let seq = sequentialise(&m, SCRATCH_SPILL);
    check_moves(&m, &seq).unwrap();
    assert!(seq.iter().any(|s| s.dst == Loc::G(SCRATCH_GPR)));
}

/// Q1: cycles of length 2, 3, 4 of general registers, double registers and mixed with slots are broken with `V7`,
/// never through the scratch slot (no memory-to-memory step in the list).
#[test]
fn cycles_of_every_length_use_v7() {
    let kinds: [fn(u8) -> Loc; 3] = [Loc::G, Loc::X, |i| {
        if i % 2 == 0 {
            Loc::X(i)
        } else {
            Loc::G(i)
        }
    }];
    for mk in kinds {
        for len in 2u8..=4 {
            let locs: Vec<Loc> = (0..len).map(&mk).collect();
            let m: Vec<Move> = (0..len as usize)
                .map(|i| Move {
                    dst: locs[i],
                    src: locs[(i + 1) % len as usize],
                })
                .collect();
            let seq = sequentialise(&m, SCRATCH_SPILL);
            check_moves(&m, &seq).unwrap();
            assert!(
                !seq.iter()
                    .any(|s| s.dst == Loc::Spill(SCRATCH_SPILL)
                        || s.src == Loc::Spill(SCRATCH_SPILL)),
                "{m:?} -> {seq:?}"
            );
            assert_eq!(seq.len(), usize::from(len) + 1, "{m:?} -> {seq:?}");
        }
    }
    // a double-register cycle with a slot hanging off it (no mem-to-mem step): still V7
    let m = [
        Move {
            dst: Loc::X(1),
            src: Loc::X(2),
        },
        Move {
            dst: Loc::X(2),
            src: Loc::X(1),
        },
        Move {
            dst: Loc::Spill(3),
            src: Loc::X(1),
        },
    ];
    let seq = sequentialise(&m, SCRATCH_SPILL);
    check_moves(&m, &seq).unwrap();
    assert!(!seq.iter().any(|s| s.dst == Loc::Spill(SCRATCH_SPILL)));
    // with a free double register: XMM cycles of length 2, 3, 4 use it (movaps), never V7 or memory
    for len in 2u8..=4 {
        let m: Vec<Move> = (0..len)
            .map(|i| Move {
                dst: Loc::X(i),
                src: Loc::X((i + 1) % len),
            })
            .collect();
        let seq = sequentialise_x(&m, SCRATCH_SPILL, Some(9));
        check_moves_scr(&m, &seq, Some(Loc::X(9))).unwrap();
        assert!(
            seq.iter()
                .all(|s| s.dst != Loc::G(SCRATCH_GPR) && s.src != Loc::G(SCRATCH_GPR)),
            "{seq:?}"
        );
        assert!(seq.iter().any(|s| s.dst == Loc::X(9)), "{seq:?}");
        assert_eq!(seq.len(), usize::from(len) + 1);
    }
    // ... and it replaces the scratch slot when V7 is busy (memory-to-memory step)
    let m = [
        Move {
            dst: Loc::G(1),
            src: Loc::G(2),
        },
        Move {
            dst: Loc::G(2),
            src: Loc::G(1),
        },
        Move {
            dst: Loc::Spill(3),
            src: Loc::Spill(4),
        },
    ];
    let seq = sequentialise_x(&m, SCRATCH_SPILL, Some(9));
    check_moves_scr(&m, &seq, Some(Loc::X(9))).unwrap();
    assert!(
        !seq.iter().any(|s| s.dst == Loc::Spill(SCRATCH_SPILL)),
        "{seq:?}"
    );
    // with a memory-to-memory step V7 is busy: the scratch slot is the only choice
    let m = [
        Move {
            dst: Loc::X(1),
            src: Loc::X(2),
        },
        Move {
            dst: Loc::X(2),
            src: Loc::X(1),
        },
        Move {
            dst: Loc::Spill(3),
            src: Loc::Spill(4),
        },
    ];
    let seq = sequentialise(&m, SCRATCH_SPILL);
    check_moves(&m, &seq).unwrap();
}

// ---- hand-built traces ----------------------------------------------------------------------------------------------

fn ins(op: TOp, t: Ty, flags: u8, a: Ref, b: Ref, aux: u32) -> Ins {
    Ins {
        op,
        t,
        flags,
        a,
        b,
        prev: NOREF,
        aux,
    }
}

fn model(slots: &[Val], nups: usize) -> Model {
    let mut m = Model::new();
    m.slots = slots.to_vec();
    m.closures = vec![vec![Val::NIL; nups]];
    m.max_iters = 3;
    m
}

/// Allocates (default and some random seeds), checks, and compares the lowering with the interpreter on `models`.
fn differential(ir: &TraceIr, models: &[Model], end_uses: &[Ref], seeds: u64) -> Vec<Allocation> {
    let mut out = Vec::new();
    for seed in std::iter::once(None).chain((0..seeds).map(Some)) {
        let opts = RaOpts {
            randra: seed,
            // P2 bundle 1: the seeds cycle through the extra-register sets (none, `rbp`, `r14`, both); the default allocation has none
            xregs: seed.map_or(0, |s| (s % 4) as u8),
            ..RaOpts::default()
        };
        let al = allocate(ir, end_uses, &opts)
            .unwrap_or_else(|e| panic!("allocate (seed {seed:?}): {e:?}"));
        if let Err(e) = check(ir, &al, end_uses) {
            panic!("checker (seed {seed:?}): {e}\n{}", dump(ir, &al));
        }
        for (mi, m0) in models.iter().enumerate() {
            let mut a = m0.clone();
            let mut b = m0.clone();
            let want = run(ir, &mut a).expect("interp");
            let got = lower_run(ir, &al, &mut b).unwrap_or_else(|e| panic!("lowering: {e}"));
            assert!(
                want == got && a == b,
                "seed {seed:?} model {mi}: interp {want:?} lowering {got:?}\nmodel {a:?}\nvs {b:?}\n{}",
                dump(ir, &al)
            );
        }
        out.push(al);
    }
    out
}

/// The P0 slice (docs/JIT_TRACE_REF.md): `s = s + i` with count and step; the register map is hand-checkable.
fn slice() -> TraceIr {
    let mut t = TraceIr::default();
    let k0 = t.kint(0);
    let k1 = t.kint(1);
    let s0 = t.snapshot(Snap::default(), &[]);
    let e = |slot: u16, r: Ref| SnapEntry { slot, r, flags: 0 };
    let b = REF_BIAS;
    let g = |op, a, bb, s: u32| ins(op, Ty::Int, F_GUARD, a, bb, guard_aux(s, 0));
    // pre-roll
    t.emit(ins(
        TOp::Budget,
        Ty::I64,
        F_GUARD,
        2,
        NOREF,
        guard_aux(s0, 0),
    )); // 0
    for slot in [4u16, 2, 0, 1] {
        t.emit(ins(
            TOp::Sload,
            Ty::Int,
            F_GUARD,
            slot,
            NOREF,
            guard_aux(s0, 1),
        )); // 1..4: s i count step
    }
    t.emit(ins(TOp::Add, Ty::Int, 0, b + 1, b + 2, 0)); // 5: s1
    let s1 = t.snapshot(Snap::default(), &[e(4, b + 5)]);
    t.emit(g(TOp::Ne, b + 3, k0, s1)); // 6
    t.emit(ins(TOp::Sub, Ty::Int, 0, b + 3, k1, 0)); // 7: count1
    t.emit(ins(TOp::Add, Ty::Int, 0, b + 2, b + 4, 0)); // 8: i1
    t.emit(ins(TOp::Loop, Ty::I64, 0, NOREF, NOREF, 0)); // 9
    for (a, bb) in [(5u16, 15u16), (8, 18), (7, 17)] {
        t.emit(ins(TOp::Phi, Ty::Int, F_PHI, b + a, b + bb, 0)); // 10..12
    }
    let sh = t.snapshot(Snap::default(), &[e(0, b + 12), e(2, b + 11), e(4, b + 10)]);
    t.emit(ins(
        TOp::Poll,
        Ty::Nil,
        F_GUARD,
        NOREF,
        NOREF,
        guard_aux(sh, 0),
    )); // 13
    t.emit(ins(
        TOp::Budget,
        Ty::I64,
        F_GUARD,
        2,
        NOREF,
        guard_aux(sh, 0),
    )); // 14
    t.emit(ins(TOp::Add, Ty::Int, 0, b + 10, b + 11, 0)); // 15: s2
    let s2 = t.snapshot(Snap::default(), &[e(0, b + 12), e(2, b + 11), e(4, b + 15)]);
    t.emit(g(TOp::Ne, b + 12, k0, s2)); // 16
    t.emit(ins(TOp::Sub, Ty::Int, 0, b + 12, k1, 0)); // 17
    t.emit(ins(TOp::Add, Ty::Int, 0, b + 11, b + 4, 0)); // 18
                                                         // observable results
    let cl = t.konst(KVal::Ptr(1, PtrKind::LClosure));
    for (j, v) in [(0i64, b + 15), (1, b + 17), (2, b + 18)] {
        let jk = t.kint(j);
        let u = t.emit(ins(TOp::Uref, Ty::P64, 0, cl, jk, 0));
        t.emit(ins(TOp::Ustore, Ty::Nil, 0, u, v, 0));
    }
    t.loop_at = Some(b + 9);
    t
}

#[test]
fn loop_slice() {
    let ir = slice();
    let mut models = Vec::new();
    for (s, i, c, st) in [
        (0, 1, 10, 1),
        (5, 3, 2, 7),
        (i64::MAX, 1, 3, 1),
        (0, 0, 1, 1),
        (1, 1, 0, 1),
    ] {
        let mut m = model(
            &[
                Val::int(c),
                Val::int(st),
                Val::int(i),
                Val::int(0),
                Val::int(s),
            ],
            3,
        );
        m.max_iters = 4;
        models.push(m);
        let mut m2 = models.last().unwrap().clone();
        m2.poll_exit = Some(2);
        models.push(m2);
        let mut m3 = models.last().unwrap().clone();
        m3.poll_exit = None;
        m3.budget = 5;
        models.push(m3);
    }
    let als = differential(&ir, &models, &[], SEEDS.min(200));
    let al = &als[0];
    assert!(
        al.nspill == 0,
        "the slice needs no spills\n{}",
        dump(&ir, al)
    );
    assert!(al.entry_seq.is_empty(), "no entry moves\n{}", dump(&ir, al));
    assert!(
        al.backedge_seq.is_empty(),
        "no back-edge moves\n{}",
        dump(&ir, al)
    );
    assert_eq!(al.backedge_after, ir.ins.len());
    assert!(al.forced_spills.is_empty());
}

fn swap_trace(num: bool) -> TraceIr {
    let mut t = TraceIr::default();
    let ty = if num { Ty::Num } else { Ty::Int };
    let s0 = t.snapshot(Snap::default(), &[]);
    let b = REF_BIAS;
    t.emit(ins(TOp::Sload, ty, F_GUARD, 0, NOREF, guard_aux(s0, 1))); // 0
    t.emit(ins(TOp::Sload, ty, F_GUARD, 1, NOREF, guard_aux(s0, 1))); // 1
    t.emit(ins(TOp::Loop, Ty::I64, 0, NOREF, NOREF, 0)); // 2
    t.emit(ins(TOp::Phi, ty, F_PHI, b, b + 4, 0)); // 3: p1 = PHI(x, p2)
    t.emit(ins(TOp::Phi, ty, F_PHI, b + 1, b + 3, 0)); // 4: p2 = PHI(y, p1)
    let e = |slot: u16, r: Ref| SnapEntry { slot, r, flags: 0 };
    let sh = t.snapshot(Snap::default(), &[e(0, b + 3), e(1, b + 4)]);
    t.emit(ins(
        TOp::Poll,
        Ty::Nil,
        F_GUARD,
        NOREF,
        NOREF,
        guard_aux(sh, 0),
    )); // 5
    let cl = t.konst(KVal::Ptr(1, PtrKind::LClosure));
    for (j, v) in [(0i64, b + 3), (1, b + 4)] {
        let jk = t.kint(j);
        let u = t.emit(ins(TOp::Uref, Ty::P64, 0, cl, jk, 0));
        t.emit(ins(TOp::Ustore, Ty::Nil, 0, u, v, 0));
    }
    t.loop_at = Some(b + 2);
    t
}

#[test]
fn phi_swap_cycles() {
    for num in [false, true] {
        let ir = swap_trace(num);
        let (x, y) = if num {
            (Val::num(1.5), Val::num(-2.5))
        } else {
            (Val::int(11), Val::int(22))
        };
        let mut m = model(&[x, y], 2);
        m.max_iters = 5;
        let mut m2 = m.clone();
        m2.poll_exit = Some(3);
        let als = differential(&ir, &[m, m2], &[], SEEDS.min(100));
        // the default allocation swaps two registers at the back-edge
        let al = &als[0];
        assert!(!al.backedge_seq.is_empty(), "{}", dump(&ir, al));
        // Q1: a double-register swap saves in a free double register (movaps), a general one in V7
        let save = al.backedge_seq[0];
        let ok = if num {
            matches!((save.dst, save.src), (Loc::X(_), Loc::X(_)))
        } else {
            save.dst == Loc::G(SCRATCH_GPR)
        };
        assert!(ok, "{}", dump(&ir, al));
    }
}

/// Many simultaneously live integers: evictions, reloads, stores (and, for 300 snapshot-only refs, `Compile(spill)`).
fn pressure_trace(nvals: usize, snapshot_only: bool) -> (TraceIr, Vec<Model>) {
    let mut t = TraceIr::default();
    let s0 = t.snapshot(Snap::default(), &[]);
    let b = REF_BIAS;
    t.emit(ins(
        TOp::Sload,
        Ty::Int,
        F_GUARD,
        0,
        NOREF,
        guard_aux(s0, 1),
    ));
    t.emit(ins(
        TOp::Sload,
        Ty::Num,
        F_GUARD,
        1,
        NOREF,
        guard_aux(s0, 1),
    ));
    let mut refs = Vec::new();
    for i in 0..nvals {
        let k = t.kint(1000 + i as i64);
        if i % 3 == 2 {
            refs.push(t.emit(ins(TOp::Fadd, Ty::Num, 0, b + 1, b + 1, 0)));
        } else {
            refs.push(t.emit(ins(TOp::Add, Ty::Int, 0, b, k, 0)));
        }
    }
    let entries: Vec<SnapEntry> = refs
        .iter()
        .enumerate()
        .map(|(i, &r)| SnapEntry {
            slot: i as u16,
            r,
            flags: 0,
        })
        .collect();
    let s1 = t.snapshot(Snap::default(), &entries);
    let kz = t.kint(0);
    if snapshot_only {
        t.emit(ins(TOp::Eq, Ty::Int, F_GUARD, b, kz, guard_aux(s1, 0)));
    } else {
        // use every value again, in reverse, so that they are live together
        let cl = t.konst(KVal::Ptr(1, PtrKind::LClosure));
        for (i, &r) in refs.iter().enumerate().rev() {
            let jk = t.kint((i % 4) as i64);
            let u = t.emit(ins(TOp::Uref, Ty::P64, 0, cl, jk, 0));
            if i % 3 == 2 {
                let h = t.emit(ins(TOp::Fmul, Ty::Num, 0, r, b + 1, 0));
                t.emit(ins(TOp::Ustore, Ty::Nil, 0, u, h, 0));
            } else {
                t.emit(ins(TOp::Ustore, Ty::Nil, 0, u, r, 0));
            }
            if i % 5 == 0 {
                t.emit(ins(TOp::Eq, Ty::Int, F_GUARD, b, kz, guard_aux(s1, 0)));
            }
        }
    }
    let mut ms = Vec::new();
    for x in [0i64, 7, -3] {
        let mut m = model(&[Val::int(x), Val::num(1.25)], 4);
        m.closures[0] = vec![Val::NIL; 4];
        ms.push(m);
    }
    (t, ms)
}

#[test]
fn pressure() {
    let (ir, ms) = pressure_trace(40, false);
    let als = differential(&ir, &ms, &[], SEEDS.min(300));
    let al = &als[0];
    assert!(al.nspill > 0, "40 live values must spill");
    assert!(
        al.ins.iter().any(|i| !i.post.is_empty()),
        "evictions reload"
    );
}

#[test]
fn spill_limit_is_compile_spill() {
    let (ir, _) = pressure_trace(300, true);
    let r = allocate(&ir, &[], &RaOpts::default());
    assert_eq!(r.err(), Some(RaError::Spill));
    let (ir, ms) = pressure_trace(120, true);
    let als = differential(&ir, &ms, &[], 0);
    assert!(
        als[0].nspill > 50,
        "snapshot-only values go to slots when registers run out"
    );
    assert!(als[0].weak.iter().filter(|&&w| w).count() >= 120);
}

#[test]
fn too_many_phis_is_compile_phi() {
    let mut t = TraceIr::default();
    let s0 = t.snapshot(Snap::default(), &[]);
    let b = REF_BIAS;
    t.emit(ins(
        TOp::Sload,
        Ty::Int,
        F_GUARD,
        0,
        NOREF,
        guard_aux(s0, 1),
    ));
    t.emit(ins(TOp::Loop, Ty::I64, 0, NOREF, NOREF, 0));
    for _ in 0..65 {
        t.emit(ins(TOp::Phi, Ty::Int, F_PHI, b, b, 0));
    }
    t.loop_at = Some(b + 1);
    assert_eq!(
        allocate(&t, &[], &RaOpts::default()).err(),
        Some(RaError::Phi)
    );
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    let mut t = TraceIr::default();
    let s0 = t.snapshot(Snap::default(), &[]);
    t.emit(ins(
        TOp::Sload,
        Ty::Int,
        F_GUARD,
        0,
        NOREF,
        guard_aux(s0, 1),
    ));
    t.emit(ins(TOp::Add, Ty::Int, 0, REF_BIAS + 5, REF_BIAS, 0));
    assert!(matches!(
        allocate(&t, &[], &RaOpts::default()),
        Err(RaError::BadIr(_))
    ));
    let mut t = TraceIr::default();
    t.emit(ins(TOp::Phi, Ty::Int, F_PHI, 1, 1, 0));
    assert!(matches!(
        allocate(&t, &[], &RaOpts::default()),
        Err(RaError::BadIr(_))
    ));
}

#[test]
fn calls_and_fusion_are_reported() {
    // Calln is a call site; the Aref of an array access is fused into its Aload / Astore
    let mut found_call = false;
    let mut found_fused = false;
    for seed in 0..60 {
        let mut rng = Rng::new(seed);
        let case = random_case(
            &mut rng,
            GenCfg {
                size: SIZE,
                loops: true,
            },
        );
        let al = allocate(&case.ir, &[], &RaOpts::default()).unwrap();
        for (i, x) in case.ir.ins.iter().enumerate() {
            if x.op == TOp::Calln {
                assert!(al.calls.iter().any(|c| c.at == i));
                found_call = true;
            }
            if x.op == TOp::Aref {
                assert!(al.fused[i], "the generator's Aref feeds a memory op only");
                assert_eq!(al.ins[i].dst, Loc::None);
                found_fused = true;
            }
        }
        assert!(al.calls.windows(2).all(|w| w[0].at < w[1].at));
    }
    assert!(found_call && found_fused);
}

// ---- property tests -------------------------------------------------------------------------------------------------

fn property(cfg: GenCfg, cases: u64, seeds: u64, shared: bool, end_uses: bool) {
    let mut loops = 0;
    let mut spills = 0;
    let mut forced = 0;
    for case_no in 0..cases {
        let mut rng = Rng::new(0x5A5A_0000 + case_no);
        let mut case = random_case(&mut rng, cfg);
        if shared {
            share_snapshots(&mut case.ir, &mut rng);
        }
        let mut ends = Vec::new();
        if end_uses {
            let vals: Vec<Ref> = (0..case.ir.ins.len())
                .filter(|&i| {
                    matches!(case.ir.ins[i].t, Ty::Int | Ty::Num)
                        && case.ir.ins[i].op != TOp::Phi
                        && !case.ir.ins[i].op.always_guard()
                        && !matches!(
                            case.ir.ins[i].op,
                            TOp::Astore | TOp::Ustore | TOp::Tbar | TOp::Loop
                        )
                })
                .map(rf)
                .collect();
            for _ in 0..rng.below(4) {
                ends.push(vals[rng.below(vals.len() as u64) as usize]);
            }
        }
        let als = differential(&case.ir, &case.models, &ends, seeds);
        loops += usize::from(case.ir.loop_at.is_some());
        spills += usize::from(als[0].nspill > 0);
        forced += als.iter().map(|a| a.forced_spills.len()).sum::<usize>();
    }
    eprintln!(
        "property: {cases} cases, {loops} with loops, {spills} spilling, {forced} forced spills"
    );
    if cases >= 100 && cfg.loops {
        assert!(
            loops > cases as usize / 3,
            "the generator must produce loops"
        );
    }
}

#[test]
fn allocation_matches_interpreter() {
    property(
        GenCfg {
            size: SIZE,
            loops: true,
        },
        CASES,
        3,
        false,
        false,
    );
}

#[test]
fn allocation_matches_interpreter_straight_line() {
    property(
        GenCfg {
            size: SIZE,
            loops: false,
        },
        CASES,
        3,
        false,
        false,
    );
}

#[test]
fn shared_snapshots_keep_exit_maps_identical() {
    property(
        GenCfg {
            size: SIZE,
            loops: true,
        },
        CASES,
        4,
        true,
        false,
    );
}

#[test]
fn end_uses_are_available_at_the_end() {
    property(
        GenCfg {
            size: SIZE,
            loops: true,
        },
        CASES / 2 + 1,
        2,
        true,
        true,
    );
}

#[test]
fn big_traces_under_pressure() {
    let size = if cfg!(miri) { 30 } else { 150 };
    property(
        GenCfg { size, loops: true },
        if cfg!(miri) { 1 } else { 30 },
        4,
        true,
        true,
    );
}

/// `+randra` over many seeds on a few cases (the same property tests must pass for any valid allocation).
#[test]
fn randra_seeds() {
    let mut forced = 0;
    for case_no in 0..if cfg!(miri) { 1 } else { 4 } {
        let mut rng = Rng::new(0xFEED + case_no);
        let mut case = random_case(
            &mut rng,
            GenCfg {
                size: SIZE,
                loops: true,
            },
        );
        share_snapshots(&mut case.ir, &mut rng);
        let models = &case.models[..2];
        for seed in 0..SEEDS {
            let opts = RaOpts {
                randra: Some(seed),
                xregs: (seed % 4) as u8,
                ..RaOpts::default()
            };
            let al = allocate(&case.ir, &[], &opts).expect("allocate");
            if let Err(e) = check(&case.ir, &al, &[]) {
                panic!("case {case_no} seed {seed}: {e}\n{}", dump(&case.ir, &al));
            }
            for (mi, m0) in models.iter().enumerate() {
                let (mut a, mut b) = (m0.clone(), m0.clone());
                let want = run(&case.ir, &mut a).unwrap();
                let got = lower_run(&case.ir, &al, &mut b).unwrap();
                assert!(
                    want == got && a == b,
                    "case {case_no} seed {seed} model {mi}\n{}",
                    dump(&case.ir, &al)
                );
            }
            forced += al.forced_spills.len();
        }
    }
    eprintln!("randra: {forced} forced spills (TR27 renames) across the seeds");
}

#[test]
fn randra_option_parses_into_ra_opts() {
    let o = rn_jit::tr::TraceOpts::parse("+randra=17").unwrap();
    assert_eq!(RaOpts::from_trace(&o).randra, Some(17));
    assert_eq!(RaOpts::from_trace(&Default::default()).randra, None);
}

// ---- PIa: `KPTR` with `b = NOREF`, `RaOpts::reserve_gpr` -------------------------------------------------------------

/// The recorder's canonical constant instruction (`b = NOREF`) is accepted; it needs no register operand.
#[test]
fn kptr_with_noref_operand_is_accepted() {
    let mut t = TraceIr::default();
    let s0 = t.snapshot(Snap::default(), &[]);
    let k = t.konst(KVal::Ptr(0x1000, PtrKind::Raw));
    let ki = t.kint(7);
    t.emit(ins(
        TOp::Sload,
        Ty::Int,
        F_GUARD,
        0,
        NOREF,
        guard_aux(s0, 1),
    ));
    let p = t.emit(ins(TOp::Kptr, Ty::ShrStr, 0, k, NOREF, 0));
    let n = t.emit(ins(TOp::Kint, Ty::Int, 0, ki, NOREF, 0));
    assert_eq!((p, n), (rf(1), rf(2)));
    t.emit(ins(TOp::Add, Ty::Int, 0, rf(0), n, 0));
    let al = allocate(&t, &[], &RaOpts::default()).expect("KPTR / KINT with b = NOREF");
    check(&t, &al, &[]).unwrap();
    // operands of a constant instruction: only `a`
    assert_eq!(operand_refs(&t, &al.fused, 1), [k, NOREF, NOREF]);
    // the interpreter and the lowering agree
    let m = model(&[Val::int(3)], 0);
    differential(&t, &[m], &[], 2);
    // a genuinely missing operand elsewhere is still an error
    let mut bad = t.clone();
    bad.ins[3].b = NOREF;
    assert!(matches!(
        allocate(&bad, &[], &RaOpts::default()),
        Err(RaError::BadIr(_))
    ));
}

/// A reserved register is never assigned (property over random traces, default and `+randra` allocations); the
/// allocation stays valid and equal to the interpreter.
#[test]
fn reserved_register_is_never_assigned() {
    let cases = if cfg!(miri) { 2 } else { 120 };
    for reserve in [BUDGET_GPR, 8, 0] {
        for loops in [true, false] {
            for case_no in 0..cases {
                let mut rng = Rng::new(0xB0D6_0000 + case_no);
                let case = random_case(&mut rng, GenCfg { size: SIZE, loops });
                for seed in [None, Some(case_no), Some(case_no ^ 0x77)] {
                    let opts = RaOpts {
                        randra: seed,
                        reserve_gpr: Some(reserve),
                        ..RaOpts::default()
                    };
                    let al = allocate(&case.ir, &[], &opts)
                        .unwrap_or_else(|e| panic!("reserve V{reserve} seed {seed:?}: {e:?}"));
                    assert!(
                        !al.uses_gpr(reserve),
                        "V{reserve} assigned (seed {seed:?})\n{}",
                        dump(&case.ir, &al)
                    );
                    check(&case.ir, &al, &[]).unwrap_or_else(|e| panic!("checker: {e}"));
                    for m0 in &case.models {
                        let (mut a, mut b) = (m0.clone(), m0.clone());
                        let want = run(&case.ir, &mut a).expect("interp");
                        let got = lower_run(&case.ir, &al, &mut b).expect("lowering");
                        assert!(want == got && a == b, "seed {seed:?}: interp vs lowering");
                    }
                }
            }
        }
    }
    // an unallocatable register is refused
    for r in [7u8, 9, 200] {
        let t = swap_trace(false);
        let o = RaOpts {
            reserve_gpr: Some(r),
            ..RaOpts::default()
        };
        assert!(
            matches!(allocate(&t, &[], &o), Err(RaError::BadIr(_))),
            "V{r}"
        );
    }
}
