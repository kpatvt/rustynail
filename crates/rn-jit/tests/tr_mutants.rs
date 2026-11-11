//! P8a: the every-mutant runner (docs/JIT_TRACE.md 9.7, 10.1).
//!
//! `rn_jit::tr::mutants::NAMES` is the frozen registry of deliberate bugs. Each task that owns the code a mutant
//! breaks injects it and names the tests that must catch it; this file is the whole-suite check. [`TABLE`] maps every
//! registered mutant to its detecting test(s) (test binary + test name, plus the environment the test needs).
//! `every_registered_mutant_is_caught` then, for every mutant, computes the result instead of asserting it by hand:
//!
//! 1. every detecting test is run in a child process WITHOUT a mutant and must pass (the baseline);
//! 2. every detecting test is run in a child process with `RUSTYNAIL_JIT_MUT=<name>`; the mutant is *caught* iff at
//!    least one of its tests fails (assertion, crash or timeout);
//! 3. an *injected* mutant (its `mutants::active("name")` site exists in `src/tr`, or in rn-core for the two VM
//!    mutants) with no detecting test, or not caught, fails the run. A mutant whose injection site does not exist
//!    yet is reported as `not injected (owner task)`: P7 (`alias_*`, `fwd_across_store`, `cse_across_store`,
//!    `peel_subst_stale`, `abc_with_store`, `nonslot_phi_skip`), F5, F7a, F8. `TR_MUT_STRICT=1` (the F9 gate) turns
//!    those into failures too.
//!
//! The detecting tests live in the other `tr_*` binaries (built on demand with a nested `cargo test --no-run`, same
//! features); tests that need `t15_oracle` (`hooked_head_no_trap`, the count-model comparisons, this task's identity
//! probes) are skipped, and reported as such, in a build without that feature. Run: `systemd-run --user --scope -q -p
//! MemoryMax=8G -p MemorySwapMax=0 timeout 3000 cargo test -p rn-jit --features tr_mutants,t15_oracle --test tr_mutants
//! -- --nocapture </dev/null`. Knobs: `TR_MUT_ONLY=<substr>` selects mutants, `TR_MUT_SKIP=a,b` leaves some out, `TR_MUT_STRICT=1`, `TR_MUT_V=1`
//! (print child output of the tests that were not caught).
//!
//! The FULL runner under the 8 GB cap (all mutants, a few hours): build once, then run sharded in one memory-capped scope; see docs/JIT_TESTING.md.
//! Resource knobs: `TR_MUT_JOBS` (test binaries in parallel, default 2), `TR_MUT_BUILD_JOBS` (nested cargo, 4), `TR_MUT_MEM_MB` (RLIMIT_AS of
//! each test binary, 4096), `TR_CORE_MEM_MB` (RLIMIT_AS of each tr_core job child, 2048), `TR_CORE_JOBS`; `TR_MUT_SHARD=i/n`,
//! `TR_MUT_RESULTS=file` (one line per finished mutant), `TR_MUT_RESUME=1` (skip those recorded as caught). A mutant none of whose
//! tests fails prints `SURVIVED`.
#![cfg(feature = "trace")]

use rn_jit::tr::mutants::NAMES;
use std::collections::BTreeSet;

/// How a test is run.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Need {
    /// Any `tr_mutants` build.
    Plain,
    /// Needs rn-jit's `t15_oracle` feature (the test does not exist without it).
    Oracle,
}

/// One detecting test: `bin` is the integration-test target, `test` the exact test name (`*`: the whole binary,
/// minus the `mutants_are_caught` runners), `env` extra variables of the child.
#[cfg_attr(not(all(feature = "tr_mutants", not(miri))), allow(dead_code))]
struct Det {
    bin: &'static str,
    test: &'static str,
    need: Need,
    env: &'static [(&'static str, &'static str)],
}

const fn d(bin: &'static str, test: &'static str) -> Det {
    Det {
        bin,
        test,
        need: Need::Plain,
        env: &[],
    }
}
const fn o(bin: &'static str, test: &'static str) -> Det {
    Det {
        bin,
        test,
        need: Need::Oracle,
        env: &[],
    }
}

#[cfg_attr(not(all(feature = "tr_mutants", not(miri))), allow(dead_code))]
struct Row {
    name: &'static str,
    /// The task that owns the injection point (documentation, and the text of "not injected").
    owner: &'static str,
    dets: &'static [Det],
}

/// A short leash for the tests of this task that run child processes of their own (a mutated trace may loop forever).
const LEASH: &[(&str, &str)] = &[("TR_CORE_TIMEOUT", "100"), ("TR_P8A_TIMEOUT", "100")];
const fn dl(bin: &'static str, test: &'static str) -> Det {
    Det {
        bin,
        test,
        need: Need::Plain,
        env: LEASH,
    }
}
const fn ol(bin: &'static str, test: &'static str) -> Det {
    Det {
        bin,
        test,
        need: Need::Oracle,
        env: LEASH,
    }
}

/// The P7 / F-task rows are listed with whole-binary detection so that the moment their injection sites appear the
/// runner exercises them (`tr_passes`, `tr_loop`); the owner may replace them by exact test names.
const P7_BINS: &[Det] = &[d("tr_passes", "*"), d("tr_loop", "*")];

const TABLE: &[Row] = &[
    // snapshots, counting, landing labels
    Row {
        name: "snap_drop_last",
        owner: "P3a",
        dets: &[
            d("tr_record", "ir_replay_matches_the_interpreter"),
            dl("tr_core", "mutant_probe_live_programs"),
            ol("tr_oracle", "mutant_probe"),
        ],
    },
    Row {
        name: "snap_prune_dead",
        owner: "P3a",
        dets: &[
            d("tr_record", "ir_replay_matches_the_interpreter"),
            dl("tr_core", "mutant_probe_live_programs"),
            ol("tr_oracle", "mutant_probe"),
        ],
    },
    Row {
        name: "refund_off_by_one",
        owner: "P3a",
        dets: &[
            d("tr_record", "count_model_is_exact"),
            d("tr_record", "ir_replay_matches_the_interpreter"),
            o("tr_record_count", "tier1_count_failures_match_the_snapshots"),
            ol("tr_identity", "mutant_probe"),
            ol("tr_oracle", "mutant_probe"),
        ],
    },
    Row {
        name: "land_pre_mid_swap",
        owner: "P3a",
        dets: &[
            d("tr_record", "count_model_is_exact"),
            d("tr_record", "ir_replay_matches_the_interpreter"),
            o("tr_record_count", "tier1_guard_exits_match_the_snapshots"),
            ol("tr_identity", "mutant_probe"),
            ol("tr_oracle", "mutant_probe"),
        ],
    },
    Row {
        name: "budget_no_check",
        owner: "P5b",
        dets: &[
            d("tr_asm", "hand_budget_charge_and_exit_value"),
            d("tr_asm", "emu_peeled_matches_interp"),
            ol("tr_identity", "mutant_probe"),
        ],
    },
    Row {
        name: "budget_no_store",
        owner: "P5b",
        dets: &[
            d("tr_asm", "hand_budget_charge_and_exit_value"),
            d("tr_asm", "emu_rolled_matches_interp"),
            ol("tr_identity", "mutant_probe"),
        ],
    },
    // register allocation and exits
    Row {
        name: "phi_skip_move",
        owner: "P5a",
        dets: &[d("tr_ra", "allocation_matches_interpreter"), d("tr_ra", "swap_cycles")],
    },
    Row {
        name: "phi_swap",
        owner: "P5a",
        dets: &[
            d("tr_ra", "phi_swap_cycles"),
            d("tr_ra", "parallel_moves_match_simultaneous_assignment"),
        ],
    },
    Row {
        name: "spill_no_reload",
        owner: "P5a",
        dets: &[d("tr_ra", "pressure"), d("tr_ra", "allocation_matches_interpreter")],
    },
    Row {
        name: "exit_rename_ignored",
        owner: "P5a",
        dets: &[
            d("tr_ra", "shared_snapshots_keep_exit_maps_identical"),
            d("tr_ra", "allocation_matches_interpreter"),
        ],
    },
    Row {
        name: "fcmp_negated",
        owner: "P5b",
        dets: &[d("tr_asm", "emu_peeled_matches_interp")],
    },
    // folding, aliasing, forwarding, CSE
    Row {
        name: "fold_nan",
        owner: "P2",
        dets: &[
            d("tr_passes", "float_folds_never_decide_nan_or_signed_zero"),
            d("tr_passes", "float_const_folds_equal_the_vm_arithmetic_or_stay"),
        ],
    },
    Row {
        name: "fold_x_plus_zero",
        owner: "P2",
        dets: &[
            d("tr_passes", "float_folds_never_decide_nan_or_signed_zero"),
            d("tr_passes", "float_identities_differential"),
        ],
    },
    Row {
        name: "alias_may_as_no",
        owner: "P7",
        dets: P7_BINS,
    },
    Row {
        name: "alias_upval_by_index",
        owner: "P7",
        dets: P7_BINS,
    },
    Row {
        name: "fwd_across_store",
        owner: "P7",
        dets: P7_BINS,
    },
    Row {
        name: "alias_maytable_as_must",
        owner: "P7",
        dets: P7_BINS,
    },
    Row {
        name: "cse_across_store",
        owner: "P7",
        dets: P7_BINS,
    },
    Row {
        name: "tbar_skip",
        owner: "P5b",
        dets: &[
            d(
                "tr_asm",
                "hand_tbar_calls_the_barrier_exactly_for_black_tables_and_white_values",
            ),
            d("tr_asm", "e2e_tbar_in_an_incremental_gc_cycle"),
        ],
    },
    // entry checks, guards, polls
    Row {
        name: "entry_no_extent",
        owner: "P5b",
        dets: &[d("tr_asm", "hand_entry_checks_tr6")],
    },
    Row {
        name: "entry_no_ci_spare",
        owner: "P5b",
        dets: &[d("tr_asm", "hand_entry_checks_tr6")],
    },
    Row {
        name: "entry_no_typecheck",
        owner: "P5b",
        dets: &[
            d("tr_asm", "emu_peeled_matches_interp"),
            d(
                "tr_asm",
                "hand_entry_failure_bookkeeping_follows_the_free_peel_rule",
            ),
        ],
    },
    Row {
        name: "callee_guard_skip",
        owner: "P3b",
        dets: &[d("tr_record_calls", "callee_identity_guards_are_recorded")],
    },
    Row {
        name: "upval_range_skip",
        owner: "P3a",
        dets: &[
            d("tr_record", "upvalue_accesses_have_the_range_guard"),
            d("tr_record", "upvalue_range_guard_exits"),
        ],
    },
    Row {
        name: "poll_skip",
        owner: "P5b",
        dets: &[d("tr_asm", "hand_poll_trap_and_interrupt")],
    },
    // frames, lifetime
    Row {
        name: "frame_savedpc_caller_off",
        owner: "P4",
        dets: &[
            d("tr_exit", "restore_matches_the_model_for_0_to_4_frames"),
            d("tr_exit", "twin_restore_equals_the_interpreter_for_0_to_4_frames"),
        ],
    },
    Row {
        name: "frame_nresults_off",
        owner: "P4",
        dets: &[
            d("tr_exit", "restore_matches_the_model_for_0_to_4_frames"),
            d("tr_exit", "twin_restore_equals_the_interpreter_for_0_to_4_frames"),
        ],
    },
    Row {
        name: "dep_edge_skip",
        owner: "P6",
        dets: &[
            d(
                "tr_runtime",
                "lifetime_freeing_an_inlined_callee_flushes_the_dependent_trace",
            ),
            d(
                "tr_runtime",
                "lifetime_a_dependency_freed_during_a_recording_aborts_it",
            ),
        ],
    },
    Row {
        name: "flush_in_handler",
        owner: "P6",
        dets: &[
            d(
                "tr_runtime",
                "deferred_flush_requested_inside_the_exit_handler_runs_at_the_next_safe_point",
            ),
            d("tr_runtime", "deferred_flush_api_semantics"),
            d(
                "tr_runtime",
                "a_collection_inside_the_exit_handler_does_not_free_the_running_trace",
            ),
            d(
                "tr_exit",
                "handler::flush_requested_inside_the_handler_is_deferred_and_the_trace_memory_stays_valid",
            ),
        ],
    },
    Row {
        name: "peel_subst_stale",
        owner: "P7",
        dets: P7_BINS,
    },
    // sinking (F8), inlined frames, hooks, validation, memory accounting
    Row {
        name: "sink_escape_ignored",
        owner: "S0",
        dets: &[dl("tr_core", "sink_programs_run_on_traces")],
    },
    Row {
        name: "sink_store_order",
        owner: "S0",
        dets: &[dl("tr_core", "sink_programs_run_on_traces")],
    },
    Row {
        name: "sink_slots_after_alloc",
        owner: "S0",
        dets: &[dl("tr_core", "sink_programs_run_on_traces")],
    },
    Row {
        name: "sink_no_gcstp",
        owner: "S0",
        dets: &[dl("tr_core", "sink_programs_run_on_traces"), dl("tr_core", "sink_programs_under_gc_stress")],
    },
    Row {
        name: "popped_frame_drop",
        owner: "P3b",
        dets: &[
            d("tr_record_calls", "popped_frame_slots_stay_in_snapshots"),
            d("tr_record_calls", "ir_replay_matches_the_interpreter"),
        ],
    },
    Row {
        name: "native_room_skip",
        owner: "P3b",
        dets: &[d("tr_record_calls", "room_and_ci_conditions")],
    },
    Row {
        name: "native_ci_skip",
        owner: "P3b",
        dets: &[
            d("tr_record_calls", "room_and_ci_conditions"),
            d("tr_record_calls", "frames_and_snapshots_are_well_formed"),
        ],
    },
    Row {
        name: "hooked_head_no_trap",
        owner: "P1",
        dets: &[o("tr_hook", "recorder_log_equals_the_instruction_oracle")],
    },
    Row {
        name: "rec_no_validate",
        owner: "P3a",
        dets: &[
            d(
                "tr_record",
                "stale_context_is_rejected_by_the_recorder_validation",
            ),
            d(
                "tr_record_calls",
                "stale_context_in_a_reused_callee_frame_is_rejected",
            ),
        ],
    },
    Row {
        name: "phi_before_guard",
        owner: "P5a",
        dets: &[d("tr_ra", "loop_slice"), d("tr_ra", "allocation_matches_interpreter")],
    },
    Row {
        name: "vbytes_no_credit",
        owner: "F-memory",
        dets: &[d("t15_runtime", "a_failed_recompile_refunds_its_version_bytes")],
    },
    Row {
        name: "vbytes_no_limit_guard",
        owner: "F-memory",
        dets: &[d("t15_runtime", "memory_limit_refuses_a_promotion_and_the_nest_retries_later")],
    },
    // 5.4 (F7a)
    Row {
        name: "atouch_skip",
        owner: "F7a",
        dets: &[dl("tr_core", "mutant_probe_live_programs")],
    },
    Row {
        name: "atouch_hoisted",
        owner: "F7a",
        dets: &[dl("tr_core", "mutant_probe_live_programs")],
    },
    Row {
        name: "forloop54_no_ctl_copy",
        owner: "F7a",
        dets: &[
            d("tr_record", "golden_ir_dumps"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    // v1.3
    Row {
        name: "entry_guard_past_store",
        owner: "P3b",
        dets: &[
            d("tr_record_calls", "entry_guard_snapshot_rules"),
            d("tr_record_calls", "nest_root_shape_and_entry_guard_rules"),
        ],
    },
    Row {
        name: "leave_trap_kept",
        owner: "P3a",
        dets: &[d("tr_record", "traps_are_clear_after_the_recording")],
    },
    Row {
        name: "nonslot_phi_skip",
        owner: "P7",
        dets: P7_BINS,
    },
    Row {
        name: "rolled_no_writeback",
        owner: "P5b",
        dets: &[
            d("tr_asm", "emu_rolled_matches_interp"),
            d("tr_asm", "e2e_traces_run_and_equal_the_interpreter"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "dep_edge_at_install",
        owner: "P3b",
        dets: &[
            d("tr_record_calls", "dep_edges_are_registered_when_inlining"),
        ],
    },
    Row {
        name: "cell_stale_version",
        owner: "F5",
        dets: &[d("tr_policy", "j4_respecialisation_rewrites_every_non_trace_cell")],
    },
    // P4c: exit restore (additive)
    Row {
        name: "exit_kcache_wrong",
        owner: "P4c",
        dets: &[
            d("tr_exit", "restore_matches_the_model_for_0_to_4_frames"),
            d("tr_exit", "twin_restore_equals_the_interpreter_for_0_to_4_frames"),
        ],
    },
    Row {
        name: "exit_top_off_by_one",
        owner: "P4c",
        dets: &[
            d("tr_exit", "restore_matches_the_model_for_0_to_4_frames"),
            d(
                "tr_exit",
                "twin_resume_from_the_restored_frames_equals_the_interpreter_alone",
            ),
        ],
    },
    Row {
        name: "exit_th_top_not_written",
        owner: "P4c",
        dets: &[
            d("tr_exit", "restore_matches_the_model_for_0_to_4_frames"),
            d("tr_exit", "twin_restore_equals_the_interpreter_for_0_to_4_frames"),
        ],
    },
    Row {
        name: "exit_refund_sign_flipped",
        owner: "P4c",
        dets: &[
            d("tr_exit", "restore_matches_the_model_for_0_to_4_frames"),
            d("tr_exit", "inline_stub_equals_restore_for_every_budget_kind"),
            ol("tr_identity", "mutant_probe"),
        ],
    },
    // P7b rows (exact detecting tests; the first two also run under the P7 whole-binary rows above).
    Row {
        name: "bounds_key_no_bound",
        owner: "P7b",
        dets: &[d("tr_loop", "bounds_checks_of_different_tables_are_kept")],
    },
    Row {
        name: "hdr_op_no_barrier",
        owner: "P7b",
        dets: &[d("tr_loop", "header_mutating_ops_stop_the_optimiser")],
    },
    Row {
        name: "forloop_step_guard_skip",
        owner: "P7b",
        dets: &[d("tr_loop", "recorded::forloop_records_a_step_guard")],
    },
    Row {
        name: "forloop_step_add_wrong",
        owner: "P7b",
        dets: &[d("tr_loop", "recorded::forloop_records_a_step_guard")],
    },
    Row {
        name: "cycle_stale_xmm",
        owner: "Q1",
        dets: &[d("tr_ra", "cycles_of_every_length_use_v7")],
    },
    // Q2: ineffective-trace rule
    Row {
        name: "ineffective_never",
        owner: "Q2",
        dets: &[dl("tr_core", "ineffective_trace_rule")],
    },
    // Q3: identical-regeneration rule
    Row {
        name: "identical_reinstall",
        owner: "Q3",
        dets: &[dl("tr_core", "identical_regeneration_is_not_installed")],
    },
    // F7a: the 5.4 `ATOUCH` assembly (a differential on random IR with hinted tables, and the end-to-end twins)
    Row {
        name: "alimit_touch",
        owner: "F7a",
        dets: &[
            d("tr_asm", "hand_atouch_follows_alimit_touch"),
            d("tr_asm", "emu_peeled_matches_interp"),
            d("tr_asm", "emu_rolled_matches_interp"),
        ],
    },
    // F1: the hash part (assembly differential on random IR, the optimiser's equivalence on random iterations, the
    // recorder's guards, the live `field_*` programs)
    Row {
        name: "hrefk_key_skip",
        owner: "F1",
        dets: &[
            d("tr_asm", "random_traces_cover_the_hash_ops"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "hrefk_lsize_skip",
        owner: "F1",
        dets: &[d("tr_record", "hash_ops_record_their_guards")],
    },
    Row {
        name: "hload_typecheck_skip",
        owner: "F1",
        dets: &[
            d("tr_asm", "random_traces_cover_the_hash_ops"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "hstore_oldcheck_skip",
        owner: "F1",
        dets: &[
            d("tr_asm", "random_traces_cover_the_hash_ops"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "hstore_oldcheck_dropped",
        owner: "F1",
        dets: &[
            d("tr_loop", "rolled_iterations_equal_peeled_traces"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "hstore_tbar_skip",
        owner: "F1",
        dets: &[
            d("tr_asm", "e2e_tbar_after_hstore_in_gc_cycles"),
            d("tr_record", "hash_ops_record_their_guards"),
        ],
    },
    Row {
        name: "alias_hashtab_as_must",
        owner: "F1",
        dets: &[
            d("tr_loop", "rolled_iterations_equal_peeled_traces"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "meta_flag_skip",
        owner: "F1",
        dets: &[
            d("tr_record", "hash_ops_record_their_guards"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "hash_nil_meta_skip",
        owner: "F1",
        dets: &[
            d("tr_record", "hash_ops_record_their_guards"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    // F2a: side traces (exit cells, side recording, count model of a mid-block start, link, family lifetime, prelude)
    Row {
        name: "side_budget_no_rem",
        owner: "F2a",
        dets: &[
            d("tr_record_calls", "side_recording_shape_and_count_model"),
            ol("tr_core", "instruction_count_identity"),
            ol("tr_oracle", "mutant_probe"),
        ],
    },
    Row {
        name: "side_start_refund",
        owner: "F2a",
        dets: &[
            d("tr_record_calls", "side_recording_shape_and_count_model"),
            ol("tr_core", "instruction_count_identity"),
            ol("tr_oracle", "mutant_probe"),
        ],
    },
    Row {
        name: "side_known_all",
        owner: "F2a",
        dets: &[
            d("tr_record_calls", "side_known_types_skip_typechecks"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "exit_cell_ignored",
        owner: "F2a",
        dets: &[dl("tr_core", "side_traces_install_and_run")],
    },
    Row {
        name: "side_cell_stale",
        owner: "F2a",
        dets: &[d(
            "tr_exit",
            "handler::side_traces_attach_to_exit_cells_detach_alone_and_flush_with_their_family",
        )],
    },
    Row {
        name: "side_family_flush_skip",
        owner: "F2a",
        dets: &[d(
            "tr_exit",
            "handler::side_traces_attach_to_exit_cells_detach_alone_and_flush_with_their_family",
        )],
    },
    Row {
        name: "side_link_no_poll",
        owner: "F2a",
        dets: &[d(
            "tr_asm",
            "hand_side_trace_link_polls_stores_the_budget_and_jumps_through_the_head_cell",
        )],
    },
    Row {
        name: "side_link_no_store",
        owner: "F2a",
        dets: &[
            d(
                "tr_asm",
                "hand_side_trace_link_polls_stores_the_budget_and_jumps_through_the_head_cell",
            ),
            ol("tr_core", "instruction_count_identity"),
        ],
    },
    Row {
        name: "side_interp_no_rem",
        owner: "F2a",
        dets: &[
            // (Q5: a small fixed family first: the long identity run alone was not a reliable detector in the runner)
            ol("tr_core", "side_recording_start_keeps_the_count_identity"),
            ol("tr_core", "instruction_count_identity"),
            ol("tr_oracle", "mutant_probe"),
        ],
    },
    Row {
        name: "side_maxside_ignored",
        owner: "F2a",
        dets: &[dl("tr_core", "side_traces_install_and_run")],
    },
    Row {
        name: "flush_keeps_cells",
        owner: "F2a",
        dets: &[d(
            "tr_exit",
            "handler::a_trace_flushed_inside_its_handler_leaves_its_cells_at_the_tier1_addresses",
        )],
    },
    Row {
        name: "ineffective_pct_ignored",
        owner: "F2a",
        dets: &[d(
            "tr_exit",
            "handler::ineffective_ratio_is_lower_with_a_type_guard_side_trace",
        )],
    },
    Row {
        name: "prelude_never",
        owner: "F2a",
        dets: &[dl("tr_core", "entry0_prelude")],
    },
    Row {
        name: "prelude_no_chain",
        owner: "F2a",
        dets: &[dl("tr_core", "side_traces_install_and_run")],
    },
    Row {
        name: "side_short_ignored",
        owner: "F2a",
        dets: &[dl("tr_core", "side_traces_install_and_run")],
    },
    Row {
        name: "nest_link_never",
        owner: "F2b",
        dets: &[dl("tr_core", "nest_links")],
    },
    Row {
        name: "side_untraced_retry",
        owner: "F2b",
        dets: &[dl("tr_core", "nest_links")],
    },
    Row {
        name: "side_forloop_end_abort",
        owner: "F2b",
        dets: &[dl("tr_core", "nest_for_loops")],
    },
    Row {
        name: "forprep_step_guard_skip",
        owner: "F2b",
        // (P-F: the literal-step guard is redundant, only the run-time-step sign guard it also skips is observable)
        dets: &[
            dl("tr_core", "nest_for_loops"),
            dl("tr_core", "pf_variable_step_loops"),
            d("tr_loop", "recorded::forprep_literal_and_variable_steps"),
        ],
    },
    Row {
        name: "frame_link_never",
        owner: "F2b",
        dets: &[dl("tr_core", "frame_links_and_stitches")],
    },
    Row {
        name: "frame_link_cell_ignored",
        owner: "F2b",
        dets: &[d(
            "tr_exit",
            "handler::a_linking_exit_continues_at_the_dispatch_cell_of_its_target",
        )],
    },
    Row {
        name: "stitch_never",
        owner: "F2b",
        dets: &[dl("tr_core", "frame_links_and_stitches")],
    },
    // F3: class-A helpers
    Row {
        name: "tnew_no_gc_check",
        owner: "F3",
        dets: &[d("tr_class_a", "tnew_exits_exactly_when_a_step_would_be_due")],
    },
    Row {
        name: "tnew_bytes_small",
        owner: "F3",
        dets: &[d("tr_class_a", "tnew_exits_exactly_when_a_step_would_be_due")],
    },
    Row {
        name: "newref_present_ok",
        owner: "F3",
        dets: &[d("tr_class_a", "newref_inserts_absent_keys_only")],
    },
    Row {
        name: "tnew_ne_guard_skip",
        owner: "F3",
        dets: &[dl("tr_core", "class_a_helpers_under_gc_stress")],
    },
    Row {
        name: "newref_ne_guard_skip",
        owner: "F3",
        dets: &[dl("tr_core", "class_a_helpers_run_on_traces")],
    },
    Row {
        name: "newref_no_newindex_guard",
        owner: "F3",
        dets: &[dl("tr_core", "class_a_helpers_run_on_traces")],
    },
    Row {
        name: "tlen_meta_skip",
        owner: "F3",
        dets: &[dl("tr_core", "class_a_helpers_run_on_traces")],
    },
    Row {
        name: "newref_model_stale",
        owner: "F3",
        dets: &[dl("tr_core", "class_a_helpers_run_on_traces")],
    },
    Row {
        name: "newref_header_safe",
        owner: "F3",
        dets: &[d("tr_loop", "header_mutating_ops_stop_the_optimiser")],
    },
    // F5: policy integration with J4 (TR19/TR19b)
    Row {
        name: "w_cell_skip",
        owner: "F5",
        dets: &[
            d("tr_policy", "cells_follow_head_state_and_nest_eligibility"),
            d("tr_policy", "promotion_blacks_the_cold_heads_of_the_nest"),
        ],
    },
    Row {
        name: "region_wait_stale",
        owner: "F5",
        dets: &[d("tr_policy", "a_black_head_whose_nest_is_traced_falls_into_tier_1")],
    },
    Row {
        name: "close_nest_keep_warm",
        owner: "F5",
        dets: &[d("tr_policy", "promotion_blacks_the_cold_heads_of_the_nest")],
    },
    Row {
        name: "nest_rule_skip",
        owner: "F5",
        dets: &[d("tr_policy", "the_nest_rule_hands_the_nest_to_j4")],
    },
    Row {
        name: "retired_freed_early",
        owner: "F5",
        dets: &[d("tr_policy", "j4_respecialisation_rewrites_every_non_trace_cell")],
    },
    Row {
        name: "nest_rule_inlined",
        owner: "F5",
        dets: &[d("tr_policy", "the_nest_rule_keeps_inner_traces_with_inlined_calls")],
    },
    // F3b: generic `for` (ipairs / next), SNEW / CONCAT, STRTO
    Row {
        name: "tfor_callee_guard_skip",
        owner: "F3b",
        dets: &[d("tr_record", "f3b_ops_record_their_guards"), dl("tr_core", "f3b_programs_run_on_traces")],
    },
    Row {
        name: "tnext_ne_guard_skip",
        owner: "F3b",
        dets: &[d("tr_record", "f3b_ops_record_their_guards"), dl("tr_core", "f3b_programs_run_on_traces")],
    },
    Row {
        name: "tnext_key_type_skip",
        owner: "F3b",
        dets: &[dl("tr_core", "f3b_programs_run_on_traces")],
    },
    Row {
        name: "snew_no_gc_check",
        owner: "F3b",
        dets: &[d("tr_class_a_f3b", "snew_exits_exactly_when_a_step_would_be_due")],
    },
    Row {
        name: "snew_bytes_small",
        owner: "F3b",
        dets: &[d("tr_class_a_f3b", "snew_exits_exactly_when_a_step_would_be_due")],
    },
    Row {
        name: "concat_no_gc_check",
        owner: "F3b",
        dets: &[d("tr_class_a_f3b", "concat_exits_exactly_when_a_step_would_be_due")],
    },
    Row {
        name: "concat_bytes_small",
        owner: "F3b",
        dets: &[d("tr_class_a_f3b", "concat_exits_exactly_when_a_step_would_be_due")],
    },
    Row {
        name: "concat_kind_skip",
        owner: "F3b",
        dets: &[d("tr_class_a_f3b", "concat_leaves_special_cases_to_the_interpreter")],
    },
    Row {
        name: "snew_ne_guard_skip",
        owner: "F3b",
        dets: &[d("tr_record", "f3b_ops_record_their_guards"), dl("tr_core", "f3b_programs_under_gc_stress")],
    },
    Row {
        name: "concat_ne_guard_skip",
        owner: "F3b",
        dets: &[d("tr_record", "f3b_ops_record_their_guards"), dl("tr_core", "f3b_programs_under_gc_stress")],
    },
    Row {
        name: "concat_conv_slot_skip",
        owner: "F3b",
        dets: &[d("tr_record", "f3b_ops_record_their_guards")],
    },
    Row {
        name: "strto_ne_guard_skip",
        owner: "F3b",
        dets: &[dl("tr_core", "f3b_programs_run_on_traces")],
    },
    Row {
        name: "strto_mode_swap",
        owner: "F3b",
        dets: &[dl("tr_core", "f3b_programs_run_on_traces")],
    },
    Row {
        name: "tfor54_no_ctl_copy",
        owner: "F3b",
        dets: &[dl("tr_core", "f3b_programs_run_on_traces")],
    },
    Row {
        name: "tfor_lua_ret_cost_skip",
        owner: "Q",
        dets: &[dl("tr_core", "instruction_count_identity")],
    },
    Row {
        name: "concat_key_traced",
        owner: "F3b",
        dets: &[d("tr_record", "f3b_ops_record_their_guards")],
    },
    // P-E: inlined vararg callees and `select`
    Row {
        name: "tail_flag_skip",
        owner: "F4",
        dets: &[dl("tr_core", "f4_programs_run_on_traces")],
    },
    Row {
        name: "exit_top_ignored",
        owner: "F4",
        dets: &[d("tr_exit", "restore_matches_the_model_for_0_to_4_frames"), dl("tr_core", "f4_unroll_limits_and_the_exit_beyond")],
    },
    Row {
        name: "exit_top_off_by_one_slot",
        owner: "F4",
        dets: &[d("tr_exit", "restore_matches_the_model_for_0_to_4_frames"), dl("tr_core", "f4_unroll_limits_and_the_exit_beyond")],
    },
    Row {
        name: "snap_top_dropped",
        owner: "F4",
        dets: &[d("tr_record_calls", "multret_consumers_carry_top"), dl("tr_core", "f4_unroll_limits_and_the_exit_beyond")],
    },
    Row {
        name: "top_count_off",
        owner: "F4",
        dets: &[d("tr_record_calls", "ir_replay_matches_the_interpreter"), dl("tr_core", "f4_programs_run_on_traces")],
    },
    Row {
        name: "unroll_off_by_one",
        owner: "F4",
        dets: &[d("tr_record_calls", "recursion_unrolls_to_the_limit_and_exits_beyond")],
    },
    Row {
        name: "tail_args_not_moved",
        owner: "F4",
        dets: &[d("tr_record_calls", "ir_replay_matches_the_interpreter"), dl("tr_core", "f4_programs_run_on_traces")],
    },
    Row {
        name: "tail_nil_fill_skip",
        owner: "F4",
        dets: &[d("tr_record_calls", "ir_replay_matches_the_interpreter"), dl("tr_core", "f4_programs_run_on_traces")],
    },
    // S0: allocation sinking, TR3b writeback
    Row {
        name: "sink_vbytes_zero",
        owner: "S0",
        dets: &[dl("tr_core", "sink_byte_count_matches_the_interpreter")],
    },
    Row {
        name: "sink_no_uncharge",
        owner: "S0",
        dets: &[dl("tr_core", "sink_byte_count_matches_the_interpreter")],
    },
    Row {
        name: "wb_loaded_slot_stored",
        owner: "S0",
        dets: &[dl("tr_core", "sink_programs_run_on_traces")],
    },
    Row {
        name: "sink_mem_error_dropped",
        owner: "F8",
        dets: &[dl("tr_core", "sink_materialise_alloc_failure_raises_memory_error")],
    },
    Row {
        name: "rec_no_watch",
        owner: "F8",
        dets: &[dl("tr_core", "sink_programs_under_gc_stress")],
    },
    // P-A: `__index` chains (absent-key proof HMISS, metatable and `__index` guards, end-of-chain flag guard)
    Row {
        name: "hmiss_skip",
        owner: "P-A",
        dets: &[
            d("tr_record", "index_chain_records_its_guards"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "hmiss_lsize_skip",
        owner: "P-A",
        dets: &[
            d("tr_record", "index_chain_records_its_guards"),
        ],
    },
    Row {
        name: "hmiss_next_skip",
        owner: "P-A",
        dets: &[
            d("tr_asm", "random_traces_cover_the_hash_ops"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "hmiss_key_skip",
        owner: "P-A",
        dets: &[
            d("tr_asm", "random_traces_cover_the_hash_ops"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "index_mt_null_skip",
        owner: "P-A",
        dets: &[
            d("tr_record", "index_chain_records_its_guards"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "index_tab_typecheck_skip",
        owner: "P-A",
        dets: &[
            d("tr_record", "index_chain_records_its_guards"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "index_nilnode_skip",
        owner: "P-A",
        dets: &[
            d("tr_record", "index_chain_records_its_guards"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    Row {
        name: "chain_end_skip",
        owner: "P-A",
        dets: &[
            d("tr_record", "index_chain_records_its_guards"),
            dl("tr_core", "mutant_probe_live_programs"),
        ],
    },
    // P-E: inlined vararg callees and `select`
    Row {
        name: "vararg_nextra_off",
        owner: "P-E",
        dets: &[d("tr_exit", "restore_matches_the_model_for_0_to_4_frames")],
    },
    Row {
        name: "vararg_room_skip",
        owner: "P-E",
        dets: &[d("tr_record_calls", "vararg_room_and_layout")],
    },
    Row {
        name: "vararg_orig_not_nilled",
        owner: "P-E",
        dets: &[d("tr_record_calls", "vararg_replay_matches_the_interpreter")],
    },
    Row {
        name: "vararg_delta_off",
        owner: "P-E",
        dets: &[d("tr_record_calls", "vararg_replay_matches_the_interpreter")],
    },
    Row {
        name: "vararg_ret_dest",
        owner: "P-E",
        dets: &[d("tr_record_calls", "vararg_replay_matches_the_interpreter")],
    },
    Row {
        name: "vararg_first_nil",
        owner: "P-E",
        dets: &[d("tr_record_calls", "vararg_replay_matches_the_interpreter")],
    },
    Row {
        name: "vararg_top_off",
        owner: "P-E",
        dets: &[d("tr_record_calls", "vararg_replay_matches_the_interpreter")],
    },
    Row {
        name: "select_guard_skip",
        owner: "P-E",
        dets: &[d("tr_record_calls", "select_selectors_are_guarded")],
    },
    Row {
        name: "select_count_off",
        owner: "P-E",
        dets: &[d("tr_record_calls", "vararg_replay_matches_the_interpreter")],
    },
    Row {
        name: "select_index_off",
        owner: "P-E",
        dets: &[d("tr_record_calls", "vararg_replay_matches_the_interpreter")],
    },
    // P-E2: function-entry roots and `table.pack`
    Row {
        name: "fn_exits_not_pre",
        owner: "P-E2",
        dets: &[dl("tr_core", "fn_root_with_a_dead_path_is_flushed_ineffective")],
    },
    Row {
        name: "fn_ineff_pct_ignored",
        owner: "P-E2",
        dets: &[dl("tr_core", "fn_root_with_a_mixed_path_is_flushed_ineffective")],
    },
    Row {
        name: "pack_asize_off",
        owner: "P-E2",
        dets: &[dl("tr_core", "pe2_programs_run_on_traces")],
    },
    Row {
        name: "pack_n_off",
        owner: "P-E2",
        dets: &[dl("tr_core", "pe2_programs_run_on_traces")],
    },
    Row {
        name: "fn_head_calls_ignored",
        owner: "P-E2",
        dets: &[d("tr_runtime", "a_loop_free_function_without_a_call_has_no_entry_head")],
    },
    Row {
        name: "fn_side_at_return",
        owner: "P-E2",
        dets: &[dl("tr_core", "fn_root_final_exit_gets_no_side_trace")],
    },
    Row {
        name: "fn_head_tbc_ignored",
        owner: "P-E2",
        dets: &[Det {
            bin: "tr_oracle",
            test: "oracle_exits",
            need: Need::Oracle,
            env: &[("TR_CORE_TIMEOUT", "100"), ("TR_P8A_TIMEOUT", "100"), ("TR_P8A_ONLY", "meta_09_close"), ("TR_P8A_LIMITS", "fnroot=1")],
        }],
    },
    Row {
        name: "fn_short_budget_ignored",
        owner: "P-E2",
        dets: &[dl("tr_core", "fn_root_short_recordings_blacklist_the_head")],
    },
    Row {
        name: "lib_ne_guard_skip",
        owner: "P-B",
        dets: &[dl("tr_record", "pb_ops_record_their_guards")],
    },
    Row {
        name: "lib_bytes_small",
        owner: "P-B",
        dets: &[dl("tr_class_a_pb", "lib_helpers_exit_exactly_when_a_step_would_be_due")],
    },
    Row {
        name: "lib_no_gc_check",
        owner: "P-B",
        dets: &[dl("tr_class_a_pb", "lib_helpers_exit_exactly_when_a_step_would_be_due")],
    },
    Row {
        name: "lib_kind_skip",
        owner: "P-B",
        dets: &[dl("tr_class_a_pb", "lib_sub_long_strings_and_the_result_kind")],
    },
    Row {
        name: "sub_start_off",
        owner: "P-B",
        dets: &[dl("tr_class_a_pb", "lib_sub_follows_the_c_function")],
    },
    Row {
        name: "byte_plus1_skip",
        owner: "P-B",
        dets: &[dl("tr_class_a_pb", "lib_byte_is_the_byte_plus_one")],
    },
    Row {
        name: "type_closure_unchecked",
        owner: "P-B",
        dets: &[dl("tr_class_a_pb", "lib_type_checks_the_closure")],
    },
    Row {
        name: "tostr_meta_skip",
        owner: "P-B",
        dets: &[dl("tr_class_a_pb", "lib_tostr_leaves_numbers_with_metatables_alone")],
    },
    Row {
        name: "tinsert_meta_skip",
        owner: "P-B",
        dets: &[dl("tr_record", "pb_ops_record_their_guards")],
    },
    Row {
        name: "streq_guard_skip",
        owner: "P-B",
        dets: &[dl("tr_record", "pb_ops_record_their_guards")],
    },
    Row {
        name: "tlen_str_wide",
        owner: "P-B",
        dets: &[dl("tr_core", "pb_programs_run_on_traces")],
    },
    Row {
        name: "storm_guard_skip",
        owner: "P-B",
        dets: &[dl("tr_record", "pb_storm_guard_aborts_allocating_loops")],
    },
    Row {
        name: "storm_rule_skip",
        owner: "P-B",
        dets: &[dl("tr_core", "pb_alloc_storm_gives_the_loop_back")],
    },
    Row {
        name: "storm_site_skip",
        owner: "P-B",
        dets: &[dl("tr_exit", "pb_allocation_guards_are_alloc_sites")],
    },
    // P-F: run-time loop steps
    Row {
        name: "forloop_step_always_var",
        owner: "P-F",
        dets: &[d("tr_loop", "recorded::forloop_records_a_step_guard")],
    },
    Row {
        name: "forloop_step_always_lit",
        owner: "P-F",
        dets: &[
            d("tr_loop", "recorded::forloop_variable_step_is_not_a_constant"),
            dl("tr_core", "pf_variable_step_loops"),
        ],
    },
    Row {
        name: "forprep_step_always_lit",
        owner: "P-F",
        dets: &[
            d("tr_loop", "recorded::forprep_literal_and_variable_steps"),
            dl("tr_core", "pf_variable_step_loops"),
        ],
    },
    Row {
        name: "forprep_var_sign_skip",
        owner: "P-F",
        dets: &[
            d("tr_loop", "recorded::forprep_literal_and_variable_steps"),
            dl("tr_core", "pf_variable_step_loops"),
        ],
    },
    Row {
        name: "sidemin_default_ignored",
        owner: "P-F",
        dets: &[dl("tr_policy", "pf_process_default_sidemin")],
    },
    Row {
        name: "forprep_huge_diff_record",
        owner: "P-F",
        dets: &[dl("tr_core", "pf_variable_step_loops")],
    },
    Row {
        name: "poll_exit_no_trap",
        owner: "P-L",
        dets: &[dl("tr_poll", "raised_poll_word_is_consumed_by_one_poll_exit")],
    },
    Row {
        name: "poll_exit_flag_kept",
        owner: "P-L",
        dets: &[dl("tr_poll", "raised_poll_word_is_consumed_by_one_poll_exit")],
    },
    Row {
        name: "exit_mem_counter_no_divert",
        owner: "P-L",
        dets: &[dl("tr_core", "pl_exit_counter_in_memory_without_a_free_register")],
    },
    Row {
        name: "exit_mem_counter_no_count",
        owner: "P-L",
        dets: &[dl("tr_core", "pl_exit_counter_in_memory_without_a_free_register")],
    },
    Row {
        name: "mod_pow2_mask_k",
        owner: "P-N",
        dets: &[dl("tr_core", "pn_pow2_divmod")],
    },
    Row {
        name: "idiv_pow2_logical",
        owner: "P-N",
        dets: &[dl("tr_core", "pn_pow2_divmod")],
    },
    Row {
        name: "land_type_check_skip",
        owner: "P-X",
        dets: &[dl("tr_core", "px_typed_landing_labels"), dl("tr_core", "pm4_recursion_typed_landing")],
    },
    Row {
        name: "strmt_flush_skip",
        owner: "P-V",
        dets: &[dl("tr_core", "pv_string_methods")],
    },
    Row {
        name: "rec_push_no_ci_next",
        owner: "P-M3",
        dets: &[dl("tr_core", "pm3_programs_run_on_traces")],
    },
    Row {
        name: "rec_push_top_off",
        owner: "P-M3",
        dets: &[dl("tr_core", "pm3_programs_run_on_traces")],
    },
    Row {
        name: "rec_push_callstatus_off",
        owner: "P-M3",
        dets: &[dl("tr_core", "pm3_programs_run_on_traces")],
    },
    Row {
        name: "rec_push_savedpc_off",
        owner: "P-M3",
        dets: &[dl("tr_core", "pm3_programs_run_on_traces")],
    },
    Row {
        name: "rec_ret_site_unchecked",
        owner: "P-M3",
        dets: &[dl("tr_core", "pm3_programs_run_on_traces")],
    },
    Row {
        name: "rec_ret_base_off",
        owner: "P-M3",
        dets: &[dl("tr_core", "pm3_programs_run_on_traces")],
    },
    Row {
        name: "rec_inner_room_skip",
        owner: "P-M3",
        dets: &[dl("tr_core", "pm3_programs_run_on_traces")],
    },
    Row {
        name: "pcall_impure_ignored",
        owner: "P-O",
        dets: &[d("tr_record_calls", "pcall_bodies_with_effects_before_a_guard_abort")],
    },
    Row {
        name: "pcall_shift_off",
        owner: "P-O",
        dets: &[d("tr_record_calls", "pcall_room_ci_and_snapshots"), dl("tr_core", "po_protected_calls")],
    },
    Row {
        name: "pcall_ccalls_skip",
        owner: "P-O",
        dets: &[d("tr_record_calls", "pcall_room_ci_and_snapshots")],
    },
    Row {
        name: "pcall_room_skip",
        owner: "P-O",
        dets: &[d("tr_record_calls", "pcall_room_ci_and_snapshots")],
    },
    Row {
        name: "ccalls_guard_off",
        owner: "P-O",
        dets: &[d("tr_class_a_pb", "lib_ccalls_follows_the_c_stack_depth")],
    },
    Row {
        name: "ccalls_inline_off_by_one",
        owner: "BUNDLE-D",
        dets: &[d("tr_core", "bd_ccalls_programs")],
    },
    Row {
        name: "armm_never",
        owner: "BUNDLE-D",
        dets: &[d("tr_bd", "bd_armm_lowering")],
    },
    Row {
        name: "armm_count_skip",
        owner: "BUNDLE-D",
        dets: &[o("tr_core", "bd_armm_programs")],
    },
    Row {
        name: "armm_land_skip",
        owner: "BUNDLE-D",
        dets: &[d("tr_core", "bd_armm_programs")],
    },
    Row {
        name: "armm_pay_skip",
        owner: "BUNDLE-D",
        dets: &[o("tr_core", "bd_armm_programs")],
    },
    Row {
        name: "armm_dst_off",
        owner: "BUNDLE-D",
        dets: &[d("tr_core", "bd_armm_programs")],
    },
    Row {
        name: "stitch_side_empty",
        owner: "BUNDLE-F",
        dets: &[dl("tr_core", "bf_stitch0_programs")],
    },
    Row {
        name: "fd_room_extent_off",
        owner: "BUNDLE-F",
        dets: &[dl("tr_core", "bf_fibdiet_programs")],
    },
    Row {
        name: "sbump2_never",
        owner: "BUNDLE-D",
        dets: &[d("tr_bd", "bd_sbump2_rule")],
    },
    Row {
        name: "sbump2_unbounded",
        owner: "BUNDLE-D",
        dets: &[d("tr_bd", "bd_sbump2_rule")],
    },
    // ---- BUNDLE-H1 ----
    Row {
        name: "hrefi_mod_size",
        owner: "BUNDLE-H1",
        dets: &[d("tr_core", "h1_int_keys_do_not_exit_per_key")],
    },
    Row {
        name: "hrefi_lsize_skip",
        owner: "BUNDLE-H1",
        dets: &[d("tr_record_calls", "h1_int_hash_keys_record_the_integer_hrefv")],
    },
    Row {
        name: "hashint_nil_meta_skip",
        owner: "BUNDLE-H1",
        dets: &[d("tr_core", "h1_hash_int_keys")],
    },
    Row {
        name: "hashint_newindex_skip",
        owner: "BUNDLE-H1",
        dets: &[d("tr_core", "h1_hash_int_keys")],
    },
    Row {
        name: "fcfn_family_excluded",
        owner: "BUNDLE-H1",
        dets: &[d("tr_core", "h1_towers_frame_sides")],
    },
    Row {
        name: "fcfn_cells_off",
        owner: "BUNDLE-H1",
        dets: &[d("tr_core", "h1_towers_frame_sides")],
    },
    Row {
        name: "fck_ignored",
        owner: "BUNDLE-H1",
        dets: &[d("tr_core", "h1_frame_side_thresholds")],
    },
    Row {
        name: "framemin_ignored",
        owner: "BUNDLE-H1",
        dets: &[d("tr_core", "h1_frame_side_thresholds")],
    },
    Row {
        name: "sidefwd_off",
        owner: "BUNDLE-H1",
        dets: &[d("tr_loop", "h1_forwarding_runs_on_a_stitched_trace")],
    },
    Row {
        name: "end_exit_folded",
        owner: "BUNDLE-H1",
        dets: &[d("tr_loop", "h1_forwarding_runs_on_a_stitched_trace")],
    },
    Row {
        name: "g2_ccalls_no_hoist",
        owner: "BUNDLE-G2",
        dets: &[d("tr_record_calls", "g2_ccalls_guard_is_hoisted")],
    },
    Row {
        name: "rec_inner_first_stale",
        owner: "P-M4",
        dets: &[dl("tr_core", "pm4_recursion_entry_counters")],
    },
    Row {
        name: "recnat_default_ignored",
        owner: "P-M4",
        dets: &[dl("tr_policy", "pm4_process_default_recnat")],
    },
    Row {
        name: "tbl_move_meta_skip",
        owner: "N2",
        dets: &[d("tr_tbl_move", "bulk_move_honours_metamethods")],
    },
    Row {
        name: "relaxstr_any_use",
        owner: "N5",
        dets: &[d("tr_core", "string_variant_programs_run_on_traces")],
    },
    Row {
        name: "uvslot_guard_skip",
        owner: "N1",
        dets: &[d("tr_core", "upvalue_slot_alias_programs")],
    },
    Row {
        name: "uvslot_off_by_one",
        owner: "N1",
        dets: &[d("tr_core", "upvalue_slot_alias_programs")],
    },
    Row {
        name: "uvslot_as_cell",
        owner: "N1",
        dets: &[d("tr_core", "upvalue_slot_alias_programs")],
    },
    Row {
        name: "uvslot_store_skip",
        owner: "N1",
        dets: &[d("tr_core", "upvalue_slot_alias_programs")],
    },
    Row {
        name: "hrefv_chain_stops_early",
        owner: "N6",
        dets: &[d("tr_core", "dynamic_key_loops_do_not_exit_per_key")],
    },
    Row {
        name: "hrefv_mask_off_by_one",
        owner: "N6",
        dets: &[d("tr_core", "dynamic_key_loops_do_not_exit_per_key")],
    },
    Row {
        name: "alias_hrefv_as_no",
        owner: "N6",
        dets: &[d("tr_core", "dynamic_key_programs")],
    },
    Row {
        name: "alias_hrefv_key_as_must",
        owner: "N6",
        dets: &[d("tr_core", "dynamic_key_programs")],
    },
    Row {
        name: "hrefv_meta_skip",
        owner: "N6",
        dets: &[d("tr_core", "dynamic_key_programs")],
    },
    // ---- N7: ipairs(t) / pairs(t) calls, TFORPREP, the end of a traversal ----
    Row {
        name: "gfor_pairs_meta_skip",
        owner: "N7",
        dets: &[d("tr_core", "gfor_programs")],
    },
    Row {
        name: "gfor_wrong_iterator",
        owner: "N7",
        dets: &[d("tr_core", "gfor_programs")],
    },
    Row {
        name: "gfor_result_count",
        owner: "N7",
        dets: &[d("tr_core", "gfor_programs")],
    },
    Row {
        name: "gfor_ipairs_start_one",
        owner: "N7",
        dets: &[d("tr_core", "gfor_programs")],
    },
    Row {
        name: "gfor_end_meta_skip",
        owner: "N7",
        dets: &[d("tr_core", "gfor_programs")],
    },
    Row {
        name: "gfor_end_guard_skip",
        owner: "N7",
        dets: &[d("tr_core", "gfor_programs")],
    },
    Row {
        name: "gfor_end_always",
        owner: "N7",
        dets: &[d("tr_core", "gfor_programs")],
    },
    Row {
        name: "tforprep_no_swap",
        owner: "N7",
        dets: &[d("tr_core", "gfor_programs")],
    },
    Row {
        name: "tnext_key_site_type",
        owner: "N7",
        dets: &[d("tr_core", "gfor_nest_stays_in_traces")],
    },
    Row {
        name: "mm_flag_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_ctx_off",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_ncalls_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_follower_free",
        owner: "N3",
        dets: &[ol("tr_core", "mm_count_identity")],
    },
    Row {
        name: "mm_result_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_result_slot_off",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_mt_null_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_tm_typecheck_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_callee_guard_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_num_mt_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_absent_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_ccalls_skip",
        owner: "N3",
        dets: &[d("tr_record", "mm_calls_record_their_guards")],
    },
    Row {
        name: "mm_cond_inverted",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_eq_raw_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_eq_absent_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_index_range_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_index_dummy_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_newindex_absent_skip",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "mm_call_shift_off",
        owner: "N3",
        dets: &[d("tr_core", "mm_programs")],
    },
    Row {
        name: "rawget_str_absent_skip",
        owner: "N",
        dets: &[d("tr_core", "mm_programs"), d("tr_core", "rawfn_programs")],
    },
    Row {
        name: "setmt_protect_skip",
        owner: "N3b",
        dets: &[d("tr_core", "setmt_programs")],
    },
    Row {
        name: "setmt_gc_skip",
        owner: "N3b",
        dets: &[d("tr_core", "setmt_programs")],
    },
    Row {
        name: "setmt_barrier_skip",
        owner: "N3b",
        dets: &[d("tr_core", "setmt_programs")],
    },
    Row {
        name: "setmt_result_mt",
        owner: "N3b",
        dets: &[d("tr_core", "setmt_programs")],
    },
    Row {
        name: "setmt_model_skip",
        owner: "N3b",
        dets: &[d("tr_core", "setmt_programs")],
    },
    Row {
        name: "alloc_shallow_impure",
        owner: "N3b",
        dets: &[d("tr_record", "setmt_in_a_metamethod_records_with_a_shallow_allocation_exit")],
    },
    Row {
        name: "tailfast_args_skip",
        owner: "R1",
        dets: &[d("tr_core", "r1_tail_and_call_loops")],
    },
    Row {
        name: "tailfast_copy_short",
        owner: "R1",
        dets: &[d("tr_core", "r1_tail_and_call_loops")],
    },
    Row {
        name: "tailfast_status_skip",
        owner: "R1",
        dets: &[d("tr_core", "r1_tail_and_call_loops")],
    },
    Row {
        name: "nr_key_skip",
        owner: "A1",
        dets: &[d("tr_core", "a1_inline_newref")],
    },
    Row {
        name: "nr_next_skip",
        owner: "A1",
        dets: &[d("tr_core", "a1_inline_newref")],
    },
    Row {
        name: "nr_flags_skip",
        owner: "A1",
        dets: &[d("tr_core", "a1_inline_newref")],
    },
    Row {
        name: "smi_gcbit_skip",
        owner: "A1",
        dets: &[d("tr_core", "setmt_programs")],
    },
    Row {
        name: "smi_black_skip",
        owner: "A1",
        dets: &[d("tr_core", "setmt_programs")],
    },
    Row {
        name: "smi_old_skip",
        owner: "A1",
        dets: &[d("tr_core", "setmt_programs")],
    },
    Row {
        name: "tlen_inline_prev_skip",
        owner: "A2",
        dets: &[d("tr_core", "tlen_inline_programs")],
    },
    Row {
        name: "tlen_inline_next_skip",
        owner: "A2",
        dets: &[d("tr_core", "tlen_inline_programs")],
    },
    Row {
        name: "tlen_inline_dummy_skip",
        owner: "A2",
        dets: &[d("tr_core", "tlen_inline_programs")],
    },
    Row {
        name: "tlen_inline_mt_skip",
        owner: "A2",
        dets: &[d("tr_core", "tlen_inline_programs")],
    },
    Row {
        name: "tlen_inline_hint_skip",
        owner: "A2",
        dets: &[d("tr_core", "tlen_inline_programs")],
    },
    Row {
        name: "tlen_inline_zero_bound_skip",
        owner: "A2",
        dets: &[d("tr_core", "tlen_inline_programs")],
    },    Row {
        name: "frame_side_never",
        owner: "S1",
        dets: &[d("tr_core", "s1_frame_side_programs")],
    },
    Row {
        name: "frame_cell_no_test",
        owner: "S1",
        dets: &[d("tr_core", "s1_frame_side_programs")],
    },
    Row {
        name: "frame_cell_unwind_skip",
        owner: "S1",
        dets: &[d("tr_core", "s1_frame_side_programs")],
    },
    Row {
        name: "fside_rem_root",
        owner: "S1",
        dets: &[ol("tr_core", "instruction_count_identity")],
    },
    Row {
        name: "fside_room_ignored",
        owner: "S1",
        // (Y4: `r2_side_room` has a frameful side trace (inside the inlined `B:step` frame) whose extent (the 60-register `wide`) is far above its
        // parent's, run right after `collectgarbage()` shrank the stack; without the install-time check there is no start check either and the
        // trace writes past the stack: heap corruption, the child aborts. `s1_frame_side_programs` has no such trace.)
        dets: &[d("tr_core", "r2_side_room_programs")],
    },
    Row {
        name: "fside_call_pc_off",
        owner: "S1",
        dets: &[d("tr_core", "s1_frame_side_programs")],
    },
    Row {
        name: "fside_nresults_off",
        owner: "S1",
        dets: &[d("tr_core", "s1_frame_side_programs")],
    },
    Row {
        name: "fside_tail_dropped",
        owner: "S1",
        dets: &[d("tr_core", "s1_frame_side_programs")],
    },
    Row {
        name: "fside_mm_plain",
        owner: "S1",
        dets: &[d("tr_core", "s1_frame_side_programs")],
    },
    Row {
        name: "fside_frames_dropped",
        owner: "S1",
        dets: &[d("tr_core", "s1_frame_side_programs")],
    },
    Row {
        name: "fmat_savedpc_caller_off",
        owner: "H2",
        dets: &[d("tr_exit", "fmat_ops_equal_restore")],
    },
    Row {
        name: "fmat_top_off_by_one",
        owner: "H2",
        dets: &[d("tr_exit", "fmat_ops_equal_restore")],
    },
    Row {
        name: "fmat_nresults_off",
        owner: "H2",
        dets: &[d("tr_exit", "fmat_ops_equal_restore")],
    },
    Row {
        name: "fmat_tail_flag_skip",
        owner: "H2",
        dets: &[d("tr_exit", "fmat_ops_equal_restore")],
    },
    Row {
        name: "fmat_trap_skip",
        owner: "H2",
        dets: &[d("tr_exit", "fmat_ops_equal_restore")],
    },
    Row {
        name: "fmat_th_top_skip",
        owner: "H2",
        dets: &[d("tr_exit", "fmat_ops_equal_restore")],
    },
    Row {
        name: "fmat_mask_skip",
        owner: "H2",
        dets: &[d("tr_core", "h1_towers_frame_sides")],
    },
    Row {
        name: "black_exit_report",
        owner: "H3",
        dets: &[d("tr_core", "h3_black_exits_not_reported")],
    },
    Row {
        name: "sink_demote_ignored",
        owner: "J1",
        dets: &[d("tr_core", "sinkroot_programs")],
    },
    Row {
        name: "hrefk_key64_hi",
        owner: "H3",
        dets: &[d("tr_asm", "random_traces_cover_the_hash_ops"), dl("tr_core", "mutant_probe_live_programs")],
    },
    Row {
        name: "tailfast_close_skip",
        owner: "M1",
        dets: &[d("tr_core", "m1_interp_shapes")],
    },
    Row {
        name: "side_stitch_never",
        owner: "M1",
        dets: &[d("tr_core", "m1_interp_shapes")],
    },
    Row {
        name: "aload_nil_skip",
        owner: "M1",
        dets: &[d("tr_core", "m1_interp_shapes")],
    },
    Row {
        name: "depth_stitch_never",
        owner: "M1",
        dets: &[d("tr_core", "m1_interp_shapes")],
    },
    Row {
        name: "f1_pow_any",
        owner: "F1",
        dets: &[d("tr_core", "f1_programs")],
    },
    Row {
        name: "f1_minmax_order",
        owner: "F1",
        dets: &[d("tr_core", "f1_programs")],
    },
    Row {
        name: "f1_strlen_off",
        owner: "F1",
        dets: &[d("tr_core", "f1_programs")],
    },
    Row {
        name: "f1_sin_cos_swap",
        owner: "F1",
        dets: &[d("tr_core", "f1_programs")],
    },
    Row {
        name: "f1_ceil_as_floor",
        owner: "F1",
        dets: &[d("tr_core", "f1_programs")],
    },
    Row {
        name: "rec_ret_wanted_unchecked",
        owner: "R2",
        dets: &[d("tr_core", "r2_recursion_programs")],
    },
    Row {
        name: "rec_ret_top_off",
        owner: "R2",
        dets: &[d("tr_core", "r2_recursion_programs")],
    },
    Row {
        name: "rec_ret_cframe_skip",
        owner: "R2",
        dets: &[d("tr_core", "r2_recursion_programs")],
    },
    Row {
        name: "rec_ret_regs_unsaved",
        owner: "BUNDLE-E",
        dets: &[d("tr_core", "be_native_return_guards_keep_registers")],
    },
    Row {
        name: "rec_loop_end_guard_skip",
        owner: "R2",
        dets: &[d("tr_core", "r2_recursion_programs")],
    },
    Row {
        name: "side_room_check_skip",
        owner: "R2",
        dets: &[d("tr_core", "r2_side_room_programs")],
    },
    Row {
        name: "side_room_meta_full",
        owner: "R2",
        dets: &[d("tr_core", "r2_side_room_programs")],
    },
    Row {
        name: "nilstore_newindex_skip",
        owner: "R2",
        dets: &[d("tr_core", "r2_nilstore_programs")],
    },
    Row {
        name: "nilstore_meta_name_ok",
        owner: "R2",
        dets: &[d("tr_core", "r2_nilstore_programs")],
    },
    Row {
        name: "r2_default_ignored",
        owner: "R2",
        dets: &[d("tr_policy", "r2_process_defaults")],
    },
    Row {
        name: "b2arr_no_not",
        owner: "B2",
        dets: &[d("tr_core", "b2_programs")],
    },
    Row {
        name: "b2arr_off8",
        owner: "B2",
        dets: &[d("tr_core", "b2_programs")],
    },
    Row {
        name: "b2mt_store",
        owner: "B2",
        dets: &[d("tr_core", "b2_programs")],
    },
    Row {
        name: "b2tag_alias_keep",
        owner: "B2",
        dets: &[d("tr_core", "b2_programs")],
    },
    Row {
        name: "b2hk_lea_off",
        owner: "B2",
        dets: &[d("tr_core", "b2_programs")],
    },
    Row {
        name: "strcmp_lt_as_le",
        owner: "F2",
        dets: &[d("tr_core", "f2_programs")],
    },
    Row {
        name: "strcmp_guard_skip",
        owner: "F2",
        dets: &[d("tr_core", "f2_programs")],
    },
    Row {
        name: "getmt_null_skip",
        owner: "F2",
        dets: &[d("tr_core", "f2_programs")],
    },
    Row {
        name: "getmt_protected_skip",
        owner: "F2",
        dets: &[d("tr_core", "f2_programs")],
    },
    Row {
        name: "assert_extra_dropped",
        owner: "F2",
        dets: &[d("tr_core", "f2_programs")],
    },
    Row {
        name: "rawset_tbar_skip",
        owner: "F2",
        dets: &[d("tr_record", "f2_ops_record_their_guards")],
    },
    Row {
        name: "rawset_result_nil",
        owner: "F2",
        dets: &[d("tr_core", "f2_programs"), d("tr_core", "rawfn_programs")],
    },
    Row {
        name: "byte_inline_off_by_one",
        owner: "F2",
        dets: &[d("tr_core", "f2_programs")],
    },
    Row {
        name: "byte_inline_len_skip",
        owner: "F2",
        dets: &[d("tr_core", "f2_programs")],
    },
    Row {
        name: "byte_range_guard_skip",
        owner: "F2",
        dets: &[d("tr_core", "f2_programs")],
    },
    Row {
        name: "byte_range_count_off",
        owner: "F2",
        dets: &[d("tr_core", "f2_programs")],
    },
    Row {
        name: "random_range_off",
        owner: "F2",
        dets: &[d("tr_core", "f2_programs")],
    },
    Row {
        name: "c2cmp_swap",
        owner: "C2",
        dets: &[d("jit_table", "c2_tier1_compares")],
    },
    Row {
        name: "c2cmp_exit_skip",
        owner: "C2",
        dets: &[d("jit_table", "c2_tier1_compares")],
    },
    Row {
        name: "mmret_nc_skip",
        owner: "BUNDLE-B",
        dets: &[d("tr_core", "bb_cmpmm_programs")],
    },
    Row {
        name: "mmret_flag_false",
        owner: "BUNDLE-B",
        dets: &[d("tr_core", "bb_cmpmm_programs")],
    },
    Row {
        name: "mmcmp_savedpc_skip",
        owner: "BUNDLE-B",
        dets: &[d("tr_core", "bb_cmpmm_programs")],
    },
    Row {
        name: "c2setupval_barrier_skip",
        owner: "C2",
        dets: &[Det {
            bin: "jit_table",
            test: "c2_tier1_setupval",
            need: Need::Plain,
            env: &[("RUSTYNAIL_GC_STRESS", "2"), ("RUSTYNAIL_GC_VERIFY", "1")],
        }],
    },
    Row {
        name: "t3idx_nil_hit",
        owner: "T3",
        dets: &[d("tr_core", "t3_programs")],
    },
    Row {
        name: "t3idx_use_mt",
        owner: "T3",
        dets: &[d("tr_core", "t3_programs")],
    },
    Row {
        name: "habs_found_ignored",
        owner: "L",
        dets: &[d("tr_core", "dm_programs")],
    },
    Row {
        name: "strself_tag_skip",
        owner: "K",
        dets: &[d("tr_core", "k_strself_programs")],
    },
    Row {
        name: "strself_mt_skip",
        owner: "K",
        dets: &[d("tr_core", "k_strself_programs")],
    },
    Row {
        name: "strself_nil_hit",
        owner: "K",
        dets: &[d("tr_core", "k_strself_programs")],
    },
    Row {
        name: "t1i_type_ident_skip",
        owner: "T1I",
        dets: &[d("tr_core", "t1i_inline_programs")],
    },
    Row {
        name: "t1i_lcf_ident_skip",
        owner: "T1I",
        dets: &[d("tr_core", "t1i_inline_programs")],
    },
    Row {
        name: "t1i_floor_inexact",
        owner: "T1I",
        dets: &[d("tr_core", "t1i_inline_programs")],
    },
    Row {
        name: "t1i_minmax_le",
        owner: "T1I",
        dets: &[d("tr_core", "t1i_inline_programs")],
    },
    Row {
        name: "t1i_byte_lo_skip",
        owner: "T1I",
        dets: &[d("tr_core", "t1i_inline_programs")],
    },
    Row {
        name: "t1i_abs_float_sign",
        owner: "T1I",
        dets: &[d("tr_core", "t1i_inline_programs")],
    },
    Row {
        name: "t1i_select_count",
        owner: "T1I",
        dets: &[d("tr_core", "t1i_inline_programs")],
    },
    Row {
        name: "t1i_rawlen_next_skip",
        owner: "T1I",
        dets: &[d("tr_core", "t1i_inline_programs")],
    },
    Row {
        name: "t1i_sqrt_int_bits",
        owner: "T1I",
        dets: &[d("tr_core", "t1i_inline_programs")],
    },
    Row {
        name: "t1i_type_index",
        owner: "T1I",
        dets: &[d("tr_core", "t1i_inline_programs")],
    },
    Row {
        name: "t3_default_ignored",
        owner: "T3",
        dets: &[d("tr_policy", "t3_process_default")],
    },
    Row {
        name: "ineff_cover_ignored",
        owner: "P2",
        dets: &[d("tr_core", "p2_scheduler_tree_is_kept")],
    },
    Row {
        name: "typesides_ignored",
        owner: "P2",
        dets: &[d("tr_core", "p2_type_guard_exits_get_side_traces")],
    },
    Row {
        name: "ineff_hop_never",
        owner: "P2",
        dets: &[d("tr_determinism", "default_policy_reads_no_clock_and_repeats")],
    },
    Row {
        name: "hop_top_ignored",
        owner: "P",
        dets: &[d("tr_determinism", "default_policy_reads_no_clock_and_repeats")],
    },
    Row {
        name: "ineff_persist_never",
        owner: "P2",
        dets: &[d("tr_core", "p2_persistent_change_is_still_flushed")],
    },
    Row {
        name: "p2_default_ignored",
        owner: "P2",
        dets: &[d("tr_policy", "p2_process_default")],
    },
    Row {
        name: "tailhook_ignored",
        owner: "B1",
        dets: &[d("tr_core", "b1_tail_only_callee_is_compiled")],
    },
    Row {
        name: "hdrlib_never",
        owner: "B1",
        dets: &[d("tr_core", "b1_hdr_peels_length_loops")],
    },
    Row {
        name: "hdrlib_table_tlen",
        owner: "B1",
        dets: &[d("tr_loop", "hdrlib_classifies_lib_ops")],
    },
    Row {
        name: "hdrlib_setmt",
        owner: "B1",
        dets: &[d("tr_loop", "hdrlib_classifies_lib_ops")],
    },
    Row {
        name: "hdrlib_newref",
        owner: "B1",
        dets: &[d("tr_loop", "hdrlib_classifies_lib_ops")],
    },
    Row {
        name: "hdrlib_third_stale",
        owner: "B1",
        dets: &[d("tr_loop", "hdrlib_classifies_lib_ops")],
    },
    Row {
        name: "hdrlib_sub_always",
        owner: "HDRSUB",
        dets: &[d("tr_loop", "hdrlib_classifies_lib_ops"), d("tr_core", "hs_sub_peels_only_from_level_2")],
    },
    Row {
        name: "hdrlib_sub_phi_stale",
        owner: "HDRSUB",
        dets: &[d("tr_core", "hs_sub_ring_matrix")],
    },
    Row {
        name: "libsub_lean_args_swapped",
        owner: "HDRSUB",
        dets: &[d("tr_core", "hs_sub_ring_matrix")],
    },
    Row {
        name: "tonum_type_guard_skip",
        owner: "BUNDLE-A",
        dets: &[d("tr_core", "ba_num_matrix")],
    },
    Row {
        name: "tonum_mode_ignored",
        owner: "BUNDLE-A",
        dets: &[d("tr_core", "ba_num_does_not_exit_on_numerals")],
    },
    Row {
        name: "next_ctl_nil",
        owner: "BUNDLE-A",
        dets: &[d("tr_core", "ba_next_matrix")],
    },
    Row {
        name: "find_specials_skip",
        owner: "BUNDLE-A",
        dets: &[d("tr_class_a_pb", "lib_find_follows_the_library"), d("tr_core", "ba_find_matrix")],
    },
    Row {
        name: "find_end_off",
        owner: "BUNDLE-A",
        dets: &[d("tr_core", "ba_find_matrix")],
    },
    Row {
        name: "find_init_neg_skip",
        owner: "BUNDLE-A",
        dets: &[d("tr_class_a_pb", "lib_find_follows_the_library"), d("tr_core", "ba_find_matrix")],
    },
    Row {
        name: "find_outcome_guard_skip",
        owner: "BUNDLE-A",
        dets: &[d("tr_core", "ba_find_matrix")],
    },
    Row {
        name: "tfmt_kind_skip",
        owner: "BUNDLE-C",
        dets: &[d("tr_class_a_pb", "lib_fmt_follows_the_library"), d("tr_core", "tc_fmt_matrix")],
    },
    Row {
        name: "tfmt_fmt_guard_skip",
        owner: "BUNDLE-C",
        dets: &[d("tr_core", "tc_fmt_matrix")],
    },
    Row {
        name: "tfmt_args_swap",
        owner: "BUNDLE-C",
        dets: &[d("tr_class_a_pb", "lib_fmt_follows_the_library"), d("tr_core", "tc_fmt_matrix")],
    },
    Row {
        name: "tfmt_stash_stale",
        owner: "BUNDLE-C",
        dets: &[d("tr_class_a_pb", "lib_fmt_follows_the_library")],
    },
    Row {
        name: "next_callee_guard_skip",
        owner: "BUNDLE-A",
        dets: &[d("tr_record", "ba_ops_record_their_guards"), d("tr_core", "ba_next_matrix")],
    },
    Row {
        name: "wgate_full_rearm",
        owner: "B1",
        dets: &[d("tr_core", "b1_wgate_keeps_the_formation_order")],
    },
    Row {
        name: "b1_default_ignored",
        owner: "B1",
        dets: &[d("tr_policy", "b1_process_defaults")],
    },
    Row {
        name: "nopay_all_types",
        owner: "B4",
        dets: &[d("tr_core", "b4_programs")],
    },
    Row {
        name: "divmagic_no_sign",
        owner: "B4",
        dets: &[d("tr_core", "b4_div_programs")],
    },
    Row {
        name: "divmagic_shift_off",
        owner: "B4",
        dets: &[d("tr_core", "b4_div_programs")],
    },
    Row {
        name: "hkfuse_tag_skip",
        owner: "B4",
        dets: &[d("tr_core", "b4_hk_programs")],
    },
    Row {
        name: "hkfuse_split_never",
        owner: "J1",
        dets: &[d("tr_asm", "hand_fused_hrefk_hload_keeps_both_exits"), d("tr_core", "j1_hkfuse_fires")],
    },
    Row {
        name: "hkfuse_cold_swap",
        owner: "J1",
        dets: &[d("tr_asm", "hand_fused_hrefk_hload_keeps_both_exits")],
    },
    Row {
        name: "hkfuse_state_skip",
        owner: "J1",
        dets: &[d("tr_asm", "random_traces_cover_the_hash_ops"), d("tr_asm", "native_matches_the_reference_peeled_and_rolled")],
    },
    Row {
        name: "hmiss_key64_hi",
        owner: "J1",
        dets: &[d("tr_asm", "hand_fused_hrefk_hload_keeps_both_exits")],
    },
    Row {
        name: "eqref_proto64_hi",
        owner: "J1",
        dets: &[d("tr_asm", "hand_eqref_proto_and_builtin_forms")],
    },
    Row {
        name: "sinkstub_never",
        owner: "J1",
        dets: &[d("tr_asm", "hand_sunk_exit_stub_calls_the_materialising_helper"), d("tr_core", "j1_sinkstub_programs")],
    },
    Row {
        name: "sinkstub_save_skip",
        owner: "J1",
        dets: &[d("tr_asm", "hand_sunk_exit_stub_calls_the_materialising_helper"), d("tr_core", "j1_sinkstub_programs")],
    },
    Row {
        name: "sinkstub_err_ignored",
        owner: "J1",
        dets: &[d("tr_asm", "hand_sunk_exit_stub_calls_the_materialising_helper"), d("tr_core", "j1_sinkstub_alloc_failure_raises_memory_error")],
    },
    Row {
        name: "sinkstub_frames_never",
        owner: "J1",
        dets: &[d("tr_core", "j1_sinkstub_programs")],
    },
    Row {
        name: "b5_default_ignored",
        owner: "B5",
        dets: &[d("tr_policy", "b5_process_defaults")],
    },
    Row {
        name: "sinkmt_never",
        owner: "B5",
        dets: &[d("tr_core", "b5_sink_counts")],
    },
    Row {
        name: "sinkmt_gc_guard_skip",
        owner: "B5",
        dets: &[d("tr_core", "b5_sink_programs")],
    },
    Row {
        name: "sinkmt_no_mt_materialise",
        owner: "B5",
        dets: &[d("tr_core", "b5_sink_programs")],
    },
    Row {
        name: "sinkside_dead_all",
        owner: "B5",
        dets: &[d("tr_core", "b5_sink_programs")],
    },
    Row {
        name: "sunkframe_cell_nomat",
        owner: "SINK-F",
        dets: &[d("tr_core", "sinkf_side_programs")],
    },
    Row {
        name: "sinkroot_never",
        owner: "SINK-R",
        dets: &[d("tr_core", "sinkroot_programs")],
    },
    Row {
        name: "sinkroot_default_ignored",
        owner: "SINK-R",
        dets: &[d("tr_policy", "sinkroot_process_defaults")],
    },
    Row {
        name: "bb_default_ignored",
        owner: "BB",
        dets: &[d("tr_policy", "bb_process_defaults")],
    },
    Row {
        name: "nilset_flags_skip",
        owner: "BB",
        dets: &[d("tr_core", "bb_tset_programs")],
    },
    Row {
        name: "nilset_mt_skip",
        owner: "BB",
        dets: &[d("tr_core", "bb_tset_programs")],
    },
    Row {
        name: "nilset_barrier_skip",
        owner: "BB",
        dets: &[d("tr_core", "bb_tset_programs")],
    },
    Row {
        name: "nilset_arr_mt_skip",
        owner: "BB",
        dets: &[d("tr_core", "bb_tset_programs")],
    },
    Row {
        name: "tset_str_tag_skip",
        owner: "BB",
        dets: &[d("tr_core", "bb_tset_programs")],
    },
    Row {
        name: "tset_int_key_unchecked",
        owner: "BB",
        dets: &[d("tr_core", "bb_tset_programs")],
    },
    Row {
        name: "tget_nil_hit",
        owner: "G1",
        dets: &[d("tr_core", "bb_tset_programs")],
    },
    Row {
        name: "tget_tag_skip",
        owner: "G1",
        dets: &[d("tr_core", "bb_tset_programs")],
    },
    Row {
        name: "g1fmt_fmt_guard_skip",
        owner: "G1",
        dets: &[d("tr_core", "g1_programs")],
    },
    Row {
        name: "g1fmt_result_fmt",
        owner: "G1",
        dets: &[d("tr_core", "g1_programs")],
    },
    Row {
        name: "streq_inline_len_diff_eq",
        owner: "G1",
        dets: &[d("tr_core", "g1_programs")],
    },
    Row {
        name: "streq_inline_tail_ne_eq",
        owner: "G1",
        dets: &[d("tr_core", "g1_programs")],
    },
    Row {
        name: "streq_inline_tail_eq",
        owner: "G1",
        dets: &[d("tr_core", "g1_programs")],
    },
    Row {
        name: "deep_never",
        owner: "J2",
        dets: &[d("tr_core", "j2_deep_programs")],
    },
    Row {
        name: "deep_every_time",
        owner: "J2",
        dets: &[d("tr_policy", "j2_deep_site_state"), dl("tr_core", "j2_deep_programs")],
    },
    Row {
        name: "deep_limits_ignored",
        owner: "J2",
        dets: &[d("tr_core", "j2_deep_programs")],
    },
    Row {
        name: "deep_ins_unbounded",
        owner: "J2",
        dets: &[d("tr_core", "j2_deep_programs")],
    },
    Row {
        name: "deep_default_ignored",
        owner: "J2",
        dets: &[d("tr_policy", "j2_process_defaults")],
    },
    Row {
        name: "deep_hop_rule_kept",
        owner: "J2",
        dets: &[d("tr_policy", "j2_deep_site_state")],
    },
    // BUNDLE-M
    Row {
        name: "dynmask_mask_off_by_one",
        owner: "DYNMASK",
        dets: &[d("tr_core", "dm_programs"), d("tr_core", "dm_growth_exits_are_bounded")],
    },
    Row {
        name: "dynmask_cse_merge",
        owner: "DYNMASK",
        dets: &[d("tr_core", "dm_programs")],
    },
    Row {
        name: "dynmask_nil_store",
        owner: "DYNMASK",
        dets: &[d("tr_core", "dm_programs")],
    },
    Row {
        name: "dynmask_nil_meta_skip",
        owner: "DYNMASK",
        dets: &[d("tr_core", "dm_programs")],
    },
    Row {
        name: "nr_img_record_skip",
        owner: "M",
        dets: &[d("tr_record", "m_lockstep_off_records_the_same_ir")],
    },
    Row {
        name: "light_charge_skip",
        owner: "M",
        dets: &[d("tr_record", "m_charge_with_lockstep_off")],
    },
    Row {
        name: "light_tnew_unregistered",
        owner: "M",
        dets: &[d("tr_record", "m_lockstep_off_records_the_same_ir")],
    },
    Row {
        name: "polyside_default_ignored",
        owner: "O",
        dets: &[dl("tr_policy", "o_polyside_process_default")],
    },
    Row {
        name: "polywait_ignored",
        owner: "O",
        dets: &[d("tr_core", "po_poly_programs")],
    },
    Row {
        name: "polyframes_ignored",
        owner: "O",
        dets: &[d("tr_core", "po_poly_programs")],
    },
    Row {
        name: "loopany_default_ignored",
        owner: "S",
        dets: &[dl("tr_policy", "s_loopany_process_default")],
    },
    Row {
        name: "loopany_guard_never",
        owner: "S",
        dets: &[d("tr_core", "s_loopany_guard_programs")],
    },
    Row {
        name: "laprune_flushes_root",
        owner: "P",
        dets: &[d("tr_core", "p_laprune_programs"), dl("tr_policy", "p_laprune_policy")],
    },
    Row {
        name: "laprune_never_flushes_sides",
        owner: "P",
        dets: &[d("tr_core", "p_laprune_programs")],
    },
    Row {
        name: "cispare_never",
        owner: "C36",
        dets: &[d("tr_core", "c36_cispare_programs")],
    },
    Row {
        name: "cispare_flush_anyway",
        owner: "C36",
        dets: &[d("tr_core", "c36_cispare_programs")],
    },
    Row {
        name: "dom_never",
        owner: "C36",
        dets: &[d("tr_core", "c36_dom_programs"), dl("tr_policy", "c36_policy")],
    },
    Row {
        name: "dom_repeats",
        owner: "C36",
        dets: &[dl("tr_policy", "c36_policy")],
    },
    Row {
        name: "second_retry_never",
        owner: "C36",
        dets: &[dl("tr_policy", "c36_policy")],
    },
    Row {
        name: "second_retry_repeats",
        owner: "C36",
        dets: &[dl("tr_policy", "c36_policy")],
    },
    Row {
        name: "second_identical_never",
        owner: "C36",
        dets: &[dl("tr_policy", "c36_policy")],
    },
    Row {
        name: "second_identical_repeats",
        owner: "C36",
        dets: &[dl("tr_policy", "c36_policy")],
    },
    Row {
        name: "chain_never",
        owner: "P3",
        dets: &[dl("tr_core", "p3_chain_programs"), dl("tr_policy", "p3_policy")],
    },
    Row {
        name: "chain_unbounded",
        owner: "P3",
        dets: &[dl("tr_policy", "p3_policy")],
    },
    Row {
        name: "chain_pass_ignored",
        owner: "P3",
        dets: &[dl("tr_policy", "p3_policy")],
    },
    Row {
        name: "chain_gain_ignored",
        owner: "P3",
        dets: &[dl("tr_core", "p3_chain_programs"), dl("tr_policy", "p3_policy")],
    },
    Row {
        name: "chain_stub_never",
        owner: "P3",
        dets: &[dl("tr_core", "p3_chain_programs")],
    },
    Row {
        name: "chain_stub_no_test",
        owner: "P3",
        dets: &[dl("tr_core", "p3_chain_programs")],
    },
    Row {
        name: "chain_no_rebase",
        owner: "P3",
        dets: &[dl("tr_core", "p3_chain_programs")],
    },
    Row {
        name: "chaindepth_ignored",
        owner: "BUNDLE-G",
        dets: &[dl("tr_core", "bg_chaindepth_programs"), dl("tr_policy", "bg_policy")],
    },
    Row {
        name: "chaindepth_cover_ignored",
        owner: "BUNDLE-G",
        dets: &[dl("tr_core", "bg_chaindepth_programs"), dl("tr_policy", "bg_policy")],
    },
    Row {
        name: "ncallee_never",
        owner: "P3",
        dets: &[dl("tr_core", "p3_fdisp_programs"), dl("tr_policy", "p3_policy")],
    },
    Row {
        name: "ncallee_without_entry_guard",
        owner: "P3",
        dets: &[dl("tr_policy", "p3_policy")],
    },
    Row {
        name: "budget_refund_skip",
        owner: "P2",
        dets: &[d("tr_asm", "hand_budget_sub_refunds_on_exit")],
    },
    Row {
        name: "budget_refund_short",
        owner: "P2",
        dets: &[d("tr_asm", "hand_budget_sub_refunds_on_exit")],
    },
    Row {
        name: "poll_r15_disp_off",
        owner: "P2",
        dets: &[d("tr_asm", "hand_poll_from_r15")],
    },
    Row {
        name: "fpool_bits_off",
        owner: "P2",
        dets: &[dl("tr_core", "p2_loopdiet_programs")],
    },
    Row {
        name: "mul2_any_const",
        owner: "P2",
        dets: &[dl("tr_core", "p2_loopdiet_programs")],
    },
    Row {
        name: "loopdiet_default_ignored",
        owner: "P2",
        dets: &[d("tr_policy", "p2_loopdiet_process_defaults")],
    },
    Row {
        name: "xregs_no_reloadk",
        owner: "P2",
        dets: &[dl("tr_core", "p2_xregs_programs")],
    },
    Row {
        name: "xregs_rec_r14",
        owner: "P2",
        dets: &[dl("tr_core", "p2_xregs_programs")],
    },
    Row {
        name: "rearm_never",
        owner: "R1",
        dets: &[dl("tr_core", "r1_rearm_programs")],
    },
    Row {
        name: "rearm_warm_credit",
        owner: "R1",
        dets: &[dl("tr_core", "r1_rearm_programs")],
    },
    Row {
        name: "rearm_any_head",
        owner: "R1",
        dets: &[dl("tr_core", "r1_rearm_programs")],
    },
    Row {
        name: "rearm_tick_reset",
        owner: "R1",
        dets: &[dl("tr_core", "r1_rearm_programs")],
    },
    Row {
        name: "rearm_default_ignored",
        owner: "R1",
        dets: &[d("tr_policy", "r1_rearm_process_default")],
    },
    Row {
        name: "uvsep_no_guard",
        owner: "P2",
        dets: &[d("tr_loop", "uvsep_forwards_over_distinct_upvalue_stores"), dl("tr_core", "p2_uvsep_programs")],
    },
    Row {
        name: "uvsep_first_guard_only",
        owner: "P2",
        dets: &[d("tr_loop", "uvsep_forwards_over_distinct_upvalue_stores")],
    },
    Row {
        name: "cleanup_third_dead",
        owner: "P2",
        dets: &[d("tr_loop", "cleanup_renumbers_third_operands")],
    },
    Row {
        name: "cleanup_third_stale",
        owner: "P2",
        dets: &[d("tr_loop", "cleanup_renumbers_third_operands")],
    },
    Row {
        name: "inner_retry_never",
        owner: "C39",
        dets: &[d("tr_core", "c39_inner_programs"), dl("tr_policy", "c39_policy")],
    },
    Row {
        name: "inner_retry_unbounded",
        owner: "C39",
        dets: &[dl("tr_policy", "c39_policy")],
    },
    Row {
        name: "side_frame_link_never",
        owner: "C38",
        dets: &[d("tr_core", "c38_side_programs")],
    },
    Row {
        name: "sbump_never",
        owner: "BUNDLE-C",
        dets: &[d("tr_core", "bc_sbump_programs")],
    },
    Row {
        name: "fnpromo_never",
        owner: "BUNDLE-C",
        dets: &[dl("tr_policy", "bc_policy")],
    },
    Row {
        name: "fnpromo_repeats",
        owner: "BUNDLE-C",
        dets: &[dl("tr_policy", "bc_policy")],
    },
    Row {
        name: "gm_kind_skip",
        owner: "BUNDLE-GM",
        dets: &[dl("tr_core", "gm_programs")],
    },
    Row {
        name: "gm_no_commit",
        owner: "BUNDLE-GM",
        dets: &[dl("tr_core", "gm_programs")],
    },
    Row {
        name: "gm_end_guard_skip",
        owner: "BUNDLE-GM",
        dets: &[dl("tr_core", "gm_programs")],
    },
    Row {
        name: "gm_storm_guard",
        owner: "BUNDLE-GM",
        dets: &[dl("tr_core", "gm_programs")],
    },
    Row {
        name: "rand_storm_guard",
        owner: "C43",
        dets: &[dl("tr_core", "random_storm_programs")],
    },
    Row {
        name: "charn_range_skip",
        owner: "BUNDLE-H",
        dets: &[d("tr_class_a_pb", "lib_char_n_makes_the_bytes_in_order"), d("tr_core", "bh_lib_matrix")],
    },
    Row {
        name: "charn_order",
        owner: "BUNDLE-H",
        dets: &[d("tr_class_a_pb", "lib_char_n_makes_the_bytes_in_order"), d("tr_core", "bh_lib_matrix")],
    },
    Row {
        name: "randf_plus_zero",
        owner: "BUNDLE-H",
        dets: &[d("tr_class_a_pb", "lib_randf_follows_math_random")],
    },
    Row {
        name: "randf_scale_off",
        owner: "BUNDLE-H",
        dets: &[d("tr_core", "bh_lib_matrix")],
    },
    Row {
        name: "lower_is_upper",
        owner: "BUNDLE-H",
        dets: &[d("tr_class_a_pb", "lib_case_and_reverse_follow_the_library"), d("tr_core", "bh_lib_matrix")],
    },
    Row {
        name: "rep_cap_skip",
        owner: "BUNDLE-H",
        dets: &[d("tr_class_a_pb", "lib_rep_follows_the_library")],
    },
    Row {
        name: "rep_sep_trailing",
        owner: "BUNDLE-H",
        dets: &[d("tr_class_a_pb", "lib_rep_follows_the_library"), d("tr_core", "bh_lib_matrix")],
    },
    Row {
        name: "libk_lean_cmp_swapped",
        owner: "BUNDLE-H",
        dets: &[d("tr_core", "bh_lib_matrix")],
    },
    Row {
        name: "libk_lean_char_args",
        owner: "BUNDLE-H",
        dets: &[d("tr_core", "bh_lib_matrix")],
    },
    Row {
        name: "libk_type_fn_skip",
        owner: "BUNDLE-H",
        dets: &[d("tr_core", "bh_lib_matrix")],
    },
    Row {
        name: "libk_type_index_off",
        owner: "BUNDLE-H",
        dets: &[d("tr_core", "bh_lib_matrix")],
    },
    Row {
        name: "libcall_default_ignored",
        owner: "BUNDLE-H",
        dets: &[d("tr_policy", "bh_libcall_process_default")],
    },
    Row {
        name: "repcls_guard_skip",
        owner: "BUNDLE-R",
        dets: &[d("tr_core", "br_rep_class_guard_gets_a_side_trace")],
    },
    Row {
        name: "tunpack_meta_skip",
        owner: "BUNDLE-R",
        dets: &[d("tr_core", "br_lib_matrix")],
    },
    Row {
        name: "tunpack_len_guard_skip",
        owner: "BUNDLE-R",
        dets: &[d("tr_core", "br_lib_matrix")],
    },
    Row {
        name: "tunpack_off_by_one",
        owner: "BUNDLE-R",
        dets: &[d("tr_core", "br_lib_matrix")],
    },
    Row {
        name: "tonum_fail_guard_skip",
        owner: "BUNDLE-R",
        dets: &[d("tr_core", "br_lib_matrix")],
    },
    Row {
        name: "libk_lean_find_swapped",
        owner: "BUNDLE-R",
        dets: &[d("tr_core", "ba_find_matrix")],
    },
    Row {
        name: "tmove_args_guard_skip",
        owner: "BUNDLE-R",
        dets: &[d("tr_core", "br_lib_matrix")],
    },
    Row {
        name: "tmove_same_guard_skip",
        owner: "BUNDLE-R",
        dets: &[d("tr_core", "br_lib_matrix")],
    },
    Row {
        name: "tmove_order_flip",
        owner: "BUNDLE-R",
        dets: &[d("tr_core", "br_lib_matrix")],
    },
    Row {
        name: "tmove_no_preread",
        owner: "BUNDLE-VA",
        dets: &[d("tr_core", "va_tmove_overlap_programs")],
    },
    Row {
        name: "seg_root_code_stale",
        owner: "BUNDLE-N",
        dets: &[d("tr_exit", "n2a_segment_snapshots_use_the_segments_prototype_and_code")],
    },
    Row {
        name: "seg_land_proto_stale",
        owner: "BUNDLE-N",
        dets: &[d("tr_exit", "n2a_segment_snapshots_use_the_segments_prototype_and_code")],
    },
    Row {
        name: "retf_callstatus_unchecked",
        owner: "BUNDLE-N",
        dets: &[d("tr_asm", "hand_retf_guards_exit_before_the_pop")],
    },
    Row {
        name: "retf_cframe_unchecked",
        owner: "BUNDLE-N",
        dets: &[d("tr_asm", "hand_retf_guards_exit_before_the_pop")],
    },
    Row {
        name: "retf_site_unchecked",
        owner: "BUNDLE-N",
        dets: &[d("tr_asm", "hand_retf_guards_exit_before_the_pop")],
    },
    Row {
        name: "retf_k_unchecked",
        owner: "BUNDLE-N",
        dets: &[d("tr_asm", "hand_retf_k_checks_tbc_and_open_upvalues")],
    },
    Row {
        name: "retf_shift_off",
        owner: "BUNDLE-N",
        dets: &[d("tr_asm", "hand_retf_pops_natively_and_exits_in_the_caller")],
    },
    Row {
        name: "retf_no_ci_store",
        owner: "BUNDLE-N",
        dets: &[d("tr_asm", "hand_retf_pops_natively_and_exits_in_the_caller")],
    },
    Row {
        name: "retf_no_reloadk",
        owner: "BUNDLE-N",
        dets: &[d("tr_asm", "hand_retf_pops_natively_and_exits_in_the_caller")],
    },
    Row {
        name: "retf_uvrel_cse",
        owner: "BUNDLE-N",
        dets: &[d("tr_ir", "retf_cuts_the_uvrel_cse_chain")],
    },
    Row {
        name: "retf_model_uvrel_unshifted",
        owner: "BUNDLE-N",
        dets: &[d("tr_ir", "retf_in_the_model_rebases_slots_and_uvrel")],
    },
    Row {
        name: "retf_slot_remap_off",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_retf_traces_are_installed")],
    },
    Row {
        name: "retf_results_dest_off",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_retf_traces_are_installed")],
    },
    Row {
        name: "retf_multret_top_unmapped",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_retf_traces_are_installed")],
    },
    Row {
        name: "retf_root_func_stale",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_retf_traces_are_installed")],
    },
    Row {
        name: "retf_ci_stays",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_retf_traces_are_installed")],
    },
    Row {
        name: "retf_known_unshifted",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_n3_programs")],
    },
    Row {
        name: "retf_func_ref_stale",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_n3_programs")],
    },
    Row {
        name: "retf_extent_unshifted",
        owner: "BUNDLE-N",
        dets: &[d("tr_policy", "n3a_child_room_is_translated_by_the_segment")],
    },
    Row {
        name: "retf_maxci_unshifted",
        owner: "BUNDLE-N",
        dets: &[d("tr_policy", "n3a_child_room_is_translated_by_the_segment")],
    },
    Row {
        name: "retf_model_func_stale",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_n3_programs")],
    },
    Row {
        name: "retf_entry_hoist",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_n3_programs")],
    },
    Row {
        name: "retf_entry_checks_past",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_n3_programs")],
    },
    Row {
        name: "retf_untraced_aborts",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_retf_traces_are_installed"), d("tr_core", "nr_n3_programs")],
    },
    Row {
        name: "retf_second_return_aborts",
        owner: "BUNDLE-N",
        dets: &[d("tr_core", "nr_retf_traces_are_installed"), d("tr_core", "nr_n3_programs")],
    },
    Row {
        name: "relink_jumps_to_land",
        owner: "BUNDLE-N",
        dets: &[d("tr_exit", "n3b_link_stub_inputs_equal_the_handlers")],
    },
    Row {
        name: "relink_savedpc_off",
        owner: "BUNDLE-N",
        dets: &[d("tr_exit", "n3b_link_stub_inputs_equal_the_handlers")],
    },
    Row {
        name: "tnext_hdr_never",
        owner: "BUNDLE-H",
        dets: &[d("tr_core", "bh_tnext_peels")],
    },
    Row {
        name: "tnextinl_default_ignored",
        owner: "BUNDLE-H",
        dets: &[d("tr_policy", "bh_tnextinl_process_default")],
    },
    Row {
        name: "promote_skip",
        owner: "BUNDLE-J",
        dets: &[
            d("tr_hashd", "hashd_pairs_order_matches_the_reference"),
            d("tr_hashd", "hashd_promoted_hash_is_the_puc_hash"),
        ],
    },
    Row {
        name: "strtab_idx_raw",
        owner: "BUNDLE-J",
        dets: &[d("tr_hashd", "hashd_string_table_stays_consistent")],
    },
    Row {
        name: "chain_cmp_hash_only",
        owner: "BUNDLE-J",
        dets: &[
            d("tr_hashd", "hashd_string_table_stays_consistent"),
            d("tr_hashd", "hashd_pairs_order_matches_the_reference"),
        ],
    },
    Row {
        name: "record_mainpos_raw",
        owner: "BUNDLE-J",
        dets: &[d("tr_hashd", "hashd_recorder_mainpos_uses_the_puc_hash")],
    },
    Row {
        name: "promote_seed0",
        owner: "BUNDLE-J",
        dets: &[d("tr_hashd", "hashd_promoted_hash_is_the_puc_hash")],
    },
    Row {
        name: "strtag_zero",
        owner: "BUNDLE-M",
        dets: &[d("tr_hashd", "hashd_string_table_stays_consistent"), d("tr_hashd", "hashd_string_table_beyond_the_tag")],
    },
    Row {
        name: "strtag_shift",
        owner: "BUNDLE-M",
        dets: &[d("tr_hashd", "hashd_string_table_stays_consistent"), d("tr_hashd", "hashd_string_table_beyond_the_tag")],
    },
    Row {
        name: "strtag_wide",
        owner: "BUNDLE-M",
        dets: &[d("tr_hashd", "hashd_string_table_beyond_the_tag")],
    },
    Row {
        name: "f2ifl_never",
        owner: "BUNDLE-P",
        dets: &[d("tr_asm", "hand_f2ifl_floor_guard"), d("tr_core", "bp_f2ifl_programs")],
    },
    Row {
        name: "f2ifl_all",
        owner: "BUNDLE-P",
        dets: &[d("tr_asm", "hand_f2ifl_floor_guard"), d("tr_core", "bp_f2ifl_programs")],
    },
    Row {
        name: "bytetail_hi_byte_skip",
        owner: "BUNDLE-P",
        dets: &[d("tr_hashd", "bp_fast_hash_equals_reference")],
    },
    Row {
        name: "rearmside_never",
        owner: "BUNDLE-P",
        dets: &[d("tr_policy", "bp_rearmside_rule"), d("tr_core", "bp_rearmside_programs")],
    },
    Row {
        name: "rearmside_any",
        owner: "BUNDLE-P",
        dets: &[d("tr_policy", "bp_rearmside_rule")],
    },
    Row {
        name: "rearmclean_ignores_faults",
        owner: "C45",
        dets: &[d("tr_core", "bp_rearmside_programs")],
    },
    Row {
        name: "rearmclean_ignores_la",
        owner: "C45",
        dets: &[d("tr_core", "bp_rearmside_programs")],
    },
    Row {
        name: "rearmclean_default_ignored",
        owner: "C45",
        dets: &[d("tr_policy", "bp_process_defaults"), d("tr_core", "bp_rearmside_programs")],
    },
    Row {
        name: "centry_default_ignored",
        owner: "BUNDLE-J",
        dets: &[d("tr_policy", "bj_centry_process_default")],
    },
    Row {
        name: "centry_fnmin_kept",
        owner: "BUNDLE-J",
        dets: &[dl("tr_core", "centry_small_comparator_is_a_root")],
    },
    Row {
        name: "centry_weight_ignored",
        owner: "BUNDLE-J",
        dets: &[dl("tr_core", "centry_small_comparator_is_a_root")],
    },
    Row {
        name: "centry_no_head",
        owner: "BUNDLE-J",
        dets: &[dl("tr_core", "centry_small_comparator_is_a_root")],
    },
    Row {
        name: "centry_not_promoted",
        owner: "BUNDLE-J",
        dets: &[dl("tr_core", "centry_small_comparator_is_a_root")],
    },
    Row {
        name: "centry_black_keeps_entering",
        owner: "BUNDLE-J",
        dets: &[dl("tr_core", "centry_blacklisted_comparator_leaves_c_entry")],
    },
    Row {
        name: "centry_wait_ignored",
        owner: "BUNDLE-J",
        dets: &[dl("tr_core", "centry_registration_waits_for_hot_calls")],
    },
    Row {
        name: "lazy_fill_skip",
        owner: "BUNDLE-K",
        dets: &[dl("tr_core", "bk_lazy_programs")],
    },
    Row {
        name: "lazy_leave_root_zero",
        owner: "BUNDLE-K",
        dets: &[d("tr_exit", "lazy_leave_completes_the_chain_like_the_interpreter")],
    },
    Row {
        name: "lazy_fill_top_off",
        owner: "BUNDLE-K",
        dets: &[d("tr_exit", "lazy_leave_completes_the_chain_like_the_interpreter")],
    },
    Row {
        name: "lazy_fill_trap_skip",
        owner: "BUNDLE-K",
        dets: &[d("tr_exit", "lazy_leave_completes_the_chain_like_the_interpreter")],
    },
    Row {
        name: "lazy_fill_cs_off",
        owner: "BUNDLE-K",
        dets: &[d("tr_exit", "lazy_leave_completes_the_chain_like_the_interpreter")],
    },
    Row {
        name: "lazy_fill_newframe_skip",
        owner: "BUNDLE-K",
        dets: &[d("tr_exit", "lazy_leave_completes_the_chain_like_the_interpreter")],
    },
    Row {
        name: "lazy_fill_th_top_skip",
        owner: "BUNDLE-K",
        dets: &[d("tr_exit", "lazy_leave_completes_the_chain_like_the_interpreter")],
    },
    Row {
        name: "lazy_leave_in_handler_skip",
        owner: "BUNDLE-K",
        dets: &[dl("tr_core", "bk_lazy_programs")],
    },
    Row {
        name: "lazy_rec_cell_plain",
        owner: "BUNDLE-K",
        dets: &[dl("tr_core", "bk_lazy_programs")],
    },
    Row {
        name: "lazy_rec_cell_nozero",
        owner: "BUNDLE-K",
        dets: &[dl("tr_core", "bk_lazy_programs")],
    },
    Row {
        name: "lazy_recfail_no_leave",
        owner: "BUNDLE-K",
        dets: &[dl("tr_core", "bk_lazy_programs")],
    },
    Row {
        name: "lazy_stub_no_leave",
        owner: "BUNDLE-K",
        dets: &[dl("tr_core", "bk_lazy_programs")],
    },
    Row {
        name: "lazy_root_not_set",
        owner: "BUNDLE-K",
        dets: &[dl("tr_core", "bk_lazy_programs")],
    },
    Row {
        name: "lazy_ret_root_unchecked",
        owner: "BUNDLE-K",
        dets: &[dl("tr_core", "bk_lazy_programs"), dl("tr_core", "bk_lazy_r2_programs")],
    },
    Row {
        name: "lazy_ret_root_stale",
        owner: "BUNDLE-K",
        dets: &[dl("tr_core", "bk_lazy_programs")],
    },
    Row {
        name: "lazy_mat_no_leave",
        owner: "BUNDLE-K",
        dets: &[dl("tr_core", "bk_lazy_programs")],
    },
    Row {
        name: "aidx_disp_ignored",
        owner: "BUNDLE-L",
        dets: &[dl("tr_core", "bl_programs")],
    },
    Row {
        name: "aidx_off8",
        owner: "BUNDLE-L",
        dets: &[dl("tr_core", "bl_programs")],
    },
    Row {
        name: "aidx_body_reuse",
        owner: "BUNDLE-L",
        dets: &[dl("tr_core", "bl_programs")],
    },
    Row {
        name: "bhoist_no_count_guard",
        owner: "BUNDLE-L",
        dets: &[d("tr_loop", "bhoist_replaces_the_body_bounds_checks_by_pre_roll_guards"), dl("tr_core", "bl_programs")],
    },
    Row {
        name: "bhoist_no_first_guard",
        owner: "BUNDLE-L",
        dets: &[d("tr_loop", "bhoist_replaces_the_body_bounds_checks_by_pre_roll_guards"), dl("tr_core", "bl_programs")],
    },
    Row {
        name: "bhoist_no_last_guard",
        owner: "BUNDLE-L",
        dets: &[d("tr_loop", "bhoist_replaces_the_body_bounds_checks_by_pre_roll_guards"), dl("tr_core", "bl_programs")],
    },
    Row {
        name: "bhoist_last_c_off",
        owner: "BUNDLE-L",
        dets: &[d("tr_loop", "bhoist_replaces_the_body_bounds_checks_by_pre_roll_guards"), dl("tr_core", "bl_programs")],
    },
    Row {
        name: "bhoist_bound_variant",
        owner: "BUNDLE-L",
        dets: &[d("tr_loop", "bhoist_keeps_the_checks_it_cannot_cover"), dl("tr_core", "bl_programs")],
    },
    Row {
        name: "bhoist_step_any",
        owner: "BUNDLE-L",
        dets: &[d("tr_loop", "bhoist_keeps_the_checks_it_cannot_cover"), dl("tr_core", "bl_programs")],
    },    Row {
        name: "t1q_eq_same_false",
        owner: "BUNDLE-Q",
        dets: &[d("tr_core", "bq_t1q_programs")],
    },
    Row {
        name: "t1q_eq_nomm_true",
        owner: "BUNDLE-Q",
        dets: &[d("tr_core", "bq_t1q_programs")],
    },
    Row {
        name: "t1q_eq_second_mm_skip",
        owner: "BUNDLE-Q",
        dets: &[d("tr_core", "bq_t1q_programs")],
    },
    Row {
        name: "t1q_mod_sign_skip",
        owner: "BUNDLE-Q",
        dets: &[d("tr_core", "bq_t1q_programs")],
    },
    Row {
        name: "t1q_mod_negzero",
        owner: "BUNDLE-Q",
        dets: &[d("tr_core", "bq_t1q_programs")],
    },
    Row {
        name: "t1q_mod_inf_fix",
        owner: "BUNDLE-Q",
        dets: &[d("tr_core", "bq_t1q_programs")],
    },
    Row {
        name: "amt_get_nil_skip",
        owner: "BUNDLE-S",
        dets: &[d("tr_core", "bs_amt_programs")],
    },
    Row {
        name: "amt_set_nil_skip",
        owner: "BUNDLE-S",
        dets: &[d("tr_core", "bs_amt_programs")],
    },
    Row {
        name: "amt_set_old_skip",
        owner: "BUNDLE-S",
        dets: &[d("tr_core", "bs_amt_programs")],
    },
    Row {
        name: "s1_self_skip",
        owner: "BUNDLE-S",
        dets: &[d("tr_core", "bs_ixmm_programs")],
    },
    Row {
        name: "s1_get_dst_off",
        owner: "BUNDLE-S",
        dets: &[d("tr_core", "bs_ixmm_programs")],
    },
    Row {
        name: "s1_savedpc_skip",
        owner: "BUNDLE-S",
        dets: &[d("tr_core", "bs_ixmm_programs")],
    },
    Row {
        name: "s1_count_skip",
        owner: "BUNDLE-S",
        dets: &[o("tr_core", "bs_ixmm_programs")],
    },
    Row {
        name: "s1_pay_skip",
        owner: "BUNDLE-S",
        dets: &[o("tr_core", "bs_ixmm_programs")],
    },
    Row {
        name: "s1_land_skip",
        owner: "BUNDLE-S",
        dets: &[d("tr_core", "bs_ixmm_programs")],
    },
    Row {
        name: "s1_obj_orig",
        owner: "BUNDLE-S",
        dets: &[d("tr_core", "bs_ixmm_programs")],
    },
    Row {
        name: "s1_close_skip",
        owner: "BUNDLE-S",
        dets: &[d("tr_core", "bs_ixmm_programs")],
    },
    Row {
        name: "cofuse_room_skip",
        owner: "BUNDLE-T",
        dets: &[d("tr_coro", "co_switch_shapes_match_the_reference")],
    },
    Row {
        name: "cofuse_hook_ignored",
        owner: "BUNDLE-T",
        dets: &[d("tr_coro", "co_hooks_close_and_tracebacks_match_the_reference")],
    },
    Row {
        name: "cofuse_ncalls_skip",
        owner: "BUNDLE-T",
        dets: &[d("tr_coro", "co_cstack_gc_and_overflow_match_the_reference")],
    },
    Row {
        name: "cofuse_finish_op_skip",
        owner: "BUNDLE-T",
        dets: &[d("tr_coro", "co_switch_shapes_match_the_reference")],
    },
    Row {
        name: "cofuse_wanted_nil_skip",
        owner: "BUNDLE-T",
        dets: &[d("tr_coro", "co_switch_shapes_match_the_reference")],
    },
    Row {
        name: "cofuse_results_room_skip",
        owner: "BUNDLE-T",
        dets: &[d("tr_coro", "co_switch_shapes_match_the_reference")],
    },
    Row {
        name: "cofuse_prev_c_ignored",
        owner: "BUNDLE-T",
        dets: &[d("tr_coro", "co_patterns_match_the_reference")],
    },
    Row {
        name: "bv_tail_nil",
        owner: "BUNDLE-V",
        dets: &[d("tr_core", "bv_mm_programs")],
    },
    Row {
        name: "bv_ret_first_nil",
        owner: "BUNDLE-V",
        dets: &[d("tr_core", "bv_mm_programs")],
    },
    Row {
        name: "upbar_skip",
        owner: "BUNDLE-V",
        dets: &[d("tr_record", "bv_upvalue_store_records_the_barrier"), d("tr_core", "bv_upbar_programs"), d("tr_core", "y5_upbar_black_programs")],
    },
    Row {
        name: "upbar_asm_skip",
        owner: "BUNDLE-V",
        dets: &[d("tr_asm", "hand_upvalue_tbar_calls_the_barrier_exactly_for_black_upvalues_and_white_values"), d("tr_core", "y5_upbar_black_programs")],
    },
];

/// Mutants whose injection site exists only under `--cfg rustynail_lua54` (the 5.5 run skips them).
#[cfg_attr(not(all(feature = "tr_mutants", not(miri))), allow(dead_code))]
const V54_ONLY: &[&str] = &[
    "atouch_skip",
    "atouch_hoisted",
    "forloop54_no_ctl_copy",
    "alimit_touch",
    "tfor54_no_ctl_copy",
];

/// Mutants whose injection site is compiled out of the 5.4 build (N7: the end of a traversal is recorded on 5.5 only; the 5.4 run skips them).
#[cfg_attr(not(all(feature = "tr_mutants", not(miri))), allow(dead_code))]
const V55_ONLY: &[&str] = &["gfor_end_meta_skip", "gfor_end_guard_skip", "gfor_end_always", "tnext_key_site_type"];

// ---- consistency of the table with the registry (pure) ---------------------------------------------------------------------------

#[test]
fn table_covers_the_registry_exactly_once() {
    let mut seen = BTreeSet::new();
    for r in TABLE {
        assert!(
            NAMES.contains(&r.name),
            "row {} is not a registered mutant",
            r.name
        );
        assert!(seen.insert(r.name), "row {} appears twice", r.name);
    }
    for n in NAMES {
        assert!(
            seen.contains(n),
            "registered mutant {n} has no row in TABLE (tests/tr_mutants.rs): add its detecting test"
        );
    }
    // the table is in registry order (a diff against the document stays readable)
    let order: Vec<&str> = TABLE.iter().map(|r| r.name).collect();
    assert_eq!(
        order,
        NAMES.to_vec(),
        "TABLE is in the order of mutants::NAMES"
    );
}

// ---- the runner ---------------------------------------------------------------------------------------------------------------

#[cfg(all(feature = "tr_mutants", not(miri)))]
mod runner {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    const HAVE_ORACLE: bool = cfg!(feature = "t15_oracle");

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repo root")
    }

    /// Is the injection site of `name` there? `mutants::active("name")` somewhere under `src` (outside the registry
    /// file), or, for the two rn-core mutants, the forwarding call of `apply_vm_mutants`.
    pub fn injected(name: &str) -> bool {
        fn scan(dir: &Path, pat: &str, skip: &str, out: &mut bool) {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    scan(&p, pat, skip, out);
                } else if p.extension().is_some_and(|x| x == "rs")
                    && p.file_name().is_some_and(|f| f != skip)
                {
                    if let Ok(s) = std::fs::read_to_string(&p) {
                        if s.contains(pat) {
                            *out = true;
                        }
                    }
                }
            }
        }
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = false;
        scan(
            &src, // (all of src: the R1 mutants are injected in rt.rs, outside src/tr)
            &format!("active(\"{name}\")"),
            "mutants.rs",
            &mut found,
        );
        if !found {
            // the recompile accounting mutants go through the wrapper `rt::vbytes_mutant("name")` (`active` itself needs the `trace` feature)
            scan(&src, &format!("vbytes_mutant(\"{name}\")"), "mutants.rs", &mut found);
        }
        if !found {
            // rn-core mutants: `apply_vm_mutants` forwards `active("name")` (inside mutants.rs)
            if let Ok(s) = std::fs::read_to_string(src.join("tr/mutants.rs")) {
                let call = format!("(active(\"{name}\"))");
                found = s.contains(&call);
            }
        }
        found
    }

    fn features() -> String {
        let mut f = String::from("tr_mutants");
        if HAVE_ORACLE {
            f.push_str(",t15_oracle");
        }
        f
    }

    /// Builds the test target `bin` (same features and profile as this binary) and returns its executable.
    fn build(bin: &str) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let release = exe.components().any(|c| c.as_os_str() == "release");
        let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cmd.current_dir(root())
            .args([
                "test",
                "-p",
                "rn-jit",
                "--features",
                &features(),
                "--test",
                bin,
            ])
            .args(["--no-run", "--message-format=json"])
            .env("CARGO_BUILD_JOBS", env_num("TR_MUT_BUILD_JOBS", 4).to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if release {
            cmd.arg("--release");
        }
        let out = cmd.output().ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        for l in text.lines() {
            if l.contains("\"reason\":\"compiler-artifact\"")
                && l.contains(&format!("\"name\":\"{bin}\""))
                && l.contains("\"test\":true")
            {
                let k = l.find("\"executable\":\"")? + "\"executable\":\"".len();
                let end = l[k..].find('"')? + k;
                return Some(PathBuf::from(&l[k..end]));
            }
        }
        None
    }

    /// `(binary, test, env)` of a baseline run.
    type BaseKey = (
        &'static str,
        &'static str,
        &'static [(&'static str, &'static str)],
    );

    #[derive(Clone, Debug)]
    enum Res {
        Pass,
        /// Failed, crashed or timed out; the tail of the output.
        Fail(String),
        /// The test does not exist in this build ("running 0 tests").
        Absent,
    }

    fn env_num(k: &str, default: u64) -> u64 {
        std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
    }

    /// Caps the address space of the child about to be spawned (`RLIMIT_AS`, `mb` MiB; 0: no cap).
    fn cap_memory(cmd: &mut Command, mb: u64) {
        if mb == 0 {
            return;
        }
        unsafe {
            cmd.pre_exec(move || {
                let l = libc::rlimit {
                    rlim_cur: mb << 20,
                    rlim_max: mb << 20,
                };
                libc::setrlimit(libc::RLIMIT_AS, &l);
                Ok(())
            });
        }
    }

    fn run_one(exe: &Path, d: &Det, mutant: &str) -> Res {
        let mut cmd = Command::new(exe);
        // the test binary itself is small; its job children (tr_core) get their own cap, TR_CORE_MEM_MB, from the
        // environment below (a mutated trace can allocate without bound)
        cap_memory(&mut cmd, env_num("TR_MUT_MEM_MB", 4096));
        cmd.env("TR_CORE_MEM_MB", env_num("TR_CORE_MEM_MB", 2048).to_string());
        if d.test == "*" {
            cmd.args(["--skip", "mutants_are_caught", "--test-threads=1"]);
        } else {
            cmd.args(["--exact", d.test, "--test-threads=1"]);
        }
        cmd.env("RUSTYNAIL_JIT_MUT", mutant)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Its own process group: the test binary starts child processes of its own (a mutated trace may loop
            // forever), and killing only the binary used to leave them running for hours, holding the pipes open.
            .process_group(0);
        for (k, v) in d.env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("spawn");
        let pgid = child.id() as libc::pid_t;
        let kill_group = || unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        };
        let mut so = child.stdout.take().unwrap();
        let mut se = child.stderr.take().unwrap();
        let h1 = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = std::io::Read::read_to_string(&mut so, &mut s);
            s
        });
        let h2 = std::thread::spawn(move || {
            let mut s = Vec::new();
            let _ = std::io::Read::read_to_end(&mut se, &mut s);
            String::from_utf8_lossy(&s).into_owned()
        });
        let limit = Duration::from_secs(
            std::env::var("TR_MUT_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(if mutant.is_empty() { 900 } else { 300 }),
        );
        let t0 = Instant::now();
        let mut timed_out = false;
        let status = loop {
            match child.try_wait().expect("wait") {
                Some(s) => break s,
                None if t0.elapsed() > limit => {
                    kill_group();
                    let _ = child.kill();
                    timed_out = true;
                    break child.wait().expect("wait");
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        // Whatever the test binary left behind (children of a mutated run) dies with it.
        kill_group();
        let (so, se) = (h1.join().unwrap(), h2.join().unwrap());
        if status.success() && !timed_out {
            if so.contains("running 0 tests") || !so.contains(" passed") || so.contains(" 0 passed")
            {
                return Res::Absent;
            }
            return Res::Pass;
        }
        let tail = |s: &str| {
            let v: Vec<&str> = s.lines().collect();
            v[v.len().saturating_sub(14)..].join("\n")
        };
        Res::Fail(format!(
            "{}{}{}",
            if timed_out { "TIMEOUT\n" } else { "" },
            tail(&so),
            tail(&se)
        ))
    }

    /// Runs `f` over `jobs` on `TR_MUT_JOBS` worker threads (default 2: every job is a test binary that runs worker
    /// processes of its own), results in job order.
    fn pool<T: Send + Sync, R: Send>(jobs: Vec<T>, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
        let n = env_num("TR_MUT_JOBS", 2).max(1) as usize;
        let queue = Mutex::new(jobs.iter().enumerate().rev().collect::<Vec<_>>());
        let out = Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..n {
                s.spawn(|| loop {
                    let next = queue.lock().unwrap().pop();
                    let Some((i, j)) = next else { break };
                    let r = f(j);
                    out.lock().unwrap().push((i, r));
                });
            }
        });
        let mut v = out.into_inner().unwrap();
        v.sort_by_key(|x| x.0);
        v.into_iter().map(|x| x.1).collect()
    }

    #[test]
    fn every_registered_mutant_is_caught() {
        // TR_MUT_ONLY=a,b: only the mutants whose name contains one of the comma-separated substrings
        let only = std::env::var("TR_MUT_ONLY").ok();
        let strict = std::env::var("TR_MUT_STRICT").is_ok();
        let verbose = std::env::var("TR_MUT_V").is_ok();
        // TR_MUT_SKIP=a,b: leaves out the mutants whose name contains one of the substrings
        let skip: Vec<String> = std::env::var("TR_MUT_SKIP")
            .map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect())
            .unwrap_or_default();
        // TR_MUT_SHARD=i/n: only the rows whose TABLE index is i modulo n (all n shards together run every mutant)
        let shard: Option<(usize, usize)> = std::env::var("TR_MUT_SHARD").ok().and_then(|v| {
            let (i, n) = v.split_once('/')?;
            let (i, n): (usize, usize) = (i.parse().ok()?, n.parse().ok()?);
            (n > 0 && i < n).then_some((i, n))
        });
        // TR_MUT_RESULTS=file: one line per finished mutant (`name<TAB>caught|SURVIVED<TAB>detail`), appended as
        // they finish; with TR_MUT_RESUME=1 the mutants already recorded as caught in it are not run again
        let results_path = std::env::var("TR_MUT_RESULTS").ok();
        let done: BTreeSet<String> = match (&results_path, std::env::var("TR_MUT_RESUME").is_ok()) {
            (Some(f), true) => std::fs::read_to_string(f)
                .unwrap_or_default()
                .lines()
                .filter_map(|l| {
                    let mut it = l.split('\t');
                    let (n, st) = (it.next()?, it.next()?);
                    (st == "caught").then(|| n.to_string())
                })
                .collect(),
            _ => BTreeSet::new(),
        };
        let rows: Vec<&Row> = TABLE
            .iter()
            .enumerate()
            .filter(|(i, _)| shard.is_none_or(|(k, n)| i % n == k))
            .map(|(_, r)| r)
            .filter(|r| !done.contains(r.name))
            .filter(|r| only.as_ref().is_none_or(|o| o.split(',').any(|w| r.name.contains(w))))
            .filter(|r| !skip.iter().any(|s| r.name.contains(s.as_str())))
            .collect();

        // 1. which rows take part, and which binaries they need
        let mut problems: Vec<String> = Vec::new();
        let mut not_injected: Vec<(&str, &str)> = Vec::new();
        let mut needs_oracle: Vec<&str> = Vec::new();
        let mut active: Vec<(&Row, Vec<&Det>, Vec<&Det>)> = Vec::new(); // row, runnable, unavailable
        for r in &rows {
            if !cfg!(rustynail_lua54) && V54_ONLY.contains(&r.name) {
                continue; // a 5.4 mutant: its injection site is compiled out of the 5.5 build (F7a)
            }
            if cfg!(rustynail_lua54) && V55_ONLY.contains(&r.name) {
                continue; // a 5.5 mutant (N7): its site is not reached on 5.4
            }
            if !injected(r.name) {
                not_injected.push((r.name, r.owner));
                if strict {
                    problems.push(format!("{}: not injected yet (owner {})", r.name, r.owner));
                }
                continue;
            }
            if r.dets.is_empty() {
                problems.push(format!(
                    "{}: injected but TABLE names no detecting test (owner {})",
                    r.name, r.owner
                ));
                continue;
            }
            let (run, skip): (Vec<&Det>, Vec<&Det>) = r
                .dets
                .iter()
                .partition(|d| d.need == Need::Plain || HAVE_ORACLE);
            if run.is_empty() {
                // (a build without the oracle cannot run them; the F9 gate and the acceptance run have the feature)
                needs_oracle.push(r.name);
                if strict {
                    problems.push(format!(
                        "{}: all its detecting tests need the feature t15_oracle (run with --features tr_mutants,t15_oracle)",
                        r.name
                    ));
                }
                continue;
            }
            active.push((r, run, skip));
        }

        // 2. build the test binaries
        let bins: BTreeSet<&str> = active
            .iter()
            .flat_map(|(_, run, _)| run.iter().map(|d| d.bin))
            .collect();
        let mut exes: BTreeMap<&str, PathBuf> = BTreeMap::new();
        for b in &bins {
            match build(b) {
                Some(p) => {
                    exes.insert(*b, p);
                }
                None => problems.push(format!("test binary {b} could not be built")),
            }
        }

        // 3. baselines: every distinct (binary, test, env) passes without a mutant
        let mut base_keys: Vec<BaseKey> = Vec::new();
        for (_, run, _) in &active {
            for d in run {
                let k = (d.bin, d.test, d.env);
                if exes.contains_key(d.bin) && !base_keys.contains(&k) {
                    base_keys.push(k);
                }
            }
        }
        let base = pool(base_keys.clone(), |(bin, test, env)| {
            run_one(
                &exes[bin],
                &Det {
                    bin,
                    test,
                    need: Need::Plain,
                    env,
                },
                "",
            )
        });
        let mut base_ok: BTreeSet<(&str, &str)> = BTreeSet::new();
        for ((bin, test, _), r) in base_keys.iter().zip(&base) {
            match r {
                Res::Pass => {
                    base_ok.insert((*bin, *test));
                }
                Res::Absent => {
                    problems.push(format!("baseline {bin}::{test}: the test does not exist"))
                }
                Res::Fail(t) => problems.push(format!(
                    "baseline {bin}::{test} FAILED without a mutant:\n{t}"
                )),
            }
        }

        // 4. every mutant against its tests (one pool unit per mutant, so a finished mutant is reported and recorded
        // at once: a long run can be interrupted and resumed with TR_MUT_RESUME=1)
        let units: Vec<(&Row, Vec<&Det>)> = active
            .iter()
            .map(|(r, run, _)| {
                (
                    *r,
                    run.iter()
                        .copied()
                        .filter(|d| base_ok.contains(&(d.bin, d.test)))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let record = Mutex::new(());
        let ures = pool(units.clone(), |(r, dets)| {
            let rs: Vec<(String, Res)> = dets
                .iter()
                .map(|d| (format!("{}::{}", d.bin, d.test), run_one(&exes[d.bin], d, r.name)))
                .collect();
            let by: Vec<&str> = rs
                .iter()
                .filter(|(_, x)| matches!(x, Res::Fail(_)))
                .map(|(t, _)| t.as_str())
                .collect();
            let (state, detail) = if by.is_empty() {
                ("SURVIVED", rs.iter().map(|x| x.0.as_str()).collect::<Vec<_>>().join(","))
            } else {
                ("caught", by.join(","))
            };
            let _g = record.lock().unwrap();
            eprintln!("[tr_mutants] done {:<26} {state} {detail}", r.name);
            if let Some(f) = &results_path {
                use std::io::Write;
                if let Ok(mut fh) = std::fs::OpenOptions::new().create(true).append(true).open(f) {
                    let _ = writeln!(fh, "{}\t{state}\t{detail}", r.name);
                }
            }
            rs
        });
        let mut by_mutant: BTreeMap<&str, Vec<(String, Res)>> = BTreeMap::new();
        for ((r, _), rs) in units.iter().zip(ures) {
            by_mutant.insert(r.name, rs);
        }

        // 5. the table of results
        eprintln!("\n[tr_mutants] {:<26} {:<8} result", "mutant", "owner");
        let (mut caught, mut missed) = (0, 0);
        for (r, _, skipped) in &active {
            let rs = by_mutant.get(r.name).cloned().unwrap_or_default();
            let by: Vec<&str> = rs
                .iter()
                .filter(|(_, x)| matches!(x, Res::Fail(_)))
                .map(|(t, _)| t.as_str())
                .collect();
            let not_by: Vec<&str> = rs
                .iter()
                .filter(|(_, x)| !matches!(x, Res::Fail(_)))
                .map(|(t, _)| t.as_str())
                .collect();
            if by.is_empty() {
                missed += 1;
                eprintln!(
                    "[tr_mutants] {:<26} {:<8} NOT CAUGHT (passed: {})",
                    r.name,
                    r.owner,
                    not_by.join(", ")
                );
                problems.push(format!(
                    "{}: NOT caught by any of {}",
                    r.name,
                    r.dets
                        .iter()
                        .map(|d| format!("{}::{}", d.bin, d.test))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                if verbose {
                    for (t, x) in &rs {
                        eprintln!("    {t}: {x:?}");
                    }
                }
            } else {
                caught += 1;
                eprintln!(
                    "[tr_mutants] {:<26} {:<8} caught by {}{}{}",
                    r.name,
                    r.owner,
                    by.join(", "),
                    if not_by.is_empty() {
                        String::new()
                    } else {
                        format!("; not by {}", not_by.join(", "))
                    },
                    if skipped.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "; skipped without t15_oracle: {}",
                            skipped
                                .iter()
                                .map(|d| d.test)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                );
            }
        }
        for n in &needs_oracle {
            eprintln!("[tr_mutants] {n:<26} skipped: every detecting test needs t15_oracle");
        }
        for (n, own) in &not_injected {
            eprintln!("[tr_mutants] {n:<26} {own:<8} not injected yet");
        }
        eprintln!(
            "[tr_mutants] shard {:?}, {} previously recorded as caught (TR_MUT_RESUME) and left out",
            shard,
            done.len()
        );
        eprintln!(
            "[tr_mutants] {} registered: {caught} caught, {missed} NOT caught, {} not injected yet (owners: {})",
            rows.len(),
            not_injected.len(),
            {
                let mut o: Vec<&str> = not_injected.iter().map(|x| x.1).collect();
                o.sort_unstable();
                o.dedup();
                o.join(" ")
            }
        );
        assert!(
            problems.is_empty(),
            "mutant runner problems:\n{}",
            problems.join("\n")
        );
    }
}
