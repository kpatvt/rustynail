//! Execution smoke tests: assemble tiny functions, map them executable and call them.

use super::*;

/// Copies `code` into a fresh RW mapping, flips it to RX, passes the entry address to `f`, unmaps.
fn with_code<R>(code: &[u8], f: impl FnOnce(*const u8) -> R) -> R {
    assert!(code.len() <= 4096);
    // SAFETY: anonymous private mapping request; the result is checked against MAP_FAILED.
    let p = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            4096,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    assert_ne!(p, libc::MAP_FAILED, "mmap failed");
    // SAFETY: `p` is a writable 4096-byte mapping and `code` is at most 4096 bytes; regions are disjoint.
    unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), p.cast::<u8>(), code.len()) };
    // SAFETY: `p` is a valid page-aligned 4096-byte mapping owned by this function.
    let rc = unsafe { libc::mprotect(p, 4096, libc::PROT_READ | libc::PROT_EXEC) };
    assert_eq!(rc, 0, "mprotect failed");
    let r = f(p.cast::<u8>());
    // SAFETY: unmapping the mapping created above; no references into it outlive this call.
    let rc = unsafe { libc::munmap(p, 4096) };
    assert_eq!(rc, 0);
    r
}

#[test]
fn exec_integer_functions() {
    // rdi*3+1 via lea
    let mut a = Assembler::new();
    a.lea(Size::S64, RAX, Mem::sib(RDI, RDI, 2, 0));
    a.alu_ri(Alu::Add, Size::S64, RAX, 1);
    a.ret();
    let code = a.finish().unwrap();
    with_code(&code, |p| {
        // SAFETY: `p` points at assembled code implementing `extern "C" fn(i64) -> i64`.
        let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(p) };
        for x in [0i64, 1, 5, -7, 1 << 40] {
            assert_eq!(f(x), x * 3 + 1);
        }
    });

    // rdi*3+1 via imul
    let mut a = Assembler::new();
    a.imul_rri(Size::S64, RAX, RDI, 3);
    a.alu_ri(Alu::Add, Size::S64, RAX, 1);
    a.ret();
    let code = a.finish().unwrap();
    with_code(&code, |p| {
        // SAFETY: as above.
        let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(p) };
        assert_eq!(f(14), 43);
        assert_eq!(f(-2), -5);
    });

    // sum 1..=n with a backward jcc, using push/pop, rcx loop counter and div by 2 via idiv
    let mut a = Assembler::new();
    let top = a.new_label();
    a.push(RBX);
    a.alu_rr(Alu::Xor, Size::S32, RAX, RAX);
    a.mov_rr(Size::S64, RCX, RDI);
    a.bind(top);
    a.alu_rr(Alu::Add, Size::S64, RAX, RCX);
    a.alu_ri(Alu::Sub, Size::S64, RCX, 1);
    a.jcc_label(Cond::Ne, top);
    a.mov_ri(RBX, 2);
    a.cqo();
    a.idiv(Size::S64, RBX); // rax = sum / 2
    a.pop(RBX);
    a.ret();
    let code = a.finish().unwrap();
    with_code(&code, |p| {
        // SAFETY: as above.
        let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(p) };
        assert_eq!(f(10), 55 / 2);
        assert_eq!(f(100), 5050 / 2);
    });

    // forward jump + setcc + cmov + movabs + memory round trip on the stack
    let mut a = Assembler::new();
    let skip = a.new_label();
    a.mov_ri(RAX, 0x1122_3344_5566_7788);
    a.alu_ri(Alu::Sub, Size::S64, RSP, 16);
    a.mov_mr(Size::S64, Mem::base(RSP), RAX);
    a.mov_m8_i(Mem::base_disp(RSP, 1), 0x7f);
    a.movzx_rm8(RAX, Mem::base_disp(RSP, 1));
    a.alu_rr(Alu::Cmp, Size::S64, RAX, RDI);
    a.setcc(Cond::E, RDX);
    a.movzx_rr8(RDX, RDX);
    a.jmp_label(skip);
    a.int3();
    a.bind(skip);
    a.alu_ri(Alu::Add, Size::S64, RSP, 16);
    a.mov_rr(Size::S64, RAX, RDX);
    a.ret();
    let code = a.finish().unwrap();
    with_code(&code, |p| {
        // SAFETY: as above.
        let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(p) };
        assert_eq!(f(0x7f), 1);
        assert_eq!(f(0x7e), 0);
    });
}

#[test]
fn exec_float_functions() {
    let mut a = Assembler::new();
    a.sse_rr(SseOp::Add, XMM0, XMM1);
    a.ret();
    let code = a.finish().unwrap();
    with_code(&code, |p| {
        // SAFETY: `p` points at assembled code implementing `extern "C" fn(f64, f64) -> f64`.
        let f: extern "C" fn(f64, f64) -> f64 = unsafe { std::mem::transmute(p) };
        assert_eq!(f(1.5, 2.25), 3.75);
    });

    // (double)rdi * xmm0 via cvtsi2sd, then sqrt, then truncating back via a movq round trip
    let mut a = Assembler::new();
    a.cvtsi2sd_rr(Size::S64, XMM1, RDI);
    a.sse_rr(SseOp::Mul, XMM0, XMM1);
    a.sse_rr(SseOp::Sqrt, XMM0, XMM0);
    a.movq_rx(RAX, XMM0);
    a.movq_xr(XMM2, RAX);
    a.movsd_rr(XMM0, XMM2);
    a.ret();
    let code = a.finish().unwrap();
    with_code(&code, |p| {
        // SAFETY: `p` points at assembled code implementing `extern "C" fn(f64, i64) -> f64`
        // (xmm0 = first float argument, rdi = first integer argument).
        let f: extern "C" fn(f64, i64) -> f64 = unsafe { std::mem::transmute(p) };
        assert_eq!(f(4.0, 9), 6.0);
    });

    // ucomisd + setcc, and cvttsd2si
    let mut a = Assembler::new();
    a.ucomisd_rr(XMM0, XMM1);
    a.setcc(Cond::A, RAX);
    a.movzx_rr8(RAX, RAX);
    a.ret();
    let code = a.finish().unwrap();
    with_code(&code, |p| {
        // SAFETY: `p` points at assembled code implementing `extern "C" fn(f64, f64) -> i64`.
        let f: extern "C" fn(f64, f64) -> i64 = unsafe { std::mem::transmute(p) };
        assert_eq!(f(2.0, 1.0), 1);
        assert_eq!(f(1.0, 2.0), 0);
    });
    let mut a = Assembler::new();
    a.cvttsd2si(Size::S64, RAX, XMM0);
    a.ret();
    let code = a.finish().unwrap();
    with_code(&code, |p| {
        // SAFETY: `p` points at assembled code implementing `extern "C" fn(f64) -> i64`.
        let f: extern "C" fn(f64) -> i64 = unsafe { std::mem::transmute(p) };
        assert_eq!(f(-7.9), -7);
    });
}

#[test]
fn exec_call_label_and_rip_relative() {
    // main: call helper; ret.  helper loads a constant stored after the code via [rip+label].
    let mut a = Assembler::new();
    let helper = a.new_label();
    let konst = a.new_label();
    a.alu_ri(Alu::Sub, Size::S64, RSP, 8);
    a.call_label(helper);
    a.alu_ri(Alu::Add, Size::S64, RSP, 8);
    a.ret();
    a.bind(helper);
    a.mov_rm(Size::S64, RAX, Mem::rip(konst));
    a.ret();
    a.align(8);
    a.bind(konst);
    a.emit_bytes(&0x0123_4567_89AB_CDEFu64.to_le_bytes());
    let code = a.finish().unwrap();
    with_code(&code, |p| {
        // SAFETY: `p` points at assembled code implementing `extern "C" fn() -> u64`.
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(p) };
        assert_eq!(f(), 0x0123_4567_89AB_CDEF);
    });
}
