//! J3c: hosts that deny or limit executable memory, and the code cap (eviction policy).
//!
//! * `PR_SET_MDWE`, a seccomp filter that refuses `mprotect`/`mmap` with `PROT_EXEC`, and an address-space limit
//!   that makes `mmap` fail run in a child process (this test binary re-executed with `RUSTYNAIL_J3C_CHILD`):
//!   the JIT must disable itself silently and the program must give the interpreter's result.
//! * The code cap: when it is hit further compiles are refused (transient, the function stays interpreted),
//!   freed code space is reused once prototypes were collected, and mapped bytes return to the baseline.
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)));
    });
}

fn run_in(st: &mut LuaState, src: &str) -> Result<String, String> {
    let base = st.top();
    assert_eq!(st.load_buffer(src.as_bytes(), b"=env", None), Status::Ok);
    let func = st.top() - 1;
    let s = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let text = match (v.as_string(), v.as_number()) {
        (Some(s), _) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
        (None, Some(n)) => {
            String::from_utf8_lossy(rn_core::number::tostring(n).as_bytes()).into_owned()
        }
        _ => format!("{v:?}"),
    };
    st.set_top(base);
    if s == Status::Ok {
        Ok(text)
    } else {
        Err(text)
    }
}

const PROG: &str = "
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local t = {}
for i = 1, 300 do t[i] = load('local a = ... return function(x) return x * a + 1 end')(i) end
local s = 0
for r = 1, 4 do for i = 1, #t do s = s + t[i](r) end end
local co = coroutine.wrap(function() for i = 1, 100 do coroutine.yield(fib(10) + i) end end)
for i = 1, 100 do s = s + co() end
return s + fib(20)";

// ---------------------------------------------------------------------------------------------------------
// Child processes
// ---------------------------------------------------------------------------------------------------------

const CHILD_ENV: &str = "RUSTYNAIL_J3C_CHILD";

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn mdwe() -> Result<(), String> {
    const PR_SET_MDWE: i32 = 65;
    const PR_MDWE_REFUSE_EXEC_GAIN: u64 = 1;
    // SAFETY: prctl with integer arguments.
    let rc = unsafe { libc::prctl(PR_SET_MDWE, PR_MDWE_REFUSE_EXEC_GAIN, 0u64, 0u64, 0u64) };
    if rc != 0 {
        return Err(format!("PR_SET_MDWE unsupported (errno {})", errno()));
    }
    Ok(())
}

/// Installs a seccomp filter that fails `mprotect`, `pkey_mprotect` and `mmap` with `PROT_EXEC` (EPERM).
fn seccomp_deny_exec() -> Result<(), String> {
    use libc::{sock_filter, sock_fprog};
    const BPF_LD_W_ABS: u16 = 0x20;
    const BPF_JMP_JEQ_K: u16 = 0x15;
    const BPF_JMP_JSET_K: u16 = 0x45;
    const BPF_RET_K: u16 = 0x06;
    const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
    const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
    const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;
    const PROT_EXEC: u32 = 4;
    let stmt = |code: u16, k: u32| sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    };
    let jump = |code: u16, k: u32, jt: u8, jf: u8| sock_filter { code, jt, jf, k };
    // seccomp_data: nr at 0, arch at 4, args[i] (u64) at 16 + 8 * i; the low word of args[2] is `prot`.
    let mut prog = vec![
        stmt(BPF_LD_W_ABS, 4),
        jump(BPF_JMP_JEQ_K, AUDIT_ARCH_X86_64, 1, 0),
        stmt(BPF_RET_K, SECCOMP_RET_ALLOW),
        stmt(BPF_LD_W_ABS, 0),
    ];
    for nr in [libc::SYS_mprotect, libc::SYS_mmap, libc::SYS_pkey_mprotect] {
        // if nr == syscall: check prot, else skip the 3 instructions
        prog.push(jump(BPF_JMP_JEQ_K, nr as u32, 0, 4));
        prog.push(stmt(BPF_LD_W_ABS, 16 + 8 * 2));
        prog.push(jump(BPF_JMP_JSET_K, PROT_EXEC, 0, 1));
        prog.push(stmt(BPF_RET_K, SECCOMP_RET_ERRNO | libc::EPERM as u32));
        prog.push(stmt(BPF_LD_W_ABS, 0));
    }
    prog.push(stmt(BPF_RET_K, SECCOMP_RET_ALLOW));
    let fprog = sock_fprog {
        len: prog.len() as u16,
        filter: prog.as_mut_ptr(),
    };
    // SAFETY: prctl/seccomp with a valid filter program that outlives the call.
    unsafe {
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            return Err(format!("NO_NEW_PRIVS failed ({})", errno()));
        }
        if libc::prctl(
            libc::PR_SET_SECCOMP,
            2, /* SECCOMP_MODE_FILTER */
            &fprog as *const sock_fprog,
        ) != 0
        {
            return Err(format!("seccomp filter failed ({})", errno()));
        }
    }
    Ok(())
}

/// Lowers `RLIMIT_AS` to the current address-space size plus `headroom` bytes.
fn limit_address_space(headroom: u64) -> Result<(), String> {
    let statm = std::fs::read_to_string("/proc/self/statm").map_err(|e| e.to_string())?;
    let pages: u64 = statm
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or("statm")?;
    let cur = pages * 4096 + headroom;
    let lim = libc::rlimit {
        rlim_cur: cur,
        rlim_max: cur,
    };
    // SAFETY: setrlimit with a valid struct.
    if unsafe { libc::setrlimit(libc::RLIMIT_AS, &lim) } != 0 {
        return Err(format!("setrlimit failed ({})", errno()));
    }
    Ok(())
}

/// Child body: `kind` selects the restriction; prints `J3C-OK ...` and exits 0, or `J3C-SKIP` / exit 77.
fn child(kind: &str) -> ! {
    init();
    if kind == "seccomp_mid" {
        // code is already compiled and packed into a partly used page when exec mappings become impossible
        assert!(rn_jit::available());
        let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
        rn_jit::set_mode(&mut st, Mode::Always);
        run_in(&mut st, PROG).expect("first run");
        assert!(rn_jit::stats(&mut st).compiled > 0);
        if let Err(e) = seccomp_deny_exec() {
            println!("J3C-SKIP {e}");
            std::process::exit(77);
        }
        let r = run_in(
            &mut st,
            "local function g(x) return x * 3 end for i = 1, 5 do g(i) end return g(2)",
        );
        println!("J3C-MID-SURVIVED {r:?}");
        std::process::exit(0);
    }
    let reference = || {
        let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
        rn_jit::set_mode(&mut st, Mode::Off);
        run_in(&mut st, PROG).expect("reference run")
    };
    // MDWE / seccomp must be in force before the process first probes executable memory; the other two
    // restrict a process whose probe already passed (and whose reference run is done).
    let early = matches!(kind, "mdwe" | "seccomp");
    let mut expect = None;
    if !early {
        assert!(
            rn_jit::available(),
            "the probe must pass before the restriction"
        );
        expect = Some(reference());
    }
    let restricted = match kind {
        "mdwe" => mdwe(),
        "seccomp" | "seccomp_late" => seccomp_deny_exec(),
        // mmap of the first code chunk (1 MiB) fails; the probe's and thunk's pages are small
        "rlimit" => limit_address_space(256 * 1024),
        _ => panic!("unknown kind {kind}"),
    };
    if let Err(e) = restricted {
        println!("J3C-SKIP {e}");
        std::process::exit(77);
    }
    let expect = expect.unwrap_or_else(reference);
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, Mode::Always);
    let got = run_in(&mut st, PROG);
    let s = rn_jit::stats(&mut st);
    let avail = rn_jit::available();
    match got {
        Ok(v) if v == expect && s.enters == 0 => {
            println!("J3C-OK {kind} available={avail} {s:?}");
            std::process::exit(0)
        }
        other => {
            println!("J3C-BAD {kind} expect={expect:?} got={other:?} available={avail} {s:?}");
            std::process::exit(1)
        }
    }
}

#[test]
fn child_entry() {
    if let Ok(kind) = std::env::var(CHILD_ENV) {
        child(&kind);
    }
}

fn spawn_child(kind: &str) -> Option<String> {
    let exe = std::env::current_exe().unwrap();
    let out = std::process::Command::new(exe)
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, kind)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    match out.status.code() {
        Some(0) => {
            assert!(text.contains("J3C-OK"), "{text}");
            Some(text)
        }
        Some(77) => {
            eprintln!("SKIP {kind}: {text}");
            None
        }
        c => panic!(
            "child {kind} failed: {c:?}\n{text}\n{}",
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

#[test]
fn mdwe_falls_back_to_the_interpreter_with_identical_results() {
    if let Some(t) = spawn_child("mdwe") {
        assert!(t.contains("available=false"), "{t}");
    }
}

#[test]
fn seccomp_denying_exec_mappings_falls_back_to_the_interpreter() {
    if let Some(t) = spawn_child("seccomp") {
        assert!(t.contains("available=false"), "{t}");
    }
}

#[test]
fn seccomp_installed_after_the_probe_refuses_every_compile_cleanly() {
    // probe passed, then every PROT_EXEC mprotect fails: compiles fail permanently, nothing crashes
    spawn_child("seccomp_late");
}

/// Known gap (docs/JIT.md): a filter that appears after code was compiled makes the next compile abort the
/// process (it cannot seal the page it re-opened); this pins the behaviour so that a change is noticed.
#[test]
fn seccomp_installed_after_code_was_compiled_aborts_or_survives_cleanly() {
    use std::os::unix::process::ExitStatusExt;
    let exe = std::env::current_exe().unwrap();
    let out = std::process::Command::new(exe)
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "seccomp_mid")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    match (out.status.code(), out.status.signal()) {
        (Some(77), _) => eprintln!("SKIP: {text}"),
        (Some(0), _) => assert!(text.contains("J3C-MID-SURVIVED Ok(\"6\")"), "{text}\n{err}"),
        (_, Some(6)) => assert!(err.contains("cannot seal a code page"), "{text}\n{err}"),
        other => panic!("unexpected end {other:?}\n{text}\n{err}"),
    }
}

#[test]
fn failing_mmap_of_code_chunks_falls_back_silently() {
    spawn_child("rlimit");
}

// ---------------------------------------------------------------------------------------------------------
// Code cap
// ---------------------------------------------------------------------------------------------------------

/// Generated hot functions: `n` distinct chunks, each compiled, run a few times and dropped; `keep` of every 10
/// are retained. Returns a checksum.
fn gen_prog(n: usize, keep_every: usize, collect_every: usize) -> String {
    format!(
        "local keep, s = {{}}, 0
         for i = 1, {n} do
           local f = load('local a = ... return function(x) local r = 0 for k = 1, x do r = r + k * a end return r + ' .. i .. ' end')(i)
           s = s + f(3) + f(2)
           if {keep_every} > 0 and i % {keep_every} == 0 then keep[#keep + 1] = f end
           if i % {collect_every} == 0 then collectgarbage() end
         end
         for _, f in ipairs(keep) do s = s + f(1) end
         return s .. ':' .. #keep"
    )
}

fn off_result(src: &str) -> String {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, Mode::Off);
    run_in(&mut st, src).expect("reference")
}

#[test]
fn code_cap_hit_refuses_compiles_and_reuses_freed_space() {
    init();
    let src = gen_prog(3000, 0, 40);
    let expect = off_result(&src);
    for cap_kb in [64usize, 200, 1100] {
        let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
        rn_jit::set_mode(&mut st, Mode::Always);
        // establish the arena (thunk), then cap what is mapped next to it
        run_in(
            &mut st,
            "local function w(x) return x + 1 end for i = 1, 3 do w(i) end",
        )
        .unwrap();
        let base = rn_jit::stats(&mut st).mapped as usize;
        let cap = base + cap_kb * 1024;
        rn_jit::set_code_cap(&mut st, cap);
        let got = run_in(&mut st, &src).unwrap();
        assert_eq!(got, expect, "cap {cap_kb} KiB");
        let s = rn_jit::stats(&mut st);
        assert!(
            s.mapped as usize <= cap,
            "cap {cap}: mapped {} {s:?}",
            s.mapped
        );
        // freed space is reused: far more code was generated than the cap holds
        assert!(
            s.code_bytes as usize > 2 * cap_kb * 1024,
            "cap {cap_kb}: only {} code bytes generated, nothing was reused: {s:?}",
            s.code_bytes
        );
        assert!(s.compiled > 500, "cap {cap_kb}: {s:?}");
        // after collecting everything the arena holds the baseline plus at most what a few still-live blocks
        // pin: chunks are only unmapped (or pooled) when completely empty, so one live prototype keeps a chunk
        run_in(&mut st, "collectgarbage() collectgarbage()").unwrap();
        let end = rn_jit::stats(&mut st);
        assert!(end.live < 40, "{end:?}");
        assert!(
            end.mapped as usize <= (base + 2 * 1024 * 1024).min(cap),
            "cap {cap_kb}: mapped {} did not return to the baseline {base}: {end:?}",
            end.mapped
        );
    }
}

#[test]
fn code_cap_with_retained_functions_refuses_but_stays_correct() {
    init();
    // every 3rd function stays alive: the cap is eventually full of live code, later compiles are refused
    let src = gen_prog(1500, 3, 50);
    let expect = off_result(&src);
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, Mode::Always);
    run_in(
        &mut st,
        "local function w(x) return x + 1 end for i = 1, 3 do w(i) end",
    )
    .unwrap();
    let base = rn_jit::stats(&mut st).mapped as usize;
    let cap = base + 64 * 1024;
    rn_jit::set_code_cap(&mut st, cap);
    let got = run_in(&mut st, &src).unwrap();
    assert_eq!(got, expect);
    let s = rn_jit::stats(&mut st);
    assert!(s.mapped as usize <= cap, "{s:?}");
    assert!(s.refused > 0, "the cap was never hit: {s:?}");
    // the held functions are still runnable (interpreted or compiled) and the cap recovers after they die
    run_in(&mut st, "collectgarbage() collectgarbage()").unwrap();
    let before = rn_jit::stats(&mut st);
    let again = run_in(&mut st, &gen_prog(200, 0, 20)).unwrap();
    assert_eq!(again, off_result(&gen_prog(200, 0, 20)));
    let after = rn_jit::stats(&mut st);
    // (with RUSTYNAIL_JIT_T15 on every promoted function keeps a retired block next to the current one, so the
    // same cap holds half as many functions between two collections)
    let t15 = std::env::var("RUSTYNAIL_JIT_T15").is_ok_and(|v| v != "off" && v != "0");
    let need = if t15 { 40 } else { 100 };
    assert!(
        after.compiled > before.compiled + need,
        "compiling did not resume after code was freed: {before:?} {after:?}"
    );
}

#[test]
fn repeated_rounds_of_generated_functions_do_not_grow_the_arena() {
    init();
    let src = gen_prog(1500, 0, 100);
    let expect = off_result(&src);
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, Mode::Always);
    let mut mapped = Vec::new();
    let mut mem = Vec::new();
    for _ in 0..5 {
        assert_eq!(run_in(&mut st, &src).unwrap(), expect);
        run_in(&mut st, "collectgarbage() collectgarbage()").unwrap();
        mapped.push(rn_jit::stats(&mut st).mapped);
        mem.push(st.heap().mem_mut().ext_bytes());
    }
    let s = rn_jit::stats(&mut st);
    assert!(s.freed + 40 >= s.compiled, "code not freed: {s:?}");
    assert!(mapped[4] <= mapped[1], "arena grows: {mapped:?}");
    assert!(mem[4] <= mem[1] + 16 * 1024, "charged bytes grow: {mem:?}");
}

#[test]
fn code_referenced_only_by_suspended_coroutines_lives_and_dies_with_them() {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, Mode::Always);
    let make = "held = {}
        for i = 1, 80 do
          local f = load('return function(n) local s = 0 for k = 1, n do s = s + k coroutine.yield(s) end return s + ' .. i .. ' end')()
          local co = coroutine.create(f)
          coroutine.resume(co, 5)
          held[i] = co
          f = nil
        end
        collectgarbage() collectgarbage()";
    run_in(&mut st, make).unwrap();
    let held = rn_jit::stats(&mut st);
    assert!(
        held.live >= 80,
        "prototypes referenced by suspended coroutines were freed: {held:?}"
    );
    // they still run (the code is alive), to the end
    let r = run_in(
        &mut st,
        "local t = 0 for i = 1, 80 do for r = 1, 5 do local ok, v = coroutine.resume(held[i]) t = t + (v or 0) end end return t",
    )
    .unwrap();
    // per coroutine: yields 3, 6, 10, 15, then returns 15 + i
    assert_eq!(r, (80 * 49 + 80 * 81 / 2).to_string());
    run_in(&mut st, "held = nil collectgarbage() collectgarbage()").unwrap();
    let end = rn_jit::stats(&mut st);
    assert!(
        end.live < 20,
        "code of dropped coroutines was not freed: {end:?}"
    );
    assert!(end.freed >= held.live - 20, "{held:?} {end:?}");
}

#[test]
fn zero_cap_disables_compiling_without_errors() {
    init();
    let src = gen_prog(200, 0, 50);
    let expect = off_result(&src);
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, Mode::Always);
    rn_jit::set_code_cap(&mut st, 0);
    assert_eq!(run_in(&mut st, &src).unwrap(), expect);
    let s = rn_jit::stats(&mut st);
    assert_eq!((s.compiled, s.enters), (0, 0), "{s:?}");
}
