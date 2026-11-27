//! Test-only code mutations of the trace compiler (docs/JIT_TRACE.md 9.7). Each mutant is a deliberate bug that the
//! test suite must detect (the `every_mutant` runner of P8a). P0 freezes the name registry ([`NAMES`]); the injection
//! point of each mutant is written by the task that owns the code it breaks (10.1), and that task's acceptance includes
//! "its mutants are caught by its own tests".
//!
//! They exist only under the cargo feature `tr_mutants` (off by default); without it [`active`] is a constant `false`
//! and the optimiser removes every mutation site, so default code is byte-identical. The active mutants come from
//! `RUSTYNAIL_JIT_MUT=name[,name...]` (read once per process; the variable is shared with `t15::mutants`).
//!
//! Meaning of the names: see the bullet of the same name in 9.7 and the rule it cites (TR numbers).

/// The frozen registry of mutant names (9.7), in the order of the document.
pub const NAMES: &[&str] = &[
    // snapshots, counting, landing labels
    "snap_drop_last",
    "snap_prune_dead",
    "refund_off_by_one",
    "land_pre_mid_swap",
    "budget_no_check",
    "budget_no_store",
    // register allocation and exits
    "phi_skip_move",
    "phi_swap",
    "spill_no_reload",
    "exit_rename_ignored",
    "fcmp_negated",
    // folding, aliasing, forwarding, CSE
    "fold_nan",
    "fold_x_plus_zero",
    "alias_may_as_no",
    "alias_upval_by_index",
    "fwd_across_store",
    "alias_maytable_as_must",
    "cse_across_store",
    "tbar_skip",
    // entry checks, guards, polls
    "entry_no_extent",
    "entry_no_ci_spare",
    "entry_no_typecheck",
    "callee_guard_skip",
    "upval_range_skip",
    "poll_skip",
    // frames, lifetime
    "frame_savedpc_caller_off",
    "frame_nresults_off",
    "dep_edge_skip",
    "flush_in_handler",
    "peel_subst_stale",
    // sinking (F8), inlined frames, hooks, validation, memory accounting
    "sink_escape_ignored",
    "sink_store_order",
    "sink_slots_after_alloc",
    "sink_no_gcstp",
    "popped_frame_drop",
    "native_room_skip",
    "native_ci_skip",
    "hooked_head_no_trap",
    "rec_no_validate",
    "phi_before_guard",
    "vbytes_no_credit",
    "vbytes_no_limit_guard",
    // 5.4 (F7a)
    "atouch_skip",
    "atouch_hoisted",
    "atouch_guard_skip",
    "forloop54_no_ctl_copy",
    // v1.3
    "entry_guard_past_store",
    "leave_trap_kept",
    "nonslot_phi_skip",
    "rolled_no_writeback",
    "dep_edge_at_install",
    "cell_stale_version",
    // P4c: exit restore (additive)
    "exit_kcache_wrong",
    "exit_top_off_by_one",
    "exit_th_top_not_written",
    "exit_refund_sign_flipped",
    // P7b: loop optimiser / step guard (additive)
    "bounds_key_no_bound",
    "hdr_op_no_barrier",
    "forloop_step_guard_skip",
    "forloop_step_add_wrong",
    // Q1: parallel-move cycle breaking through V7
    "cycle_stale_xmm",
    // Q2: ineffective-trace rule
    "ineffective_never",
    // Q3: identical-regeneration rule
    "identical_reinstall",
    // F7a: the 5.4 `ATOUCH` assembly (the new array limit is not stored)
    "alimit_touch",
    // F1: hash part (HREFK/HLOAD/HSTORE, field forwarding, metatable flag guards)
    "hrefk_key_skip",
    "hrefk_lsize_skip",
    "hload_typecheck_skip",
    "hstore_oldcheck_skip",
    "hstore_oldcheck_dropped",
    "hstore_tbar_skip",
    "alias_hashtab_as_must",
    "meta_flag_skip",
    "hash_nil_meta_skip",
    // F2a: side traces (exit cells, side recording, count model of a mid-block start, link, family lifetime)
    "side_budget_no_rem",
    "side_start_refund",
    "side_known_all",
    "side_known_unmod_all",
    "tagskip_all",
    "tagskip_nil",
    "tagskip_entry_fail",
    "link2_sig_ignored",
    "link2_cell_stale_after_flush",
    "entry2_skip_entry_ok",
    "entry2_no_poll",
    "exit_cell_ignored",
    "side_cell_stale",
    "side_family_flush_skip",
    "side_link_no_poll",
    "side_link_no_store",
    "side_interp_no_rem",
    "side_maxside_ignored",
    "flush_keeps_cells",
    "ineffective_pct_ignored",
    "prelude_never",
    "prelude_no_chain",
    "side_short_ignored",
    // F2b: linking (nest roots, waiting side exits, ...)
    "nest_link_never",
    "side_untraced_retry",
    "side_forloop_end_abort",
    "forprep_step_guard_skip",
    "frame_link_never",
    "frame_link_cell_ignored",
    "stitch_never",
    // F3: class-A helpers (TNEW, NEWREF, TLEN, SETLIST): helper contract, guards, model sync, optimiser kills
    "tnew_no_gc_check",
    "tnew_bytes_small",
    "newref_present_ok",
    "tnew_ne_guard_skip",
    "newref_ne_guard_skip",
    "newref_no_newindex_guard",
    "tlen_meta_skip",
    "newref_model_stale",
    "newref_header_safe",
    // F5: policy integration with J4 (TR19/TR19b)
    "w_cell_skip",
    "region_wait_stale",
    "close_nest_keep_warm",
    "nest_rule_skip",
    "retired_freed_early",
    "nest_rule_inlined",
    // F3b: generic `for` (ipairs / next), CONCAT, STRTO
    "tfor_callee_guard_skip",
    "tnext_ne_guard_skip",
    "tnext_key_type_skip",
    "snew_no_gc_check",
    "snew_bytes_small",
    "concat_no_gc_check",
    "concat_bytes_small",
    "concat_kind_skip",
    "snew_ne_guard_skip",
    "concat_ne_guard_skip",
    "concat_conv_slot_skip",
    "strto_ne_guard_skip",
    "strto_mode_swap",
    "tfor54_no_ctl_copy",
    "tfor_lua_ret_cost_skip",
    "concat_key_traced",
    // F4: recursion unroll, TAILCALL, multiple results with an explicit `top`
    "tail_flag_skip",
    "exit_top_ignored",
    "exit_top_off_by_one_slot",
    "snap_top_dropped",
    "top_count_off",
    "unroll_off_by_one",
    "tail_args_not_moved",
    "tail_nil_fill_skip",
    // S0: allocation sinking and the TR3b writeback
    "sink_vbytes_zero",
    "sink_no_uncharge",
    "wb_loaded_slot_stored",
    // F8: a failed allocation while materialising raises the memory error (TR29.3); the recorder watches the tables it records
    "sink_mem_error_dropped",
    "rec_no_watch",
    // P-A: `__index` chains in traces (absent-key proof, metatable and `__index` guards, end-of-chain flag guard)
    "hmiss_skip",
    "hmiss_lsize_skip",
    "hmiss_next_skip",
    "hmiss_key_skip",
    "index_mt_null_skip",
    "index_tab_typecheck_skip",
    "index_nilnode_skip",
    "chain_end_skip",
    // P-E: inlined vararg callees (`VARARGPREP` frame layout, `VARARG`, `RETURN` of a vararg frame) and `select`
    "vararg_nextra_off",
    "vararg_room_skip",
    "vararg_orig_not_nilled",
    "vararg_delta_off",
    "vararg_ret_dest",
    "vararg_first_nil",
    "vararg_top_off",
    "select_guard_skip",
    "select_count_off",
    "select_index_off",
    // P-E2: function-entry roots
    "fn_exits_not_pre",
    "fn_ineff_pct_ignored",
    "pack_asize_off",
    "pack_n_off",
    "fn_head_calls_ignored",
    "fn_side_at_return",
    "fn_head_tbc_ignored",
    "fn_short_budget_ignored",
    // P-B: library-call intrinsics (`TOp::Lib`), fast concat, string equality
    "lib_ne_guard_skip",
    "lib_bytes_small",
    "lib_no_gc_check",
    "lib_kind_skip",
    "sub_start_off",
    "byte_plus1_skip",
    "type_closure_unchecked",
    "tostr_meta_skip",
    "tinsert_meta_skip",
    "streq_guard_skip",
    "tlen_str_wide",
    "storm_guard_skip",
    "storm_rule_skip",
    "storm_site_skip",
    // P-F: run-time loop steps (sign guard + variable divisor / literal detection)
    "forloop_step_always_var",
    "forloop_step_always_lit",
    "forprep_step_always_lit",
    "forprep_var_sign_skip",
    "sidemin_default_ignored",
    "forprep_huge_diff_record",
    // P-L: one poll word, memory-form exit counters
    "poll_exit_no_trap",
    "poll_exit_flag_kept",
    "exit_mem_counter_no_divert",
    "exit_mem_counter_no_count",
    // P-N: power-of-two divisors without idiv
    "mod_pow2_mask_k",
    "idiv_pow2_logical",
    // P-X: the landing label of a typed mid-block pc verifies the tags tier 1 assumes there
    "land_type_check_skip",
    // P-V: `s:method` resolved through the constant string metatable
    "strmt_flush_skip",
    // P-M3: native self-recursion
    "rec_push_no_ci_next",
    "rec_push_top_off",
    "rec_push_callstatus_off",
    "rec_push_savedpc_off",
    "rec_ret_site_unchecked",
    "rec_ret_base_off",
    "rec_inner_room_skip",
    // P-O: `pcall` / `xpcall` of a Lua closure as a trace construct
    "pcall_impure_ignored",
    "pcall_shift_off",
    "pcall_ccalls_skip",
    "pcall_room_skip",
    "ccalls_guard_off",
    // BUNDLE-D: `ccalls` (inline C-stack check), `armm` (tier-1 arithmetic metamethod continuation), `sbump2`
    "ccalls_inline_off_by_one",
    "armm_never",
    "armm_count_skip",
    "armm_land_skip",
    "armm_pay_skip",
    "armm_dst_off",
    "stitch_side_empty",
    "fd_room_extent_off",
    "sbump2_never",
    "sbump2_unbounded",
    // BUNDLE-H1: integer keys in the hash part (`HREFV` integer form) and frame cells for function-entry / recursion traces
    "hrefi_mod_size",
    "hrefi_lsize_skip",
    "hashint_nil_meta_skip",
    "hashint_newindex_skip",
    // H1b (CP5): the absent integer-key read in traces (`Rec::int_absent`)
    "iabs_meta_skip",
    "iabs_array_guard_skip",
    "iabs_lsize_skip",
    "fnpromo_kl_keep",
    "fcfn_family_excluded",
    "fcfn_cells_off",
    "fck_ignored",
    "framemin_ignored",
    "sidefwd_off",
    "end_exit_folded",
    // BUNDLE-G2
    "g2_ccalls_no_hoist",
    "rec_inner_first_stale",
    // P-M4: the process default of `recnat`
    "recnat_default_ignored",
    // N2: the bulk element move of `table.insert/remove/move` (rn-core `TableRef::arr_move`)
    "tbl_move_meta_skip",
    // N5: string loads that do not specialise on the short / long variant (the pass relaxes a load whose variant a consumer needs)
    "relaxstr_any_use",
    // N1: upvalues that point into the root frame are recorded as the register access under an identity guard
    "uvslot_guard_skip",
    "uvslot_off_by_one",
    "uvslot_as_cell",
    "uvslot_store_skip",
    // N6: guard-free hash lookup for dynamic short-string keys (`HREFV`)
    "hrefv_chain_stops_early",
    "hrefv_mask_off_by_one",
    "alias_hrefv_as_no",
    "alias_hrefv_key_as_must",
    "hrefv_meta_skip",
    "gfor_pairs_meta_skip",
    "gfor_wrong_iterator",
    "gfor_result_count",
    "gfor_ipairs_start_one",
    "gfor_end_meta_skip",
    "gfor_end_guard_skip",
    "gfor_end_always",
    "tforprep_no_swap",
    "tnext_key_site_type",
    // N3: metamethod function calls (stage 1: arithmetic, unary, `#`, `..`)
    "mm_flag_skip",
    "mm_ctx_off",
    "mm_ncalls_skip",
    "mm_follower_free",
    "mm_result_skip",
    "mm_result_slot_off",
    "mm_mt_null_skip",
    "mm_tm_typecheck_skip",
    "mm_callee_guard_skip",
    "mm_num_mt_skip",
    "mm_absent_skip",
    "mm_ccalls_skip",
    // N3 stage 2: comparison metamethods
    "mm_cond_inverted",
    "mm_eq_raw_skip",
    "mm_eq_absent_skip",
    // N3 stage 3: `__index` / `__newindex` functions, `__call`
    "mm_index_range_skip",
    "mm_index_dummy_skip",
    "mm_newindex_absent_skip",
    "mm_call_shift_off",
    "rawget_str_absent_skip",
    // N3b: `setmetatable` as a recorded intrinsic, `return f(...)` of a native
    "setmt_protect_skip",
    "setmt_gc_skip",
    "setmt_barrier_skip",
    "setmt_result_mt",
    "setmt_model_skip",
    // N3b: allocation exits inside metamethod frames restart the triggering instruction
    "alloc_shallow_impure",
    // R1: the inline `TAILCALL` of tier 1
    "tailfast_args_skip",
    "tailfast_copy_short",
    "tailfast_status_skip",
    // A1: the inline `NEWREF` of a constant key into a table the trace allocated
    "nr_key_skip",
    "nr_next_skip",
    "nr_flags_skip",
    // A1: the inline `setmetatable`
    "smi_gcbit_skip",
    "smi_black_skip",
    "smi_old_skip",
    // the inline `#t` (trace `TLEN` and tier-1 `LEN`): previous-slot / next-slot / dummy-hash / (tier 1) metatable tests
    "tlen_inline_prev_skip",
    "tlen_inline_next_skip",
    "tlen_inline_dummy_skip",
    "tlen_inline_mt_skip",
    "tlen_inline_hint_skip",
    "tlen_inline_zero_bound_skip",
    // S1: side traces from exits inside inlined frames (virtual frames, cells of frameful exits)
    "frame_side_never",
    "frame_cell_no_test",
    "frame_cell_unwind_skip",
    "fside_rem_root",
    "fside_room_ignored",
    "fside_call_pc_off",
    "fside_nresults_off",
    "fside_tail_dropped",
    "fside_mm_plain",
    "fside_frames_dropped",
    // H2: the inline stub of a frameful exit rebuilds the inlined frames itself
    "fmat_savedpc_caller_off",
    "fmat_top_off_by_one",
    "fmat_nresults_off",
    "fmat_tail_flag_skip",
    "fmat_trap_skip",
    "fmat_th_top_skip",
    "fmat_mask_skip",
    // H3-A: a blacklisted exit keeps being reported to the handler by the frame-materialising stub
    "black_exit_report",
    // H3-G: the per-site sinking policy
    "sink_demote_ignored",
    // H3-F: the 64-bit key compare of HREFK
    "hrefk_key64_hi",
    // M1: tier 1's `k` fast paths, side recordings that stitch at recursion, nil array loads
    "tailfast_close_skip",
    "side_stitch_never",
    "aload_nil_skip",
    "depth_stitch_never",
    // F1: the constant-exponent pow fold, n-ary min/max, the string-length load, the sin helper
    "f1_pow_any",
    "f1_minmax_order",
    "f1_strlen_off",
    "f1_sin_cos_swap",
    "f1_ceil_as_floor",
    // R2: native returns of `RETURN0` / `RETURN1` into call sites that want more results, the caller `top` after a native return,
    // the guard of a `for` loop end that a continuation trace records
    "rec_ret_wanted_unchecked",
    "rec_ret_top_off",
    "rec_ret_cframe_skip",
    "rec_ret_regs_unsaved",
    "rec_loop_end_guard_skip",
    // R2: the room checks a frameful side trace makes itself
    "side_room_check_skip",
    "side_room_meta_full",
    // R2: stores into hash nodes that hold nil: the `__newindex` guard, the metamethod-name exclusion
    "nilstore_newindex_skip",
    "nilstore_meta_name_ok",
    // R2: the process defaults of `recany`, `siderooms`, `nilstore`
    "r2_default_ignored",
    // B2: scaled array addressing, the array-read metatable guard, the known-tag store skip
    "b2arr_no_not",
    "b2arr_off8",
    "b2mt_store",
    "b2tag_alias_keep",
    "b2hk_lea_off",
    // F2: library-call recorders
    "strcmp_lt_as_le",
    "strcmp_guard_skip",
    "getmt_null_skip",
    "getmt_protected_skip",
    "assert_extra_dropped",
    "rawset_tbar_skip",
    "rawset_result_nil",
    "byte_inline_off_by_one",
    "byte_inline_len_skip",
    "byte_range_guard_skip",
    "byte_range_count_off",
    // B1 (`pureb`): the range's `j <= #s` guard is not emitted (a string shorter than `j` gives a `BYTE` of 0, i.e. -1)
    "pureb_len_guard_skip",
    // B2 (`opencall`): the open-result `CALL` defines every register from `A - 2` up
    "opencall_defs_low",
    // S73 (`landfix`): the typed landing labels rewrite the tag of a LIVE known register too (instead of exiting on a mismatch)
    "landfix_live_rewrite",
    // section 75 (`oframe`): the snapshots of an inlined frame drop the slots above the ROOT frame's top (live callee registers)
    "oframe_root_top",
    // section 75 (`oframe`): the arguments of a `CALL` that reach above the frame's top (`__call` shift, native self-call) are dropped too
    "oframe_floor_skip",
    // section 75 (`retfuse`): the fused tail of return site k pops to the frame and cell of site k + 1
    "retfuse_site_next",
    // section 75 (`retfuse`): the region root of a lazy family returns through the tail of the frames above it (no root bookkeeping)
    "retfuse_root_as_nonroot",
    // section 75.7 (`dynunroll`): an inlined `for` with run-time bounds keeps the recorded count as a constant / its skip is not guarded
    "dynunroll_count_const",
    "dynunroll_skip_unguarded",
    // section 75.12 (`mfkeys`): the region check does not verify the other method keys
    "mfkeys_check_skip",
    // section 75.14 (`mfshare`): a later lookup reuses the first lookup of ANY extra key
    "mfshare_any_key",
    // section 77.3 (`uvk`): writes into read-only variables flush nothing / the closure's identity is not guarded / any upvalue is folded /
    // the collection of a closure held by identity is not reported
    "uvk_flush_skip",
    "uvk_ident_skip",
    "uvk_any_upvalue",
    "uvk_dep_skip",
    // M1 (rn-std `tmovepre`): `move_presize` ignores holes of the source range
    "tmove_presize_holes",
    // R1 (rn-std `tpop`): the `table.remove(t)` fast path ignores the metatable
    "tpop_meta_skip",
    // S6 (rn-std `gmplus`): `[^%x]+` is scanned as `%x+` by the direct `gmatch` scan
    "gmplus_set_neg_skip",
    "random_range_off",
    // C2: tier-1 comparisons of non-numbers (`jit_cmp`)
    "c2cmp_swap",
    "c2cmp_exit_skip",
    // BUNDLE-B: tier-1 comparisons that call a Lua `__lt` / `__le` / `__eq`
    "mmret_nc_skip",
    "mmret_flag_false",
    "mmcmp_savedpc_skip",
    "c2setupval_barrier_skip",
    // T3: the inline first `__index` level of tier 1
    "t3idx_nil_hit",
    "t3idx_use_mt",
    // BUNDLE-K item B: the inline string `SELF` of tier 1
    // BUNDLE-L: D1 (upvalue barrier, absent-key loads, CONCAT keys)
    "habs_found_ignored",
    "strself_tag_skip",
    "strself_mt_skip",
    "strself_nil_hit",
    // BUNDLE-T1I: the inline built-in calls of tier 1
    "t1i_type_ident_skip",
    "t1i_lcf_ident_skip",
    "t1i_floor_inexact",
    "t1i_minmax_le",
    "t1i_byte_lo_skip",
    "t1i_abs_float_sign",
    "t1i_select_count",
    "t1i_rawlen_next_skip",
    "t1i_sqrt_int_bits",
    "t1i_type_index",
    "t1i_setmt_gc_skip",
    "t1i_setmt_black_skip",
    "t1i_setmt_old_skip",
    "t1i_setmt_arg_skip",
    "t1i_tail_count_extra",
    "t1i_tail_mm_skip",
    "newkey_black_skip",
    "newkey_nil_value",
    "newkey_nonempty_skip",
    "newkey_mt_skip",
    "newkey_flags_skip",
    "newkey_layout_skip",
    "newkey_mainpos_off",
    "newkey_dummy_skip",
    "tailfast_dyn_count_short",
    "tailfast_dyn_top_high",
    "sinkfast_order_reversed",
    "sinkfast_flags_skip",
    "t3_default_ignored",
    "ineff_cover_ignored",
    "typesides_ignored",
    "ineff_hop_never",
    "hop_top_ignored",
    "ineff_persist_never",
    "p2_default_ignored",
    // BUNDLE-1
    "tailhook_ignored",
    "hdrlib_never",
    "hdrlib_table_tlen",
    "hdrlib_setmt",
    "hdrlib_newref",
    "hdrlib_third_stale",
    "hdrlib_sub_always",
    "hdrlib_sub_phi_stale",
    "libsub_lean_args_swapped",
    "tonum_type_guard_skip",
    "tonum_mode_ignored",
    "next_ctl_nil",
    "find_specials_skip",
    "find_end_off",
    "find_init_neg_skip",
    "find_outcome_guard_skip",
    "tfmt_kind_skip",
    "tfmt_fmt_guard_skip",
    "tfmt_args_swap",
    "tfmt_stash_stale",
    "next_callee_guard_skip",
    "wgate_full_rearm",
    "b1_default_ignored",
    // BUNDLE-4
    "nopay_all_types",
    "divmagic_no_sign",
    "divmagic_shift_off",
    "hkfuse_tag_skip",
    // BUNDLE-J1
    "hkfuse_split_never",
    "hkfuse_cold_swap",
    "hkfuse_state_skip",
    "hmiss_key64_hi",
    "eqref_proto64_hi",
    "sinkstub_never",
    "sinkstub_save_skip",
    "sinkstub_err_ignored",
    "sinkstub_frames_never",
    // BUNDLE-5
    "b5_default_ignored",
    "sinkmt_never",
    "sinkmt_gc_guard_skip",
    "sinkmt_no_mt_materialise",
    "sinkside_dead_all",
    // SINK-F
    "sunkframe_cell_nomat",
    "sinkroot_never",
    "sinkroot_default_ignored",
    // BUNDLE-B
    "bb_default_ignored",
    "nilset_flags_skip",
    "nilset_mt_skip",
    "nilset_barrier_skip",
    "nilset_arr_mt_skip",
    "tset_str_tag_skip",
    "tset_int_key_unchecked",
    // BUNDLE-G1
    "tget_nil_hit",
    "tget_tag_skip",
    "g1fmt_fmt_guard_skip",
    "g1fmt_result_fmt",
    "streq_inline_len_diff_eq",
    "streq_inline_tail_ne_eq",
    "streq_inline_tail_eq",
    // BUNDLE-J2
    "deep_never",
    "deep_every_time",
    "deep_limits_ignored",
    "deep_ins_unbounded",
    "deep_default_ignored",
    "deep_hop_rule_kept",
    // DYNMASK: dynamic keys with the node count and node vector read at run time, nil-miss loads, `NEWREF` for absent dynamic stores
    "dynmask_mask_off_by_one",
    "dynmask_cse_merge",
    "dynmask_nil_store",
    "dynmask_nil_meta_skip",
    // BUNDLE-M: C1 (lockstep off: the recorder's own bookkeeping)
    "nr_img_record_skip",
    "light_charge_skip",
    "light_tnew_unregistered",
    // BUNDLE-O: side traces beyond the family cap for hot exits (polymorphic sites)
    "polyside_default_ignored",
    "polywait_ignored",
    "polyframes_ignored",
    // K4: the family guard of side recordings that abort `InnerLoop` inside inlined frames
    "innerside_ignored",
    // BUNDLE-S: `loopany` (unrolled callee loops) default and its side-attempt guard
    "loopany_default_ignored",
    "loopany_guard_never",
    // BUNDLE-P: the loopany guard prunes only the unrolling side traces when the family root unrolled nothing
    "laprune_flushes_root",
    "laprune_never_flushes_sides",
    // C36: `cispare`
    "cispare_never",
    "cispare_flush_anyway",
    // K4: the in-place spare-`CallInfo` repair at the entry of a root
    "rootci_never",
    "rootci_short",
    // CP1: spare `CallInfo` repair at the TR6b check of natively entered frames
    "recci_never",
    "recci_short",
    "recci_lazy_stale",
    "retf_lazy_caller_ok",
    // CP2: frameless side traces without entry room checks when the parent covers them
    "side_nroom_skip",
    // C36: `domtop`
    "dom_never",
    "dom_repeats",
    // C36: `second`
    "second_retry_never",
    "second_retry_repeats",
    "second_identical_never",
    "second_identical_repeats",
    // P3: `chainre`
    "chain_never",
    "chain_unbounded",
    "chain_pass_ignored",
    "chain_gain_ignored",
    "chain_stub_never",
    "chain_stub_no_test",
    "chain_no_rebase",
    "chaindepth_ignored",
    "chaindepth_cover_ignored",
    // P3: `ncallee`
    "ncallee_never",
    "ncallee_without_entry_guard",
    // P2-3: `budsub`, `pollr15`, `fpool`
    "budget_refund_skip",
    "budget_refund_short",
    "poll_r15_disp_off",
    "fpool_bits_off",
    "mul2_any_const",
    "loopdiet_default_ignored",
    // P2 bundle 1: `xregs`
    "xregs_no_reloadk",
    "xregs_rec_r14",
    // R1: `rearm`
    "rearm_never",
    "rearm_warm_credit",
    "rearm_any_head",
    "rearm_tick_reset",
    "rearm_default_ignored",
    // P2 bundle 2: `uvsep`
    "uvsep_no_guard",
    "uvsep_first_guard_only",
    // BUNDLE-M: the inline traversal step (`tnextinl` bit 1)
    "tnext_inl_nonpos_skip",
    "tnext_inl_arr_bound_skip",
    "tnext_inl_array_end_skip",
    "tnext_inl_key_tag_skip",
    "tnext_inl_key_payload_skip",
    "tnext_inl_node_empty_skip",
    "tnext_inl_spill_skip",
    // BUNDLE-L: `cloreuse`
    "cloreuse_guard_skip",
    // BUNDLE-N: inline `math.random` (`randinl`)
    "randinl_empty_skip",
    "randinl_m1_skip",
    "randinl_lim_off",
    "randinl_rot_off",
    "randinl_s3_rot_off",
    "randinl_commit_skip",
    "randinl_float_shift",
    "randinl_retry_skip",
    "randinl_tag_skip",
    "randinl_restore_skip",
    "cleanup_third_dead",
    "cleanup_third_stale",
    // C39: `innerretry`
    "inner_retry_never",
    "inner_retry_unbounded",
    // C38: `sidenest`
    "side_frame_link_never",
    // K3: a side recording that reaches a callee loop head with a root trace before `sidemin` bytecodes goes on through the loop; `fretry` (the retry of a
    // `SideShort` frame attempt is not held to `framemin`)
    "frame_short_aborts",
    "fretry_never",
    "fretry_always",
    // BUNDLE-C: `sbump` (the retry of a side attempt that ended `SideShort` unrolls one level deeper) and `fnpromo` (per-site function roots)
    "sbump_never",
    "fnpromo_never",
    "fnpromo_repeats",
    "fnself_never",
    "fnself_unbounded",
    "fnself_flag_ignored",
    "fnself_recursive_ignored",
    "fnself_mark_skip",
    // BUNDLE-GM: the `gmatch` iterator helper
    "gm_kind_skip",
    "gm_no_commit",
    "gm_end_guard_skip",
    "gm_storm_guard",
    // C43: the exit storm spared the guard behind the `RANDOM` helper no more
    "rand_storm_guard",
    // BUNDLE-H (`libcall`): n-ary `string.char`, `math.random()`, `string.lower` / `upper` / `reverse` / `rep` on trace
    "charn_range_skip",
    "charn_order",
    "randf_plus_zero",
    "randf_scale_off",
    "lower_is_upper",
    "rep_cap_skip",
    "rep_sep_trailing",
    // BUNDLE-H (`libcall` bits 1 and 2): lean `LIB` entries, inline `type`
    "libk_lean_cmp_swapped",
    "libk_lean_char_args",
    "libk_type_fn_skip",
    "libk_type_index_off",
    "libcall_default_ignored",
    // BUNDLE-R
    "repcls_guard_skip",
    "tunpack_meta_skip",
    "tunpack_len_guard_skip",
    "tunpack_off_by_one",
    "tonum_fail_guard_skip",
    "libk_lean_find_swapped",
    "tmove_args_guard_skip",
    "tmove_same_guard_skip",
    "tmove_order_flip",
    // BUNDLE-VA: the same-table move reads its elements before the first store
    "tmove_no_preread",
    // BUNDLE-TM: the same-table move emits the guards of every destination element before the first store
    "tmove_interleaved_guards",
    // BUNDLE-N (RETF; N2a)
    "seg_root_code_stale",
    "seg_land_proto_stale",
    "retf_callstatus_unchecked",
    "retf_cframe_unchecked",
    "retf_site_unchecked",
    "retf_k_unchecked",
    "retf_shift_off",
    "retf_no_ci_store",
    "retf_no_reloadk",
    "retf_uvrel_cse",
    "retf_model_uvrel_unshifted",
    "retf_slot_remap_off",
    "retf_results_dest_off",
    "retf_multret_top_unmapped",
    "retf_root_func_stale",
    "retf_ci_stays",
    // BUNDLE-N (N3a)
    "retf_known_unshifted",
    "retf_func_ref_stale",
    "retf_extent_unshifted",
    "retf_maxci_unshifted",
    "retf_model_func_stale",
    "retf_entry_hoist",
    "retf_entry_checks_past",
    "retf_untraced_aborts",
    "retf_second_return_aborts",
    "relink_jumps_to_land",
    "relink_savedpc_off",
    // BUNDLE-H (`tnextinl`)
    "tnext_hdr_never",
    "tnextinl_default_ignored",
    // BUNDLE-J (J2 `hash-D`, lazy PUC hash of short strings; the sites are in rn-core `string.rs` / `heap.rs` / `table.rs`)
    "promote_skip",
    "strtab_idx_raw",
    "chain_cmp_hash_only",
    "record_mainpos_raw",
    "promote_seed0",
    "strtag_zero",
    "strtag_shift",
    "strtag_wide",
    // BUNDLE-P: P2 `f2ifl` (cheap float-to-int guard behind FFLOOR), P3 byte-load tail of `fast_hash`
    "f2ifl_never",
    "f2ifl_all",
    "bytetail_hi_byte_skip",
    // BUNDLE-P P1 `rearmside` (rearmed entries into young heads whose family has installed side traces)
    "rearmside_never",
    "rearmside_any",
    // C45 `rearmclean` (young rearm entries only into clean families)
    "rearmclean_ignores_faults",
    "rearmclean_ignores_la",
    "rearmclean_default_ignored",
    // BUNDLE-J (J1 `centry`, C-entered function roots)
    "centry_default_ignored",
    "centry_fnmin_kept",
    "centry_weight_ignored",
    "centry_no_head",
    "centry_not_promoted",
    "centry_black_keeps_entering",
    "centry_wait_ignored",
    // BUNDLE-K L1 (`lazyci`)
    "lazy_fill_skip",
    "lazy_leave_root_zero",
    "lazy_fill_top_off",
    "lazy_fill_trap_skip",
    "lazy_fill_cs_off",
    "lazy_fill_newframe_skip",
    "lazy_fill_th_top_skip",
    "lazy_leave_in_handler_skip",
    "lazy_rec_cell_plain",
    "lazy_rec_cell_nozero",
    "lazy_recfail_no_leave",
    "lazy_stub_no_leave",
    "lazy_root_not_set",
    "lazy_ret_root_unchecked",
    "lazy_ret_root_stale",
    "lazy_mat_no_leave",
    "lazy_root_link_root_unset",
    // BUNDLE-L A / B (array loops)
    "aidx_disp_ignored",
    "aidx_off8",
    "aidx_body_reuse",
    "bhoist_no_count_guard",
    "bhoist_no_first_guard",
    "bhoist_no_last_guard",
    "bhoist_last_c_off",
    "bhoist_bound_variant",
    "bhoist_step_any",
    // 5.4 gaps: the control-variable alias of a numeric `for` loop (`opt_loop::ctl_pairs`)
    "ctl_alias_no_guard",
    // BUNDLE-Q (`t1q`): tier-1 `EQ` of objects answered without the interpreter, float `MOD` through `jit_fmod`
    "t1q_eq_same_false",
    "t1q_eq_nomm_true",
    "t1q_eq_second_mm_skip",
    "t1q_mod_sign_skip",
    "t1q_mod_negzero",
    "t1q_mod_inf_fix",
    // BUNDLE-S S2 (`amt`): array reads / stores of tables that have a metatable
    "amt_get_nil_skip",
    "amt_set_nil_skip",
    "amt_set_old_skip",
    // BUNDLE-S S1 (`t1q` bits 2 / 3): Lua `__index` / `__newindex` frames in tier 1
    "s1_self_skip",
    "s1_get_dst_off",
    "s1_savedpc_skip",
    "s1_count_skip",
    "s1_pay_skip",
    "s1_land_skip",
    "s1_obj_orig",
    "s1_close_skip",
    // BUNDLE-T (`coro_fused`): the fused coroutine resume (rn-core `State::resume_fused`)
    "cofuse_room_skip",
    "cofuse_hook_ignored",
    "cofuse_ncalls_skip",
    "cofuse_finish_op_skip",
    "cofuse_wanted_nil_skip",
    "cofuse_results_room_skip",
    "cofuse_prev_c_ignored",
    // N2 (`coro_noyield` / `coro_noresume`): the direct `coroutine.yield` and the inlined fused resume
    "coyield_yieldable_skip",
    "coyield_nargs_wrong",
    "coyield_status_skip",
    "coinl_err_as_ok",
    // N2b (`coro_nodirect` / `coro_nostart`): the direct `coroutine.wrap` function / `coroutine.resume`, the fused start
    "codirect_hook_ignored",
    "codirect_true_skip",
    "codirect_nil_pad_skip",
    "codirect_rest_flag",
    "costart_func_off",
    // BUNDLE-V: V1 (`t1q` bits 4 / 5: arithmetic metamethod frames, tail calls and general returns in metamethod frames), V2 (`upbar`)
    "bv_tail_nil",
    "bv_ret_first_nil",
    "upbar_skip",
    "upbar_asm_skip",
    // BUNDLE-Q: in-place last PHI update
    "phi_inplace_nocopy",
    // F4 (`snapprune`): bytecode-liveness pruning of the dead numeric temporaries of snapshots
    "prune_live_temp",
    "prune_local",
    "prune_collectable",
    // (`prune_loop_carried`: liveness without the back-edges and without the protection of the locals, so a value that the next iteration reads looks dead)
    "prune_loop_carried",
    "prune_after_call",
    // F5 (`hstoreguard`): the `__newindex` guard that replaces the old-value check of `HSTORE`
    "hstore_nomt_check_skipped",
    "hstore_mt_stale",
    "hstore_known_nonnil_wrong",
    // G1 (`guarddiet`): transitive entry state of a side trace's chain, room checks of a guard-free link entry
    "entry2_transitive_stale",
    "entry_inherit_wrong_slot",
    "room_skip_bigger_frame",
    // H1 (`hkaddr`): the `HREFK` address folded into the access
    "hrefk_fold_wrong_disp",
    // S4 (layout id of hash parts with >= 8 nodes): the clears at every key-change site (rn-core `table.rs`) and the inline NEWREF's
    "shape_stamp_not_bumped_on_newkey",
    "shape_collide_move_no_clear",
    "shape_gc_deadkey_reuse",
    "shape_weak_clear_skip",
    "shape_nr_id_skip",
    // S4 (`shapeguard`): the shape guard (`HSHAPE`) of the recorder, the interning of ids and the exit policy
    "shape_guard_skipped",
    "shape_stale_across_rehash",
    "shape_header_fact_kept_over_alloc",
    "shape_demote_never",
    "shape_id_reuse",
    "shape_miss_counts_hot",
    "shape_fallback_key_skip",
    "shape_nofill_ignored",
    "shape_drop_keeps_cover",
    // R1 (`regcarry`): register-carrying transitions (docs/JIT_TRACE.md section 16)
    "carry_plain_no_store",
    "carry_drop_move",
    "carry_in_drop_move",
    "carry_store_dropped",
    "carry_exit_tag_skip",
    "carry_no_stale_flag",
    "carry_dense_skip",
    "carry_sig_ignored",
    "carry_wb_prune_live",
    "carry_demote_copied",
    "carry_entry_snap_bare",
    // R1 / K5: the hoisted entry checks of a side trace with carried loads: placed in front of the `BUDGET` charge (refund `n` for no charge) with an entry snapshot that does not name the carried slots
    "entry_checks_before_budget",
    // H7 (recursion protocol diet)
    "addframe_push_off",
    "stack_cmp_strict",
    "ci_next_wrong_word",
    // K1: mixed integer / float `math.min` / `math.max` (the recorded compare decides the branch and the result type), `table.remove(t, pos)`
    "k1_mix_take_a",
    "k1_mix_le",
    "k1_trm_len_guard_skip",
    "k1_trm_pos_guard_skip",
    "k1_trm_shift_off",
    "k1_trm_last_keep",
    // K2: method recursion marks the prototype `selfref` (the extended `FnShort` budget of its entry head)
    "fnrec_mark_skip",
    "fnrec_proto_ignored",
    // K5 (`slotlive`): a store over a slot that the trace already proved non-nil takes no `__newindex` / old-value guard
    "slotlive_nil_between",
    "slotlive_index_off",
    "slotlive_hload_stale",
    // L1 (`bytext`): the inline `string.byte` reads long strings with external bytes through their pointer
    "bytext_inline_read",
    // L2: the first two keys of a fresh object table without the general rehash chain (rn-core `insert_small`)
    "smallins_order_swap",
    // K7 (an AREF is never a PHI)
    "aref_phi_cse",
    // S1 (`lazystub`): deferred exit stubs (docs/JIT_TRACE.md section 31)
    "lazystub_reloadk",
    "lazystub_resume_v8",
    "lazystub_resume_xmm",
    "lazystub_dbl_skip",
    "lazystub_site_rel_pos",
    // P1 (`presize`): constant-key insertion groups into a fresh table presized to the interpreter's final layout (docs/JIT_TRACE.md section 32)
    "p1_image_check_skip",
    "p1_window_any",
    "p1_memo_any",
    "p1_memo_dunder",
    "presize_no_free",
    // E1 (`recunroll`): up-recursion unrolling (docs/JIT_TRACE.md section 34)
    "h9_inl_push_skip",
    "h9_inl_savedpc_off",
    "h9_inl_bump_off",
    "h9_maxci_short",
    "h9_fallback_never",
    "h9_fail_penalised",
    "h9_demote_ignored",
    "h9_code_rule_never",
    "h9_doom_never",
    "h9_shape_any",
    // E2 (`idfold`): region-scoped callee identity (docs/JIT_TRACE.md section 34)
    "idfold_outer_unchecked",
    "idfold_rootret_inner",
    "idfold_ustore_family",
    "idfold_push_proto",
    // E4 (`lazyloop`): lazy frames for recursion families with loops (docs/JIT_TRACE.md section 35)
    "lazyloop_retf_ok",
    "lazyloop_decide_any",
    "lazyloop_root_not_set",
    // E5 (`midfold`): region-scoped method identity (docs/JIT_TRACE.md section 35)
    "midfold_outer_unchecked",
    "midfold_rootret_inner",
    "midfold_store_family",
    "midfold_recv_any",
    "midfold_push_proto",
    "midfold_reg0_any",
    "midfold_check_true",
    "midfold_first_keep",
    // docs/JIT_TRACE.md section 36.1: the entry failure of a side trace inside a lazy region
    "lazy_sidefail_no_leave",
    // E6 (`dlink`): direct exit-to-side links, the budget register across native transfers (docs/JIT_TRACE.md section 36)
    "dlink_refund_off",
    "dlink_store_skip",
    "dlink_unlink_skip",
    "dlink_vb_fb_nostore",
    // T4 (`t4`): the guards of a native return at the end of the trace (docs/JIT_TRACE.md section 46)
    "t4_late_start_snap",
    // T5 (`t5`): object keys on trace (docs/JIT_TRACE.md section 48)
    "hrefo_lsize_skip",
    "hrefo_hash_64",
    "hrefo_nil_meta_skip",
    "hrefo_abs_meta_skip",
    "hrefo_abs_lsize_skip",
    "tset_key_tag_int",
    // T7 (`t7`): leaf-function roots (docs/JIT_TRACE.md section 48)
    "t7_fnmin_kept",
    "t7_leaf_any",
    // T9 (`innerlink`): nested-loop trace linking (docs/JIT_TRACE.md section 47)
    "innerlink_prep_never",
    "innerret_never",
    "innerret_no_wake",
    // E9 (`t1call`): the tier-1 call protocol (docs/JIT_TRACE.md section 37)
    "t1_jret_site_next",
    "t1_jret_ignore_mark",
    "t1_jret_no_setk",
    // docs/JIT_TRACE.md section 38 (`deepprobe`): a deep site picks the longest of its probe iterations
    "deepprobe_never",
    "deepprobe_any_try",
    "deepprobe_uniform_keep",
    "deepprobe_abort_keeps",
    // docs/JIT_TRACE.md section 39.1 (T1, `retfsel`): RETF for function-entry families where the caller's loop is traced and no recursion is popped
    "retfsel_never",
    "retfsel_untraced_ok",
    "retfsel_recursion_ok",
    "retfsel_outer_loop",
    // docs/JIT_TRACE.md section 44.1 (T6, `ctor`): the fused constructor of tier 1
    "ctor_nil_value",
    "ctor_flags_skip",
    "ctor_layout_skip",
    "ctor_mainpos_off",
    "ctor_collide",
    "ctor_any_reg",
    // docs/JIT_TRACE.md section 44.1 (T6 bit 1): mixed integer / float `math.min` / `math.max` of tier 1
    "t6_mm_big_skip",
    "t6_mm_conv_skip",
    // docs/JIT_TRACE.md section 44.2 (T8, `libcall` bit 10): `string.find` with a pattern on trace (`LIB FINDP`)
    "findp_init_skip",
    "findp_caps_skip",
    "findp_end_off",
    "findp_outcome_skip",
    // docs/JIT_TRACE.md section 44.2 (T8, `libcall` bit 11): `string.match` on trace (`LIB FINDM` + `SUB`)
    "findm_span_off",
    "findm_caps_skip",
    "findm_outcome_skip",
    // docs/JIT_TRACE.md section 44.3 (`blfirst`): blacklist at the first unchangeable abort
    "blfirst_never",
    "blfirst_side_always",
    // docs/JIT_TRACE.md section 43.1 (TR24c, `restarts`): in-place `Unstable` restarts of a root recording
    "restarts_never",
    "restarts_unbounded",
    // docs/JIT_TRACE.md section 39.2 (T3, `heatside`): side traces beyond `maxside` for exits that keep coming back, within a state budget
    "heatside_never",
    "heatside_cap_ignored",
    "heatside_budget_ignored",
    "heatwait_ignored",
    "heatdepth_ignored",
    "heatdefer_counts_la",
    // docs/JIT_TRACE.md section 41 (N1, the loop-body diet): `ixfuse` (constant index offset in the access displacement)
    "ixfuse_tag_disp0",
    "ixfuse_tag_disp_neg",
    "ixfuse_pay_disp_pos",
    // N1c (`cloop`): the counted-loop back-edge
    "cloop_cnt_imm2",
    "cloop_no_fixup_cnt",
    // docs/JIT_TRACE.md section 59 (`budhoist` / `pollk`): the hoisted loop budget
    "bh_chunk_end_bias_long",
    "bh_refund_skip",
    "bh_bias_skip",
    // docs/JIT_TRACE.md section 63 (`bhmin`): the per-loop policy of the hoisted budget
    "bhmin_ignored",
    // N1b (`nidx`): the carried negated index
    "nidx_disp_off1",
    "nidx_step_sign",
    "nidx_entry_off1",
    // N1d (`leaadd`)
    "leaadd_disp_off1",
    // docs/JIT_TRACE.md section 49 (C1, `cspeed`): the compile-speed levers emit the code of the old paths (`ccheck`)
    "cspeed_enc_disp32",
    "cspeed_ra_last_free",
    "cspeed_ra_unfuse_skip",
    "cspeed_sink_cand_miss",
    // docs/JIT_TRACE.md section 50 (`slink`): register-passing side links (the carry link entry)
    "slink_store_skip",
    "slink_tag_skip",
    "slink_refund_off",
    "slink_count_skip",
    // docs/JIT_TRACE.md section 51 (`rapol`): policy levers of the trace register allocator
    "callspill_no_reload",
    // docs/JIT_TRACE.md section 52 (`sinkmm`, `resink`): the sunk-frame stub for metamethod frames, sunk allocations replayed into side traces
    "sinkmm_unatt_inline",
    "resink_copy_skip",
    "resink_flag_ignored",
    "resink_rcell_skip",
    "resink_revert_slot_off",
    "resink_setmt_guard_any",
    "resink_handler_mat",
    // docs/JIT_TRACE.md section 53 (`cspeed` lever `FWDSNAP`): the side-trace forwarding pass's snapshot copy (`ccheck`)
    "fwdsnap_frame_stale",
    // docs/JIT_TRACE.md section 54 (R2, `t1g`): the tier-1 generic-`for` paths (`CLOSE` skip, lean `next`)
    "t1g_close_tbc_skip",
    "t1g_close_level_skip",
    "t1g_next_fn_skip",
    // docs/JIT_TRACE.md section 59 (`reroot`): a root on the minority direction of its loop is re-recorded once with a path constraint
    "reroot_never",
    // docs/JIT_TRACE.md section 62 (`rerootchk`): the probation check of a `reroot` re-record
    "rerootchk_keep_always",
    "rerootchk_revert_always",
    // docs/JIT_TRACE.md section 68 (`regcall`): `LIB` calls with register arguments (`Op::CallD`, the register entries)
    "regcall_args_rot",
    "regcall_arg2_skip",
    // docs/JIT_TRACE.md section 64 (`wgcall`): the call weight of the start gate's loop length is ignored
    "wgcall_weight_skip",
];

#[cfg(feature = "tr_mutants")]
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

/// Is mutant `name` active? `name` must be in [`NAMES`] (checked in debug builds).
#[cfg(feature = "tr_mutants")]
pub fn active(name: &str) -> bool {
    debug_assert!(NAMES.contains(&name), "unregistered mutant {name}");
    names().iter().any(|n| n == name)
}

/// Is mutant `name` active? (Never, without the feature `tr_mutants`.)
#[cfg(not(feature = "tr_mutants"))]
#[inline(always)]
pub const fn active(_name: &str) -> bool {
    false
}

/// Apply the mutants that live in rn-core (`hooked_head_no_trap`, cfg `jit_trace_mut`): tests call this once per
/// process before running states.
#[cfg(feature = "tr_mutants")]
pub fn apply_vm_mutants() {
    rn_core::jit_hooks::set_mut_hooked_head_no_trap(active("hooked_head_no_trap"));
    rn_core::jit_hooks::set_mut_leave_trap_kept(active("leave_trap_kept"));
    rn_core::table::set_mut_tbl_move_meta_skip(active("tbl_move_meta_skip"));
    rn_core::table::set_mut_tlib(
        u8::from(active("tmove_presize_holes"))
            | u8::from(active("tpop_meta_skip")) << 1
            | u8::from(active("gmplus_set_neg_skip")) << 2,
    );
    rn_core::table::set_mut_smallins(u8::from(active("smallins_order_swap")));
    rn_core::table::set_mut_presize(u8::from(active("presize_no_free")));
    rn_core::table::set_mut_shape(
        u8::from(active("shape_stamp_not_bumped_on_newkey"))
            | u8::from(active("shape_collide_move_no_clear")) << 1
            | u8::from(active("shape_gc_deadkey_reuse")) << 2
            | u8::from(active("shape_weak_clear_skip")) << 3,
    );
    rn_core::jit_helpers::set_mut_c2_barrier_skip(active("c2setupval_barrier_skip"));
    rn_core::jit_helpers::set_mut_mm_ret_nc_skip(active("mmret_nc_skip"));
    rn_core::jit_helpers::set_mut_mm_ret_flag_false(active("mmret_flag_false"));
    rn_core::jit_helpers::set_mut_mm_ret_arith_dst(active("armm_dst_off"));
    rn_core::string::set_mut_promote_skip(active("promote_skip"));
    rn_core::string::set_mut_strtab_idx_raw(active("strtab_idx_raw"));
    rn_core::string::set_mut_chain_cmp_hash_only(active("chain_cmp_hash_only"));
    rn_core::string::set_mut_record_mainpos_raw(active("record_mainpos_raw"));
    rn_core::string::set_mut_promote_seed0(active("promote_seed0"));
    rn_core::string::set_mut_strtag_zero(active("strtag_zero"));
    rn_core::string::set_mut_strtag_shift(active("strtag_shift"));
    rn_core::string::set_mut_strtag_wide(active("strtag_wide"));
    rn_core::string::set_mut_bytetail_skip(active("bytetail_hi_byte_skip"));
    rn_core::coro::set_mut_cofuse(
        u32::from(active("cofuse_room_skip"))
            | u32::from(active("cofuse_hook_ignored")) << 1
            | u32::from(active("cofuse_ncalls_skip")) << 2
            | u32::from(active("cofuse_finish_op_skip")) << 3
            | u32::from(active("cofuse_wanted_nil_skip")) << 4
            | u32::from(active("cofuse_results_room_skip")) << 5
            | u32::from(active("cofuse_prev_c_ignored")) << 6
            | u32::from(active("coyield_yieldable_skip")) << 7
            | u32::from(active("coyield_nargs_wrong")) << 8
            | u32::from(active("coyield_status_skip")) << 9
            | u32::from(active("coinl_err_as_ok")) << 10
            | u32::from(active("codirect_hook_ignored")) << 11
            | u32::from(active("codirect_true_skip")) << 12
            | u32::from(active("codirect_nil_pad_skip")) << 13
            | u32::from(active("codirect_rest_flag")) << 14
            | u32::from(active("costart_func_off")) << 15,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_the_frozen_list() {
        assert_eq!(NAMES.len(), 899);
        let mut v = NAMES.to_vec();
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), NAMES.len(), "duplicate name");
        assert!(NAMES.iter().all(|n| n
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')));
        assert!(!active("snap_drop_last") || cfg!(feature = "tr_mutants"));
    }
}
