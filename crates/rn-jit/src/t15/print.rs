//! Textual dump of a [`RegionPlan`] (debugging, hand checks against `luac -l`, `RUSTYNAIL_JIT_VERBOSE`).

use super::{FuncInfo, RegionPlan, SlotSet};
use rn_core::instruction::Instruction;
use rn_core::ver::active::OP_NAMES;
use std::fmt::Write;

fn slots(s: &SlotSet) -> String {
    let v: Vec<String> = s.iter().map(|x| x.to_string()).collect();
    format!("{{{}}}", v.join(","))
}

/// One line per instruction: pc, opcode and operands, class, disposition, the facts of the registers it
/// reads or writes (before), and the guard / memory-tag annotations; a header lists loops, entry guards and
/// the slot sets.
pub fn dump(plan: &RegionPlan, info: &FuncInfo<'_>) -> String {
    let mut o = String::new();
    let _ = writeln!(
        o,
        "region [{}, {}] nslots={} rounds={} sweeps={}",
        plan.head, plan.end, plan.nslots, plan.stats.rounds, plan.stats.sweeps
    );
    for l in &plan.loops {
        let _ = writeln!(
            o,
            "  loop [{}, {}] depth {} parent {:?}",
            l.head, l.end, l.depth, l.parent
        );
    }
    let e: Vec<String> = plan
        .entry
        .iter()
        .map(|f| {
            format!(
                "R{}:{}{}{}",
                f.slot,
                f.set,
                if f.peel { "(peel)" } else { "" },
                if f.guarded { "" } else { "(unguarded)" }
            )
        })
        .collect();
    let _ = writeln!(o, "  entry guards: {}", e.join(" "));
    let _ = writeln!(o, "  resume points: {:?}", plan.resume_points);
    let _ = writeln!(
        o,
        "  written {} escaped {} invariant {} setupval {}",
        slots(&plan.written),
        slots(&plan.escaped),
        slots(&plan.invariant),
        plan.has_setupval
    );
    for f in &plan.fors {
        let _ = writeln!(
            o,
            "  for pc {} A={} ctl={} step_const={:?}",
            f.pc, f.a, f.ctl, f.step_const
        );
    }
    let _ = writeln!(o, "  exits: {:?}", plan.exits);
    for ins in &plan.insns {
        let i = Instruction(info.code[ins.pc as usize]);
        let name = i.opcode().map_or("?", |op| OP_NAMES[op as usize]);
        let mut used = ins.uses;
        used.union_with(&ins.defs);
        let facts: Vec<String> = used
            .iter()
            .filter(|&s| s < plan.nslots)
            .map(|s| format!("R{s}={}", plan.fact(ins.pc, s)))
            .collect();
        let _ = write!(
            o,
            "  {:4} {:<9} A={:<3} B={:<3} C={:<3} {:?}/{:?}{}{}",
            ins.pc,
            name,
            i.a(),
            i.b(),
            i.c(),
            ins.class,
            ins.disp,
            if ins.barrier { " BARRIER" } else { "" },
            if ins.reached { "" } else { " unreached" },
        );
        if ins.role.is_follower() {
            let _ = writeln!(o, "  [{:?}]", ins.role);
            continue;
        }
        let _ = write!(o, " | {}", facts.join(" "));
        if let Some(g) = ins.guard_def {
            let _ = write!(o, " | guard_def={g}");
        }
        let _ = writeln!(
            o,
            " | guards_in={} mem={}",
            slots(&ins.guard_live_in),
            slots(&ins.mem_known)
        );
    }
    o
}
