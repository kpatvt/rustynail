//! Register liveness of a prototype's bytecode (F4 snapshot pruning, docs/JIT_TRACE.md TR3 / D7).
//!
//! [`live_in`] computes, per instruction, the set of registers that may be read on some path from the instruction before
//! being written (a backward dataflow over the bytecode CFG). Everything the analysis cannot describe exactly is
//! over-approximated: instructions that read an open range (`CALL B=0`, `RETURN B=0`, `SETLIST B=0`, ...), generic-`for`
//! instructions, `CLOSE`, `VARARGPREP` and unknown opcodes read every register from `A` up (or all); definitions are only
//! recorded where they are unconditional, so a missed definition keeps a register live. A prototype with `TBC`, a jump
//! out of the code, or a CFG the analysis cannot follow has no result (`None`: nothing is pruned).
//!
//! [`prunable`] turns the liveness into the sets a trace snapshot may drop at each pc: the dead *temporaries* (registers
//! at or above the active locals around the pc, never a register an inner closure captures).

use rn_core::func::Proto;
use rn_core::instruction::Instruction;
use rn_core::ver::active::OpCode;

/// A set of registers `0..256`.
pub type Set = [u64; 4];

const EMPTY: Set = [0; 4];

fn one(r: u32) -> Set {
    let mut s = EMPTY;
    s[(r as usize >> 6) & 3] |= 1 << (r & 63);
    s
}

/// Registers `a ..= a + n - 1` (clamped to 256).
fn range(a: u32, n: u32) -> Set {
    let mut s = EMPTY;
    for r in a..(a + n).min(256) {
        s[(r as usize) >> 6] |= 1 << (r & 63);
    }
    s
}

/// Registers `a ..`.
fn from(a: u32) -> Set {
    range(a, 256u32.saturating_sub(a))
}

fn or(a: Set, b: Set) -> Set {
    [a[0] | b[0], a[1] | b[1], a[2] | b[2], a[3] | b[3]]
}

fn minus(a: Set, b: Set) -> Set {
    [a[0] & !b[0], a[1] & !b[1], a[2] & !b[2], a[3] & !b[3]]
}

/// Section 76.9: is the set empty?
#[inline]
pub fn is_empty(s: &Set) -> bool {
    s.iter().all(|&w| w == 0)
}

/// Is register `r` in `s`?
pub fn has(s: &Set, r: u32) -> bool {
    r < 256 && s[(r as usize) >> 6] & (1 << (r & 63)) != 0
}

/// Deliberate faults of the analysis, for the mutant runner (all false in production).
#[derive(Clone, Copy, Debug, Default)]
pub struct Faults {
    /// Jumps backwards do not count as successors (loop-carried registers look dead).
    pub no_backedges: bool,
    /// Open-ended reads (`B == 0` of `CALL`, `RETURN`, `TAILCALL`, `SETLIST`) read nothing.
    pub open_empty: bool,
    /// Liveness is ignored: every register is dead.
    pub ignore_live: bool,
    /// Every register is a temporary: the active locals are not protected.
    pub ignore_locals: bool,
    /// B2 (`opencall`, not a fault): a `CALL` with `C == 0` defines every register from `A` up.
    pub open_call_defs: bool,
    /// Mutant `opencall_defs_low` (with `open_call_defs`): the open-result `CALL` defines every register from `A - 2` up (kills the pending
    /// operand of an enclosing expression).
    pub open_defs_low: bool,
}

/// The effect of one instruction: registers read, registers unconditionally written, and the successors.
struct Eff {
    uses: Set,
    defs: Set,
    succ: [Option<i64>; 2],
}

fn is_arith(op: OpCode) -> bool {
    use OpCode::*;
    matches!(
        op,
        Addi | Addk
            | Subk
            | Mulk
            | Modk
            | Powk
            | Divk
            | Idivk
            | Bandk
            | Bork
            | Bxork
            | Shli
            | Shri
            | Add
            | Sub
            | Mul
            | Mod
            | Pow
            | Div
            | Idiv
            | Band
            | Bor
            | Bxor
            | Shl
            | Shr
    )
}

fn is_mmbin(op: Option<OpCode>) -> bool {
    matches!(op, Some(OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink))
}

/// `None`: the proto has an instruction the analysis refuses (`TBC`).
fn effect(code: &[u32], pc: usize, f: Faults) -> Option<Eff> {
    use OpCode::*;
    let i = Instruction(code[pc]);
    let (a, b, c) = (i.a(), i.b(), i.c());
    let next = pc as i64 + 1;
    let mut e = Eff {
        uses: EMPTY,
        defs: EMPTY,
        succ: [Some(next), None],
    };
    let Some(op) = i.opcode() else {
        e.uses = from(0);
        return Some(e);
    };
    // `RK(C)`: a register when `k` is clear
    let open = |a: u32| if f.open_empty { EMPTY } else { from(a) };
    let rk_c = |i: Instruction| if i.k() == 0 { one(i.c()) } else { EMPTY };
    match op {
        Move => {
            e.uses = one(b);
            e.defs = one(a);
        }
        Loadi | Loadf | Loadk | Loadfalse | Loadtrue | Getupval | Gettabup | Newtable | Closure => {
            e.defs = one(a);
        }
        Loadkx => e.defs = one(a),
        Lfalseskip => {
            e.defs = one(a);
            e.succ = [Some(pc as i64 + 2), None];
        }
        Loadnil => e.defs = range(a, b + 1),
        Setupval => e.uses = one(a),
        Gettable => {
            e.uses = or(one(b), one(c));
            e.defs = one(a);
        }
        Geti | Getfield | Unm | Bnot | Not | Len => {
            e.uses = one(b);
            e.defs = one(a);
        }
        Settabup => e.uses = rk_c(i),
        Settable => e.uses = or(or(one(a), one(b)), rk_c(i)),
        Seti | Setfield => e.uses = or(one(a), rk_c(i)),
        // (the key is a register when `k` is clear: read it conservatively in both versions)
        SelfOp => {
            e.uses = or(one(b), rk_c(i));
            e.defs = or(one(a), one(a + 1));
        }
        op if is_arith(op) => {
            let two = matches!(
                op,
                Add | Sub | Mul | Mod | Pow | Div | Idiv | Band | Bor | Bxor | Shl | Shr
            );
            e.uses = if two { or(one(b), one(c)) } else { one(b) };
            e.defs = one(a);
            if is_mmbin(code.get(pc + 1).and_then(|&w| Instruction(w).opcode())) {
                e.succ = [Some(next), Some(pc as i64 + 2)];
            }
        }
        // the metamethod result goes to `A` of the arithmetic instruction before
        Mmbin => {
            e.uses = or(one(a), one(b));
            if pc > 0 {
                e.defs = one(Instruction(code[pc - 1]).a());
            }
        }
        Mmbini | Mmbink => {
            e.uses = one(a);
            if pc > 0 {
                e.defs = one(Instruction(code[pc - 1]).a());
            }
        }
        Concat => {
            e.uses = range(a, b);
            e.defs = one(a);
        }
        Close => e.uses = from(a),
        Tbc => return None,
        Jmp => e.succ = [Some(next + i64::from(i.sj())), None],
        Eq | Lt | Le => {
            e.uses = or(one(a), one(b));
            e.succ = [Some(next), Some(next + 1)];
        }
        Eqk | Eqi | Lti | Lei | Gti | Gei | Test => {
            e.uses = one(a);
            e.succ = [Some(next), Some(next + 1)];
        }
        Testset => {
            e.uses = one(b);
            e.succ = [Some(next), Some(next + 1)];
        }
        Call => {
            e.uses = if b == 0 { open(a) } else { range(a, b) };
            if c >= 2 {
                e.defs = range(a, c - 1);
            } else if c == 0 && f.open_call_defs {
                // B2: the results are `A .. top`; the open reader that follows (`CALL` / `RETURN` / `SETLIST` with `B == 0`) reads only them,
                // and the registers above `top` are free temporaries that the code generator writes before it reads them
                e.defs = from(if f.open_defs_low {
                    a.saturating_sub(2)
                } else {
                    a
                });
            }
        }
        Tailcall => {
            e.uses = if b == 0 { open(a) } else { range(a, b) };
            e.succ = [None, None];
        }
        Return => {
            e.uses = match b {
                0 => open(a),
                1 => EMPTY,
                _ => range(a, b - 1),
            };
            e.succ = [None, None];
        }
        Return0 => e.succ = [None, None],
        Return1 => {
            e.uses = one(a);
            e.succ = [None, None];
        }
        Forloop => {
            e.uses = range(a, 3);
            e.succ = [Some(next), Some(next - i64::from(i.bx()))];
        }
        Forprep => {
            e.uses = range(a, 3);
            e.succ = [Some(next), Some(next + i64::from(i.bx()) + 1)];
        }
        Tforprep => {
            e.uses = from(a);
            e.succ = [Some(next + i64::from(i.bx())), None];
        }
        Tforcall => e.uses = from(a),
        Tforloop => {
            e.uses = from(a);
            e.succ = [Some(next), Some(next - i64::from(i.bx()))];
        }
        Setlist => {
            #[cfg(rustynail_lua54)]
            let n = i.b();
            #[cfg(not(rustynail_lua54))]
            let n = i.vb();
            e.uses = if n == 0 { open(a) } else { range(a, n + 1) };
        }
        Vararg => {
            if c >= 2 {
                e.defs = range(a, c - 1);
            }
        }
        #[cfg(not(rustynail_lua54))]
        Getvarg => {
            e.uses = or(one(b), one(c));
            e.defs = one(a);
        }
        #[cfg(not(rustynail_lua54))]
        Errnnil => e.uses = one(a),
        Varargprep => e.uses = from(0),
        Extraarg => {}
        _ => e.uses = from(0),
    }
    Some(e)
}

/// The registers live before every instruction (`out[pc]`), or `None` when the analysis refuses the code.
pub fn live_in(code: &[u32]) -> Option<Vec<Set>> {
    live_in_with(code, Faults::default())
}

/// [`live_in`] with deliberate faults (mutants).
pub fn live_in_with(code: &[u32], f: Faults) -> Option<Vec<Set>> {
    let n = code.len();
    let mut eff = Vec::with_capacity(n);
    for pc in 0..n {
        let mut e = effect(code, pc, f)?;
        for t in e.succ.into_iter().flatten() {
            // a successor past the last instruction is a malformed function: refuse (the compiler always ends with a RETURN)
            if t < 0 || t as usize >= n {
                return None;
            }
        }
        if f.no_backedges {
            for t in &mut e.succ {
                if t.is_some_and(|t| t <= pc as i64) {
                    *t = None;
                }
            }
        }
        eff.push(e);
    }
    let mut live = vec![EMPTY; n];
    let mut changed = true;
    while changed {
        changed = false;
        for pc in (0..n).rev() {
            let e = &eff[pc];
            let mut out = EMPTY;
            for t in e.succ.into_iter().flatten() {
                out = or(out, live[t as usize]);
            }
            let l = or(e.uses, minus(out, e.defs));
            if l != live[pc] {
                live[pc] = l;
                changed = true;
            }
        }
    }
    Some(live)
}

/// For every pc, the registers a snapshot taken before that instruction may leave out of its map when they hold a
/// number: registers that are dead there (no path reads them before writing them), at or above the active local
/// variables of the neighbourhood (`pc - 1 ..= pc + 1`, so a hook that sees `pc` either side of the instruction still
/// finds every local), and not captured by an inner closure. `None`: no pruning for this prototype (no debug information
/// on the locals, `TBC`, ...).
pub fn prunable(p: &Proto) -> Option<Vec<Set>> {
    prunable_with(p, Faults::default())
}

/// [`prunable`] with deliberate faults (mutants).
pub fn prunable_with(p: &Proto, f: Faults) -> Option<Vec<Set>> {
    let code = p.code();
    let locvars = p.locvars();
    if locvars.is_empty() {
        return None;
    }
    let mut live = live_in_with(code, f)?;
    if f.ignore_live {
        live.iter_mut().for_each(|l| *l = EMPTY);
    }
    let mut captured = EMPTY;
    for q in p.protos() {
        for u in q.upvalue_descs() {
            if u.instack {
                captured = or(captured, one(u32::from(u.idx)));
            }
        }
    }
    let nact = |pc: i64| -> u32 {
        if pc < 0 {
            return 0;
        }
        let pc = pc as u32;
        locvars
            .iter()
            .filter(|l| l.startpc <= pc && pc < l.endpc)
            .count() as u32
    };
    let maxs = u32::from(p.maxstacksize);
    Some(
        (0..code.len())
            .map(|pc| {
                let n = if f.ignore_locals {
                    0
                } else {
                    nact(pc as i64 - 1)
                        .max(nact(pc as i64))
                        .max(nact(pc as i64 + 1))
                };
                minus(minus(range(n, maxs.saturating_sub(n)), live[pc]), captured)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets() {
        assert!(has(&range(3, 2), 3) && has(&range(3, 2), 4) && !has(&range(3, 2), 5));
        assert!(has(&from(250), 255) && !has(&from(250), 249));
        assert!(has(&one(200), 200));
    }

    #[test]
    fn straight_line() {
        // r2 = r0 + r1 (+ MMBIN); return r2
        let code = [
            Instruction::abck(OpCode::Add, 2, 0, 1, 0).0,
            Instruction::abck(OpCode::Mmbin, 0, 1, 6, 0).0,
            Instruction::abck(OpCode::Return1, 2, 2, 0, 0).0,
        ];
        let l = live_in(&code).unwrap();
        assert!(has(&l[0], 0) && has(&l[0], 1) && !has(&l[0], 2));
        assert!(has(&l[2], 2) && !has(&l[2], 0));
    }
}
