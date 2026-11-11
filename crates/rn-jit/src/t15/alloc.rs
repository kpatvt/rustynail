//! Register assignment of a region (docs/JIT_TIER15.md 4.1, 4.3; as built: section 16).
//!
//! Pure functions (no VM state, no code emission), so they run under Miri:
//!
//! * [`choose`]: which Lua registers ("slots") get a machine register, by loop-depth weight. A slot has **one**
//!   register for the whole region (decision 15.3: no interval splitting in the INT slice); the others are
//!   *slot-resident* (their VM stack slot is their only home).
//! * [`solve_rr`]: the forward must-dataflow of the *register-resident set* `RR(pc)` = the slots whose current
//!   value lives in their register (memory is stale). It is a pure function of the successor relation, the
//!   per-instruction effects and the stub load sets, so the code generator can emit every edge with its exact
//!   state.

use super::SlotSet;
use crate::ir::{V, X};

/// Machine registers region code may assign to Lua registers: `V0..V4` = `rsi rdi r8 r9 r10`.
pub const NALLOC: usize = 5;
/// First allocatable double register: `X2..X15` (`X0`/`X1` are the scratch of region code).
pub const XFIRST: X = 2;
/// Number of double registers region code may assign (`xmm2..xmm15`).
pub const NALLOC_X: usize = 14;

/// `slot -> register` of the slots that live in a register while they are available.
#[derive(Clone, Debug)]
pub struct Assign {
    /// `reg[s]`: the integer register of slot `s` (`None` = no integer register).
    pub reg: Vec<Option<V>>,
    /// `xreg[s]`: the double register of slot `s`. A slot has at most one of `reg` and `xreg` (T4b: one class per
    /// slot for the whole region); a slot with neither is slot-resident.
    pub xreg: Vec<Option<X>>,
}

impl Assign {
    /// No registers at all (`-ra`).
    pub fn none(nslots: usize) -> Assign {
        Assign {
            reg: vec![None; nslots],
            xreg: vec![None; nslots],
        }
    }
    /// Slots that have an integer register.
    pub fn gslots(&self) -> SlotSet {
        let mut s = SlotSet::EMPTY;
        for (i, r) in self.reg.iter().enumerate() {
            if r.is_some() {
                s.insert(i as u32);
            }
        }
        s
    }
    /// Slots that have a double register.
    pub fn xslots(&self) -> SlotSet {
        let mut s = SlotSet::EMPTY;
        for (i, r) in self.xreg.iter().enumerate() {
            if r.is_some() {
                s.insert(i as u32);
            }
        }
        s
    }
    /// Slots that have a register of either class.
    pub fn slots(&self) -> SlotSet {
        let mut s = self.gslots();
        s.union_with(&self.xslots());
        s
    }
    /// Integer register of `s`.
    pub fn of(&self, s: u32) -> Option<V> {
        self.reg.get(s as usize).copied().flatten()
    }
    /// Double register of `s`.
    pub fn xof(&self, s: u32) -> Option<X> {
        self.xreg.get(s as usize).copied().flatten()
    }
}

/// xorshift64* (the `+randra=SEED` stream).
pub struct Rng(u64);

impl Rng {
    /// Seeded generator (a zero seed is remapped).
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    /// Next value.
    pub fn step(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.step() % n as u64) as usize
    }
}

/// Chooses the registers. `weight[s]` is the sum over the uses and defs of slot `s` as an integer operand of
/// `10^loop depth` (0 = never used that way: not a candidate). At most `max_regs` slots (<= [`NALLOC`]) get a
/// register, the heaviest first (ties: lower slot number first). With `randra` the order is shuffled and a random
/// number of registers (possibly none) is handed out in a random order.
pub fn choose(weight: &[u64], max_regs: usize, randra: Option<u64>) -> Assign {
    let n = weight.len();
    let mut a = Assign::none(n);
    let mut cands: Vec<usize> = (0..n).filter(|&s| weight[s] > 0).collect();
    let mut regs: Vec<V> = (0..max_regs.min(NALLOC) as V).collect();
    let mut k = regs.len();
    match randra {
        None => cands.sort_by_key(|&s| (std::cmp::Reverse(weight[s]), s)),
        Some(seed) => {
            let mut r = Rng::new(seed);
            for i in (1..cands.len()).rev() {
                cands.swap(i, r.below(i + 1));
            }
            for i in (1..regs.len()).rev() {
                regs.swap(i, r.below(i + 1));
            }
            // "random refusal of up to half of the registers"
            k = regs.len() - r.below(regs.len() / 2 + 1).min(regs.len());
            if r.below(8) == 0 {
                k = 0;
            }
        }
    }
    for (s, v) in cands.into_iter().zip(regs.into_iter().take(k)) {
        a.reg[s] = Some(v);
    }
    a
}

/// Registers for both classes (T4b). `wi[s]` / `wf[s]` are the integer / double operand weights of slot `s`; a slot
/// takes the class with the larger weight (ties: integer) and competes only there. Integers get at most
/// `max_g` of `V0..V4`, doubles at most `max_x` of `X2..X15`.
pub fn choose_all(
    wi: &[u64],
    wf: &[u64],
    max_g: usize,
    max_x: usize,
    randra: Option<u64>,
) -> Assign {
    let n = wi.len();
    let gi: Vec<u64> = (0..n)
        .map(|s| if wi[s] >= wf[s] { wi[s] } else { 0 })
        .collect();
    let fl: Vec<u64> = (0..n)
        .map(|s| if wf[s] > wi[s] { wf[s] } else { 0 })
        .collect();
    let mut a = choose(&gi, max_g, randra);
    // doubles: the same procedure over the register numbers X2..X15
    let mut cands: Vec<usize> = (0..n).filter(|&s| fl[s] > 0).collect();
    let mut regs: Vec<X> = (XFIRST..XFIRST + max_x.min(NALLOC_X) as X).collect();
    let mut k = regs.len();
    match randra {
        None => cands.sort_by_key(|&s| (std::cmp::Reverse(fl[s]), s)),
        Some(seed) => {
            let mut r = Rng::new(seed ^ 0xA5A5_5A5A_1234_8765);
            for i in (1..cands.len()).rev() {
                cands.swap(i, r.below(i + 1));
            }
            for i in (1..regs.len()).rev() {
                regs.swap(i, r.below(i + 1));
            }
            if !regs.is_empty() {
                k = regs.len() - r.below(regs.len() / 2 + 1).min(regs.len());
                if r.below(8) == 0 {
                    k = 0;
                }
            }
        }
    }
    for (s, x) in cands.into_iter().zip(regs.into_iter().take(k)) {
        a.xreg[s] = Some(x);
    }
    a
}

/// One node of the region graph for [`solve_rr`].
#[derive(Clone, Debug, Default)]
pub struct Node {
    /// Reached by region code (the others are never visited).
    pub live: bool,
    /// Slots whose register is read as an integer operand (loaded from memory when not available).
    pub ruse: SlotSet,
    /// Slots defined as integers in their register.
    pub rdef: SlotSet,
    /// Slots overwritten in memory (leave the set).
    pub mdef: SlotSet,
    /// A fallback: everything is flushed, the successors are entered through their stubs.
    pub fall: bool,
    /// Successor pcs (region-relative index), for a fallback all of them.
    pub succs: Vec<usize>,
}

/// `RR` after a straight-line effect: `(rr + (ruse + rdef) & assigned) - mdef`.
pub fn transfer(rr: SlotSet, n: &Node, assigned: &SlotSet) -> SlotSet {
    let mut r = rr;
    let mut add = n.ruse;
    add.union_with(&n.rdef);
    r.union_with(&add.meet(assigned));
    r.minus(&n.mdef)
}

/// The register-resident sets: `rr_in[k]` is the state at the start of node `k` (`None`: unreached). Entries are
/// the nodes in `stub_entered` (their stub loads `load(k)`), every fallback successor is entered through its
/// stub as well. Join = intersection (a slot is register-resident at a join only if it is on every path), so the
/// result is the greatest fixpoint reached from above by monotone shrinking.
pub fn solve_rr(
    nodes: &[Node],
    assigned: &SlotSet,
    stub_entered: &[usize],
    load: &dyn Fn(usize) -> SlotSet,
) -> Vec<Option<SlotSet>> {
    let mut rr_in: Vec<Option<SlotSet>> = vec![None; nodes.len()];
    let merge = |rr_in: &mut Vec<Option<SlotSet>>, k: usize, s: SlotSet| -> bool {
        if k >= nodes.len() || !nodes[k].live {
            return false;
        }
        match rr_in[k] {
            None => {
                rr_in[k] = Some(s);
                true
            }
            Some(o) => {
                let m = if super::mutants::active("rr_join_union") {
                    // (T8 mutant) join = union: a slot is register-resident after a join if it is on SOME path
                    let mut u = o;
                    u.union_with(&s);
                    u
                } else {
                    o.meet(&s)
                };
                rr_in[k] = Some(m);
                m != o
            }
        }
    };
    for &k in stub_entered {
        let l = load(k);
        merge(&mut rr_in, k, l);
    }
    loop {
        let mut changed = false;
        for k in 0..nodes.len() {
            let n = &nodes[k];
            let Some(inn) = rr_in[k] else { continue };
            if !n.live {
                continue;
            }
            if n.fall {
                for &s in &n.succs {
                    let l = load(s);
                    changed |= merge(&mut rr_in, s, l);
                }
            } else {
                let out = transfer(inn, n, assigned);
                for &s in &n.succs {
                    changed |= merge(&mut rr_in, s, out);
                }
            }
        }
        if !changed {
            return rr_in;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(v: &[u32]) -> SlotSet {
        let mut s = SlotSet::EMPTY;
        for &x in v {
            s.insert(x);
        }
        s
    }

    #[test]
    fn choose_takes_the_heaviest() {
        let w = [0, 10, 100, 1, 1000, 5, 7, 0];
        let a = choose(&w, 5, None);
        assert_eq!(a.slots(), set(&[1, 2, 4, 5, 6]));
        assert_eq!(a.of(4), Some(0));
        assert_eq!(a.of(2), Some(1));
        let a = choose(&w, 2, None);
        assert_eq!(a.slots(), set(&[2, 4]));
        assert_eq!(choose(&w, 0, None).slots(), SlotSet::EMPTY);
    }

    #[test]
    fn randra_is_deterministic_and_valid() {
        let w = [3u64; 12];
        for seed in 0..50 {
            let a = choose(&w, 5, Some(seed));
            let b = choose(&w, 5, Some(seed));
            assert_eq!(a.reg, b.reg);
            let mut used: Vec<V> = a.reg.iter().flatten().copied().collect();
            let n = used.len();
            used.sort_unstable();
            used.dedup();
            assert_eq!(used.len(), n, "a register was handed out twice");
            assert!(n <= 5);
        }
    }

    // a loop: 0 (head: uses/defs slot 1) -> 1 (branch) -> {0, 2}; 2 is the exit (not a node)
    #[test]
    fn rr_loop_keeps_loop_carried_values() {
        let mk = |ruse: &[u32], rdef: &[u32], succs: &[usize]| Node {
            live: true,
            ruse: set(ruse),
            rdef: set(rdef),
            mdef: SlotSet::EMPTY,
            fall: false,
            succs: succs.to_vec(),
        };
        let nodes = vec![mk(&[1], &[1], &[1]), mk(&[], &[], &[0])];
        let asg = set(&[1, 2]);
        let rr = solve_rr(&nodes, &asg, &[0], &|_| set(&[1]));
        assert_eq!(rr[0], Some(set(&[1])));
        assert_eq!(rr[1], Some(set(&[1])));
    }

    // a value defined on one branch only is not register-resident at the join
    #[test]
    fn rr_join_intersects() {
        let n = |rdef: &[u32], succs: &[usize]| Node {
            live: true,
            rdef: set(rdef),
            succs: succs.to_vec(),
            ..Node::default()
        };
        // 0: branch -> 1 | 2 ; 1 defines slot 3 -> 3 ; 2 -> 3 ; 3 join
        let nodes = vec![n(&[], &[1, 2]), n(&[3], &[3]), n(&[], &[3]), n(&[], &[])];
        let rr = solve_rr(&nodes, &set(&[3]), &[0], &|_| SlotSet::EMPTY);
        assert_eq!(rr[1], Some(SlotSet::EMPTY));
        assert_eq!(rr[3], Some(SlotSet::EMPTY));
    }

    #[test]
    fn rr_fallback_resets_to_the_load_set() {
        let nodes = vec![
            Node {
                live: true,
                rdef: set(&[1]),
                succs: vec![1],
                ..Node::default()
            },
            Node {
                live: true,
                fall: true,
                succs: vec![2],
                ..Node::default()
            },
            Node {
                live: true,
                ..Node::default()
            },
        ];
        let rr = solve_rr(&nodes, &set(&[1, 2]), &[0], &|k| {
            if k == 2 {
                set(&[2])
            } else {
                SlotSet::EMPTY
            }
        });
        assert_eq!(rr[1], Some(set(&[1])));
        assert_eq!(rr[2], Some(set(&[2])));
    }
}
