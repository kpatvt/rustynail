//! Textual dump of a trace (the `RUSTYNAIL_JIT_VERBOSE` / golden-test format of the recorder tests, 9.1).
//!
//! One instruction per line: `NNNN  FLAGS OP     TYPE  operands  [aux]`. Instruction refs print as `%N` (the index),
//! constants as `#value`, raw operands (a slot number, a budget charge, a table field) as plain numbers. Flags column:
//! `>` guard, `+` PHI, `^` loop invariant. Snapshots follow the instructions.

use super::ir::*;
use std::fmt::Write;

/// A constant ref as text.
pub fn kstr(k: KVal) -> String {
    match k {
        KVal::Int(i) => format!("#{i}"),
        KVal::Num(b) => {
            let f = f64::from_bits(b);
            if f.is_nan() {
                format!("#nan:{b:016x}")
            } else {
                format!("#{f:?}")
            }
        }
        KVal::Pri(t) => format!("#{}", t.name()),
        KVal::Ptr(a, kind) => format!("#{kind:?}:{a:#x}"),
    }
}

fn rstr(ir: &TraceIr, r: Ref) -> String {
    if r == NOREF {
        "-".to_string()
    } else if r < REF_BIAS {
        match ir.k.get(usize::from(r) - 1) {
            Some(&k) => kstr(k),
            None => format!("#?{r}"),
        }
    } else {
        format!("%{}", r - REF_BIAS)
    }
}

/// Dumps `ir` (see the module documentation).
pub fn dump(ir: &TraceIr) -> String {
    let mut s = String::new();
    for (n, i) in ir.ins.iter().enumerate() {
        let flags = format!(
            "{}{}{}",
            if i.flags & F_GUARD != 0 { '>' } else { ' ' },
            if i.flags & F_PHI != 0 { '+' } else { ' ' },
            if i.flags & F_LOOPINV != 0 { '^' } else { ' ' }
        );
        let (ra, rb) = i.op.ref_operands();
        let a = if ra { rstr(ir, i.a) } else { i.a.to_string() };
        let b = if rb { rstr(ir, i.b) } else { i.b.to_string() };
        let _ = write!(s, "{n:04} {flags} {:<6} {:<5}", i.op.name(), i.t.name());
        match i.op {
            TOp::Loop => {}
            TOp::Poll => {}
            TOp::Tvirt => {
                let _ = write!(s, " {}", i.aux);
            }
            TOp::Sstore => {
                let _ = write!(s, " slot{} <- {a}", i.aux);
            }
            TOp::Budget => {
                let _ = write!(s, " {a}");
            }
            TOp::Sload => {
                let _ = write!(s, " slot{a}");
            }
            _ => {
                if i.a != NOREF || ra {
                    let _ = write!(s, " {a}");
                }
                if i.b != NOREF || rb {
                    let _ = write!(s, " {b}");
                }
            }
        }
        if i.flags & F_GUARD != 0 {
            let _ = write!(s, "  S{}", aux_snap(i.aux));
            if i.op == TOp::Hrefv {
                let b = aux_bits(i.aux);
                if b & HREF_DYN != 0 {
                    s.push_str(" dyn");
                } else {
                    let _ = write!(
                        s,
                        " lsize={}{}",
                        b & 0xFF,
                        if b & HREF_INT != 0 { " int" } else { "" }
                    );
                }
                if b & HREF_ABS != 0 {
                    s.push_str(" abs");
                }
            } else if i.op == TOp::Hrefk {
                // F1: the bits are the recorded index of the node in the node vector
                let _ = write!(s, " node={}", aux_bits(i.aux));
            } else if i.op == TOp::Hmiss {
                let (ix, d) = hmiss_unpack(aux_bits(i.aux));
                let _ = write!(s, " node={ix} next={d}");
            } else if aux_bits(i.aux) != 0 {
                let _ = write!(s, " bits={:#x}", aux_bits(i.aux));
            }
        } else if i.op == TOp::Hrefv {
            // DYNMASK: the non-guard (nil-miss) form
            let b = aux_bits(i.aux);
            let _ = write!(s, "  dyn{}", if b & HREF_NIL != 0 { " nil" } else { "" });
        } else if let Some(v) = i.third() {
            let _ = write!(s, " {}", rstr(ir, v));
        } else if i.aux != 0 {
            let _ = write!(s, "  aux={}", i.aux);
        }
        while s.ends_with(' ') {
            s.pop();
        }
        s.push('\n');
    }
    // P7: after the loop pass ran on this thread (the pipeline driver, the P7 tests) the dump of a peeled trace says
    // so: `^` marks pre-roll instructions the body reuses (hoisted), `+` loop-carried refs. Plain IR (the P0 / P2
    // goldens, recorder dumps) never gets the line: the note is empty on threads that did not run the pass.
    let note = super::opt_loop::last_note();
    if let (Some((pre, phis, body)), false) = (super::opt_loop::shape(ir), note.is_empty()) {
        let _ = writeln!(
            s,
            "; loop: {pre} pre-roll, {phis} phis, {body} body; {note}"
        );
    }
    for (n, sn) in ir.snaps.iter().enumerate() {
        let _ = write!(s, "S{n} pc={} frame={}", sn.pc, sn.frame);
        if sn.seg != 0 {
            let _ = write!(s, " seg={}", sn.seg);
        }
        if sn.refund != 0 {
            let _ = write!(s, " refund={}", sn.refund);
        }
        s.push_str(" [");
        for (j, e) in ir.snap_entries(n as u32).iter().enumerate() {
            if j > 0 {
                s.push(' ');
            }
            if e.flags & SNAP_SUNK != 0 {
                let _ = write!(s, "{}=SUNK{}", e.slot, rstr(ir, e.r));
            } else if e.flags & SNAP_SALIAS != 0 {
                let _ = write!(s, "{}=ALIAS{}", e.slot, rstr(ir, e.r));
            } else if e.flags & SNAP_SFKEY != 0 {
                let _ = write!(s, ".{}", rstr(ir, e.r));
            } else if e.flags & SNAP_SMT != 0 {
                let _ = write!(s, ".MT={}", rstr(ir, e.r));
            } else if e.flags & SNAP_SFVAL != 0 {
                let _ = write!(s, "={}", rstr(ir, e.r));
            } else {
                let _ = write!(s, "{}={}", e.slot, rstr(ir, e.r));
            }
        }
        s.push_str("]\n");
    }
    s
}

/// The `trace-<n>.txt` file of `RUSTYNAIL_JIT_DUMP=dir` (P6): a header with the trace number and its code block, then
/// the pipeline's text (recorded path, IR before/after each pass, snapshots, register allocation, micro-ops).
pub fn trace_file(no: u32, block: usize, entry: usize, text: &str) -> String {
    format!("trace {no}\nblock {block:#x} entry {entry:#x}\n{text}\n")
}

/// The summary printed when a state closes under `RUSTYNAIL_JIT_VERBOSE=1` (9.10): recordings, aborts by reason,
/// entries, flushes. `entered` is the sum of the heads' successful-entry counters.
#[cfg(feature = "trace")]
pub fn stats_report(s: &super::cache::TrStats, entered: u64, warnings: &[String]) -> String {
    let mut o = String::new();
    for w in warnings {
        let _ = writeln!(o, "[trace] warning: {w}");
    }
    // BUNDLE-1 (6): function-entry heads that only probe for a native self-call (`FnShort`) and starts the recorder refused are
    // not recordings of a loop or call path: counted apart, so that `recorded` says how many real recordings ran
    let fn_short = s.aborted.get("FnShort").copied().unwrap_or(0);
    let start_refused = s.aborted.get("StartRefused").copied().unwrap_or(0);
    let probes = fn_short + start_refused;
    let _ = writeln!(
        o,
        "[trace] recorded {} installed {} entered {} flushes {} rerecord {} blacklisted {} refused {} deferred {} ineffective {} probes {} (fn-short {} start-refused {})",
        s.started.saturating_sub(probes),
        s.installed,
        entered,
        s.flushes,
        s.rerec,
        s.blacklisted,
        s.refused,
        s.deferred,
        s.ineffective,
        probes,
        fn_short,
        start_refused
    );
    let _ = writeln!(
        o,
        "[trace] hot exits: type-guard {} other {}; tracemove {}",
        s.exit_type_hot, s.exit_other_hot, s.moves
    );
    for (reason, n) in &s.aborted {
        let _ = writeln!(o, "[trace] aborted {reason}: {n}");
    }
    let _ = writeln!(o, "{}", super::phase::report());
    let _ = writeln!(o, "{}", super::sink::report());
    if s.ci_repaired > 0 {
        let _ = writeln!(
            o,
            "[trace] cispare: entry failures repaired {}",
            s.ci_repaired
        );
    }
    if s.dom_flushes > 0 {
        let _ = writeln!(o, "[trace] domtop: roots re-recorded {}", s.dom_flushes);
    }
    if s.chain_flushes > 0 || s.chain_checks > 0 {
        let _ = writeln!(
            o,
            "[trace] chainre: callee chains re-ordered {} (checks {})",
            s.chain_flushes, s.chain_checks
        );
    }
    if s.chain_deep > 0 {
        let _ = writeln!(
            o,
            "[trace] chaindepth: callee-guard exits that got no further link {}",
            s.chain_deep
        );
    }
    if s.ncallee_heads > 0 {
        let _ = writeln!(
            o,
            "[trace] ncallee: heads re-recorded without entry callee guards {}",
            s.ncallee_heads
        );
    }
    if s.c_entered > 0 {
        let _ = writeln!(
            o,
            "[trace] centry: registered {} light {} entries {}",
            s.c_entered, s.c_light, s.c_enters
        );
    }
    if s.fn_promoted > 0 {
        let _ = writeln!(
            o,
            "[trace] fnpromo: function heads promoted {}",
            s.fn_promoted
        );
    }
    if s.inner_retried > 0 {
        let _ = writeln!(o, "[trace] innerretry: rounds started {}", s.inner_retried);
    }
    if s.retried > 0 || s.ident_kept > 0 {
        let _ = writeln!(
            o,
            "[trace] second: heads given a second chance {} identical re-records installed {}",
            s.retried, s.ident_kept
        );
    }
    if s.la_blocked > 0 {
        let _ = writeln!(
            o,
            "[trace] loopany guard: fired {} family flushes {} pruned sides {} (self {})",
            s.la_blocked, s.la_family, s.la_pruned, s.la_self
        );
    }
    if s.black_handled > 0 {
        let _ = writeln!(
            o,
            "[trace] exit handler: blacklisted exits reported {}",
            s.black_handled
        );
    }
    if s.side_started > 0 {
        let _ = writeln!(
            o,
            "[trace] side traces: recorded {} installed {} failed {} preludes {}",
            s.side_started, s.side_installed, s.side_failed, s.side_preludes
        );
        for (reason, n) in &s.side_aborted {
            let _ = writeln!(o, "[trace] side aborted {reason}: {n}");
        }
    }
    if s.nest_installed > 0 || s.frame_links > 0 || s.stitches > 0 || s.side_frame_links > 0 {
        let _ = writeln!(
            o,
            "[trace] links: nest roots {} frame links {} stitches {} sidelinks {}",
            s.nest_installed, s.frame_links, s.stitches, s.side_frame_links
        );
    }
    if s.hk_fused > 0 {
        let _ = writeln!(
            o,
            "[trace] fused HREFK+HLOAD tag checks: {} (two exits: {})",
            s.hk_fused, s.hk_split
        );
    }
    if s.deep_sites > 0 || s.deep_recs > 0 {
        let _ = writeln!(
            o,
            "[trace] deep sites: {} deep recordings: {}",
            s.deep_sites, s.deep_recs
        );
    }
    {
        let (tb, ub) = (
            super::asm::BARRIER_CALLS[0].load(std::sync::atomic::Ordering::Relaxed),
            super::asm::BARRIER_CALLS[1].load(std::sync::atomic::Ordering::Relaxed),
        );
        if tb > 0 || ub > 0 {
            let _ = writeln!(o, "[trace] barrier helper calls: table {tb} upvalue {ub}");
        }
    }
    if s.rearm_arms > 0 || s.rearm_enters > 0 || s.rearm_misses > 0 {
        let _ = writeln!(
            o,
            "[trace] rearm: armed {} entered {} missed {} restored {} unclean {}/{}",
            s.rearm_arms,
            s.rearm_enters,
            s.rearm_misses,
            s.rearm_restored,
            s.rearm_unclean[0],
            s.rearm_unclean[1]
        );
    }
    if s.nest_rule > 0 || s.nest_rule_kept > 0 {
        let _ = writeln!(
            o,
            "[trace] nest rule: {} nests handed to J4, {} withheld (inlined calls)",
            s.nest_rule, s.nest_rule_kept
        );
    }
    o
}
