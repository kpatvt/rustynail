//! Test-only code mutations of the region generator (docs/JIT_TIER15.md 16.5). Each mutant is a deliberate bug that
//! the test suite must detect (`tests/t15_core.rs::every_mutant_is_detected`). They exist only under the cargo
//! feature `t15_mutants` (off by default); without it [`active`] is a constant `false` and the optimiser removes
//! every mutation site, so default code is byte-identical.
//!
//! The active mutants come from `RUSTYNAIL_JIT_MUT=name[,name...]` (read once per process).
//!
//! | name | the bug |
//! |------|---------|
//! | `count_after_fall` | the block count of a head that directly follows a fallback is one too high |
//! | `no_budget_reload_after_call` | stubs of the instruction after a `CALL` do not reload `r11` from `[r15]` |
//! | `drop_guard_first` | a stub with two or more guards omits its first guard |
//! | `drop_guard_last` | ... its last guard |
//! | `drop_guard_after_fall` | a stub of a fallback successor omits its first guard |
//! | `flush_skip_last` | a flush of two or more slots omits the last one |
//! | `flush_skip_tag` | a flush stores the payload but not the tag byte of its first slot |
//! | `leave_no_budget` | the pad that leaves the region does not store `r11` to `[r15]` |
//! | `pad_no_irq_poll` | the back-edge poll tests the trap word but not the interrupt flag |
//! | `fall_no_budget` | a fallback does not store `r11` before the tier-1 template |
//! | `stub_no_load_last` | a stub does not load its last register-resident slot |
//! | `flt_flush_int_tag` | (T4b) a flush stores the integer tag byte for float slots |
//! | `fcmp_negated` | (T4b) float compares branch on the negated condition (wrong for NaN) |
//! | `i2f_missing` | (T4b) an integer operand of a float operation is not converted (its bits are used as a double) |
//! | `fneg_missing` | (T4b) `-x` of a float does not flip the sign |
//! | `fforloop_always_pos` | (T4b) the float `FORLOOP` assumes a positive step |
//! | `fforloop_clobber_idx` | (T4b) the float `FORLOOP` also updates the index register on the exit path |
//! | `fbin_alias_commute` | (T4b) `d = x op d` is computed as `d = d op x` |
//! | `tbl_no_lower_bound` | (T5) an inline array access with a register key omits the `key < 1` check |
//! | `tbl_no_upper_bound` | (T5) an inline array access omits the `key > asize` check |
//! | `tbl_no_tag_check` | (T5) a typed (`INT`/`FLT`) array or field read does not compare the element's tag byte |
//! | `tbl_no_nil_check` | (T5) an untyped array or field read does not reject a nil element |
//! | `tbl_no_meta_check` | (T5) an array store into an empty slot does not look at the metatable (`__newindex`) |
//! | `tbl_no_nonnil_check` | (T5) a field store does not require an existing non-nil value (`__newindex`, insertion) |
//! | `tbl_no_barrier` | (T5) a store of a collectable value skips the black-table / white-value GC barrier test |
//! | `tbl_no_key_tag` | (T7) an inline array access with a memory key (`RegU`) does not compare the key slot's tag byte |
//! | `tbl_no_table_tag` | (T7) an inline access with an unproven table fact does not compare the table slot's tag byte |
//! | `tbl_no_coll_test` | (T7) the barrier check of a value of unproven class does not test the collectable bit before dereferencing the payload |
//! | `tbl_no_s0_reload` | (T7) `SETFIELD` does not reload the table pointer clobbered by the chain walk before the barrier check |
//! | `tbl_next_zext` | (T7) the hash chain offset `next` is zero- instead of sign-extended |
//! | `tbl_const_off` | (T7) the payload address of a constant-index (`GETI`/`SETI`) access is one element off |
//! | `pol_no_widen` | (T7) a respecialisation after the first one does not widen the speculation (it neither retargets nor joins: the same speculation is compiled again) |
//! | `pol_no_peel_exempt` | (T7) the by-design first-entry failure of a peeled slot is counted like any other stub failure |
//! | `pol_no_penalty` | (T7) a benign failure rate at the head stub (few failures per successful entry) is not recognised and triggers respecialisation |
//! | `pol_no_giveup` | (T7) a nest at the respecialisation cap keeps its region instead of being given up |
//! | `pol_no_fail_count` | (T7) the failure pads never decrement the countdown (no policy decision is ever taken) |
//! | `pol_drop_old_block` | (T7) a recompile frees the old code block instead of retiring it (activations suspended in it return into freed code) |
//! | `tbl_widen_unchecked` | (T7) an inline table load under a nil or non-singleton speculated result commits any non-nil element without checking it (the pre-T7 behaviour; downstream code then trusts the nil fact and skips the GC barrier) |
//! | `pol_countdown_64bit` | (T7 review) the failure pad compares the zero-extended countdown as 64 bits: it fires only on the exact 0 -> -1 step |
//! | `pol_stale_no_rearm` | (T7 review) the countdown that fired in a stale version's pad is not re-armed |
//! | `pol_cap_no_giveup` | (T7 review) when the normal compile cap is spent during a respecialisation the nest keeps its failing region instead of being given up with a reserved slot |
//! | `gate_upval_single` | (T7 review) `GETUPVAL` fallbacks weigh 1 (not 2) in the profitability gate (heapsort's `sift` gets a region that is slower than tier 1) |
//! | `intr_no_hook_check` | (T6) an inline `math` call does not test `thread.hookmask` |
//! | `intr_no_identity` | (T6) an inline `math` call does not compare the callee with the registered function |
//! | `intr_wrong_identity` | (T6) ... compares it with the wrong address |
//! | `intr_no_stack_check` | (T6) ... does not require `LUA_MINSTACK` free stack slots (`precall_c` would grow the stack) |
//! | `intr_no_ci_check` | (T6) ... does not require a spare `CallInfo` |
//! | `intr_no_count` | (T6) ... omits the `CountSub{1}` of the `CALL` |
//! | `intr_abs_no_clear` | (T6) `math.abs` of a float keeps the sign bit |
//! | `intr_minmax_swap` | (T6) `math.min` computes `max` and vice versa |
//! | `intr_minmax_order` | (T6) `math.min/max` of floats use the other operand order (wrong for NaN and signed zeros) |
//! | `intr_floor_ceil_swap` | (T6) `math.floor` of a non-integral float rounds like `ceil` and vice versa |
//! | `intr_floor_no_range` | (T6) `math.floor/ceil` of a NaN / infinity / out-of-range float yields an integer instead of calling the real function |
//! | `up_no_spill` | (T6) an inline `GETUPVAL` does not write back register-resident escaped slots before reading the upvalue |
//! | `pol_no_demote` | (T7/T6) an intrinsic site whose slow path or result guard keeps failing is not demoted to an ordinary `CALL` (the failures are then treated like any other stub failure) |
//! | `pol_no_bool_widen` | (T7/T6) a boolean flip at a load is retargeted like any other type change (one decision per load: the respecialisation budget is spent before every load is widened) |
//! | `pol_no_budget_reload_stay` | (T6 review) the failure pad of an intrinsic slow path does not reload `r11` from `[r15]` when the policy helper stays (the helper clobbered it; the tier-1 `CALL` then stores garbage as the budget) |
//! | `pol_count_all_slow` | (T6 review) hook / interrupt / representability misses of an inline intrinsic also feed the demotion countdown |
//! | `pol_demote_no_rollback` | (T6 review) an intrinsic demotion whose recompile is refused (code cap) is not rolled back: the site disappears from `intr_sites()` while its inline code keeps running |
//! | `exitmap_wrong_reg` | (T8) a flush writes the first slot from the register of the second (a wrong entry of the write-back map) |
//! | `leave_drop_entry` | (T8) the pads that leave the region drop the first entry of their write-back map |
//! | `fall_succ_no_guards` | (T8) the stub after a fallback does not re-guard anything |
//! | `rr_join_union` | (T8) the register-resident set at a join is the union of the predecessors' (not the intersection) |
//! | `bad_fall_targets` | NOT a bug in emitted code: the dataflow misses fallback successors, which the generator must notice and refuse (graceful degradation to tier 1) |

#[cfg(feature = "t15_mutants")]
fn names() -> &'static [String] {
    static N: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    N.get_or_init(|| {
        std::env::var("RUSTYNAIL_JIT_MUT")
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Is mutant `name` active?
#[cfg(feature = "t15_mutants")]
pub fn active(name: &str) -> bool {
    names().iter().any(|n| n == name)
}

/// Is mutant `name` active? (Never, without the feature `t15_mutants`.)
#[cfg(not(feature = "t15_mutants"))]
#[inline(always)]
pub const fn active(_name: &str) -> bool {
    false
}
