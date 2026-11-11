// Appended (include!) to the harness sliced from tests/tr_asm.rs: the entry point of the `trace_ir` fuzz target.
//
// Input: seed (8 bytes), c, c2, c3 (3 bytes). `random_case` (the P2 generator of tr::interp) builds a well-typed trace
// and models from the seed; the stages below are the ones the test suites run on the same generator:
//   1. fold/CSE (`tr::fold::optimize`) against the interpreter on every model (tr_passes `differential`);
//   2. forwarding / peeling (`tr::opt_loop::run` on the rolled trace) against the interpreter (tr_passes
//      `forwarding_preserves_the_interpreter_result`);
//   3. register allocation + exits + assembly (`diff_case`): the assembled micro-ops in the emulator (inline and generic
//      stubs) or natively, against the interpreter, with a random allocation, `+exitstorm`, and the budget in a register
//      or in memory (tr_asm `emu_*` / `native_*`).
// Contract: the generator's IR is valid by construction; an assertion in any of these is a finding. Cases the
// harness cannot model (a peeled trace that does not exit on some model within the iteration cap) are skipped.
//
// c:  bits 0-4 size (6 + 2 * v), bit 5 loops (peeled-with-PHIs generator output), bit 6 budget in a register
// c2: bit 0 fold, bit 1 cse, bit 2 run opt_loop (rolled only), bit 3 loop_opt, bit 4 fwd,
//     bits 5-7 backend: 0 emu, 1 emu generic stubs, 2 native, 3 emu + storm 1, 4 generic + storm 5, 5 native + storm 3,
//     6 emu, 7 generic
// c3: bit 0 random allocation (seed from c3 and the case seed)

pub fn run_case(data: &[u8]) {
    if data.len() < 11 {
        return;
    }
    let seed = u64::from_le_bytes(data[..8].try_into().unwrap());
    let (c, c2, c3) = (data[8], data[9], data[10]);
    let size = 6 + 2 * usize::from(c & 31);
    let loops = c & 0x20 != 0;
    let vb = (c & 0x40 != 0).then_some(ra::BUDGET_GPR);
    VB_REG.with(|v| v.set(vb));
    let (fold_on, cse_on) = (c2 & 1 != 0, c2 & 2 != 0);

    let mut rng = Rng::new(seed);
    let case = random_case(&mut rng, GenCfg { size, loops });

    // ---- 1. fold / cse
    let ir1 = if fold_on || cse_on {
        match rn_jit::tr::fold::optimize(&case.ir, fold_on, cse_on) {
            Ok(o) => o,
            Err(IrAbort::ConstGuard) => case.ir.clone(),
            Err(e) => panic!("seed {seed}: optimize: {e:?}"),
        }
    } else {
        case.ir.clone()
    };
    if fold_on || cse_on {
        for (mi, m0) in case.models.iter().enumerate() {
            let (mut ma, mut mb) = (m0.clone(), m0.clone());
            let ra_ = interp::run(&case.ir, &mut ma).unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));
            let rb_ = interp::run(&ir1, &mut mb).unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));
            assert_eq!(
                ra_,
                rb_,
                "seed {seed} fold={fold_on} cse={cse_on} model {mi}\n{}\n=>\n{}",
                rn_jit::tr::print::dump(&case.ir),
                rn_jit::tr::print::dump(&ir1)
            );
            assert_eq!(ma, mb, "seed {seed} model {mi}: final state after fold/cse");
        }
    }

    // ---- 2. forwarding / peeling of the rolled trace
    let mut ir2 = ir1;
    if c2 & 4 != 0 && ir2.loop_at.is_none() {
        use rn_jit::tr::record::Recorded;
        let mut ir = ir2.clone();
        // every snapshot's `pc` unique: exits can be told apart after the pass renumbers snapshots
        for (i, s) in ir.snaps.iter_mut().enumerate() {
            s.pc = i as u32;
        }
        let nsn = ir.snaps.len();
        let rec = Recorded {
            ir,
            writeback: Vec::new(),
            path: vec![0],
            cost: vec![1],
            snap_step: vec![0; nsn],
            path_proto: vec![ProtoKey(0)],
            path_depth: vec![0],
            room: 0,
            entry_snap: None,
            n: 1,
            rerec: 0,
            side: None,
            stitched: false,
        };
        let mut opt = rec.clone();
        let o = rn_jit::tr::TraceOpts {
            loop_opt: c2 & 8 != 0,
            fwd: c2 & 16 != 0,
            ..rn_jit::tr::TraceOpts::default()
        };
        rn_jit::tr::opt_loop::run(&mut opt, &o).unwrap_or_else(|e| panic!("seed {seed}: opt_loop: {e}"));
        rn_jit::tr::opt_loop::check(&opt.ir).unwrap_or_else(|e| {
            panic!(
                "seed {seed}: opt_loop::check: {e}\n{}",
                rn_jit::tr::print::dump(&opt.ir)
            )
        });
        for (mi, m0) in case.models.iter().enumerate() {
            let (mut ma, mut mb) = (m0.clone(), m0.clone());
            mb.max_iters = 20_000;
            ma.max_iters = 20_000;
            let r_a = interp::run(&rec.ir, &mut ma).unwrap();
            let r_b = interp::run(&opt.ir, &mut mb).unwrap();
            let key = |ir: &TraceIr, o: &Outcome| match o {
                Outcome::Done { iters } => (u32::MAX, vec![], *iters),
                Outcome::Exit { snap, slots } => (ir.snaps[*snap as usize].pc, slots.clone(), 0),
            };
            if opt.ir.loop_at.is_some() {
                // a peeled trace counts back-edges differently from the rolled one: compare exits only
                if let (Outcome::Exit { .. }, Outcome::Exit { .. }) = (&r_a, &r_b) {
                    assert_eq!(
                        key(&rec.ir, &r_a).0,
                        key(&opt.ir, &r_b).0,
                        "seed {seed} model {mi} (peeled)\n{}\n=>\n{}",
                        rn_jit::tr::print::dump(&rec.ir),
                        rn_jit::tr::print::dump(&opt.ir)
                    );
                }
            } else {
                assert_eq!(
                    key(&rec.ir, &r_a),
                    key(&opt.ir, &r_b),
                    "seed {seed} model {mi}\n{}\n=>\n{}",
                    rn_jit::tr::print::dump(&rec.ir),
                    rn_jit::tr::print::dump(&opt.ir)
                );
                assert_eq!(ma, mb, "seed {seed} model {mi}: final state after opt_loop");
            }
        }
        ir2 = opt.ir;
    }

    // ---- 3. RA + exits + asm vs the interpreter
    if ir2.loop_at.is_some() {
        // `reference` needs every model to leave through a guard; skip traces that run to the iteration cap
        for m0 in &case.models {
            let mut m = m0.clone();
            m.max_iters = 100_000;
            if !matches!(interp::run(&ir2, &mut m), Ok(Outcome::Exit { .. })) {
                return;
            }
        }
    }
    let ra_seed = (c3 & 1 != 0).then(|| seed ^ (u64::from(c3) << 8));
    let (backend, storm) = match c2 >> 5 {
        0 | 6 => (Backend::Emu, None),
        1 | 7 => (Backend::EmuGeneric, None),
        2 => (Backend::Native, None),
        3 => (Backend::Emu, Some(1)),
        4 => (Backend::EmuGeneric, Some(5)),
        _ => (Backend::Native, Some(3)),
    };
    if storm.is_some() && storm_guards(&ir2) == 0 {
        return;
    }
    diff_case(&ir2, &case.models, seed, ra_seed, backend, storm);
}
