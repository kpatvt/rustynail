//! Runs the `tests/vm` scripts through the real front end + interpreter in-process, so that Miri can
//! check the interpreter (`vm_unsafe.rs`), the call/stack machinery, the collector and the pool
//! allocator on real Lua programs (`scripts/miri.sh`; the subprocess-based differential tests cannot
//! run under Miri).
//!
//! Only FFI-free libraries are opened (`_G`, `coroutine`, `table`, `string`, `math`, `utf8`, `debug`;
//! no `io`/`os`/`package`, whose libc calls Miri cannot run) and `print` collects into a buffer.
//! Scripts that use the missing libraries or C `snprintf` formats are skipped. Natively the collected
//! output is also compared with the reference interpreter (`refrun`, hash seed 0).
//!
//! The scripts are split into shards (one test per shard and GC mode) so that `cargo miri nextest`
//! runs them in parallel. Modes: incremental, generational, and incremental on the pool allocator
//! (the default allocator outside Miri; `State::with_seed` uses the system allocator under Miri).
//! GC stress: run with `RUSTYNAIL_GC_STRESS=1|2` (under Miri also `-Zmiri-env-forward=RUSTYNAIL_GC_STRESS`).
mod common;
use common::*;
use rn_core::state::{LuaState, State};
use rn_core::status::Status;
use std::path::PathBuf;

const SHARDS: usize = 12;

/// Scripts not run under Miri, with the reason (Miri limitations or cost, not bugs).
const MIRI_SKIP: &[(&str, &str)] = &[
    (
        "err_15_select_and_misc_native_errors.lua",
        "`string.rep('x', 1 << 62)` expects 'not enough memory' from a failing malloc; Miri aborts \
         the whole run on such an allocation (resource exhaustion)",
    ),
    // Too slow under Miri (25M-900M native instructions each, i.e. hours): stack overflows to the
    // C-stack/`LUAI_MAXSTACK` limits and million-iteration loops. Covered natively and by ASan.
    ("err_09_error_in_error_handling.lua", "cost"),
    ("err_08_stack_overflow.lua", "cost"),
    ("heavy_tail_02_million.lua", "cost"),
    ("co_09_stack_and_args.lua", "cost"),
    ("for_09_nested_and_large.lua", "cost"),
    ("tail_01_basic.lua", "cost"),
    ("strings_04_pattern_start_filter.lua", "cost"),
];

/// Scripts that need `io`/`os`/`package`/`string.format` (libc) or rely on them indirectly.
fn usable(src: &str) -> bool {
    ![
        "string.format",
        ":format(",
        "io.",
        "os.",
        "require",
        "package",
        "%5",
        "loadfile",
        "dofile",
    ]
    .iter()
    .any(|p| src.contains(p))
}

fn scripts() -> Vec<PathBuf> {
    let dir = root().join("tests/vm");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "lua"))
        .filter(|p| std::fs::read_to_string(p).is_ok_and(|s| usable(&s)))
        .filter(|p| !(cfg!(miri) && MIRI_SKIP.iter().any(|(n, _)| p.ends_with(n))))
        .collect();
    v.sort();
    v
}

/// Lua prelude: a `print` that appends to `__out` (same formatting as the base `print`).
const PRELUDE: &str = r#"
local __out, concat, tostring, select = {}, table.concat, tostring, select
_G.__out = __out
function print(...)
  local n = select('#', ...)
  local t = {}
  for i = 1, n do t[i] = tostring((select(i, ...))) end
  __out[#__out + 1] = concat(t, "\t") .. "\n"
end
"#;

#[derive(Clone, Copy)]
enum Mode {
    Inc,
    Gen,
    Pool,
}

fn new_state(mode: Mode) -> LuaState {
    let mut st = match mode {
        Mode::Pool => {
            let mut st = State::new(Box::new(rn_core::alloc::PoolAllocator::new()), 0).unwrap();
            st.set_loader(rn_std::loader);
            st
        }
        _ => rn_std::new_state(0).unwrap(),
    };
    for (name, f) in rn_std::STD_LIBS {
        if [&b"io"[..], b"os", b"package"].contains(name) {
            continue;
        }
        st.requiref(name, *f, true).unwrap();
        st.pop();
    }
    st
}

fn run_chunk(st: &mut State, src: &[u8], name: &[u8]) -> Result<(), String> {
    let base = st.top();
    let status = st.load_chunk(src, name, None);
    let r = if status == Status::Ok {
        let f = st.top() - 1;
        match st.pcall(f, 0, 0) {
            Status::Ok => Ok(()),
            _ => Err(top_string(st)),
        }
    } else {
        Err(top_string(st))
    };
    st.set_top(base);
    r
}

fn top_string(st: &State) -> String {
    let v = st.get(st.top() - 1);
    v.as_string()
        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
        .unwrap_or_else(|| format!("{v:?}"))
}

/// Run one script; returns its collected output.
fn run_script(path: &PathBuf, mode: Mode) -> String {
    let mut st = new_state(mode);
    run_chunk(&mut st, PRELUDE.as_bytes(), b"=prelude").unwrap();
    if matches!(mode, Mode::Gen) {
        run_chunk(&mut st, b"collectgarbage('generational')", b"=mode").unwrap();
    }
    let src = std::fs::read(path).unwrap();
    let name = format!("@{}", path.file_name().unwrap().to_string_lossy());
    let res = run_chunk(&mut st, &src, name.as_bytes());
    run_chunk(&mut st, b"__RESULT = table.concat(__out)", b"=out").unwrap();
    let g = st.globals();
    let out = st.raw_get_str(g, b"__RESULT").unwrap();
    let mut text = out
        .as_string()
        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
        .unwrap_or_default();
    if let Err(e) = res {
        text.push_str(&format!("<error> {e}\n"));
    }
    text
}

/// `0x...` addresses differ between implementations.
#[cfg_attr(miri, allow(dead_code))]
fn normalize(s: &str) -> String {
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'0' && b.get(i + 1) == Some(&b'x') {
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_hexdigit() {
                j += 1;
            }
            if j - i > 6 {
                out.push_str("0xADDR");
                i = j;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

/// Reference output of `path` with the same prelude (native runs only).
#[cfg(not(miri))]
fn reference(path: &std::path::Path, mode: Mode) -> Option<String> {
    let bin = ref_lua(true)?;
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let mut prog = String::from(
        "local rd = io.open(arg[1], 'rb'); local src = rd:read('a'); rd:close()\nlocal w = io.write\n",
    );
    prog.push_str(PRELUDE);
    // hide the libraries ours does not open
    prog.push_str("io, os, package, require, dofile, loadfile = nil, nil, nil, nil, nil, nil\n");
    if matches!(mode, Mode::Gen) {
        prog.push_str("collectgarbage('generational')\n");
    }
    prog.push_str(&format!(
        "local f, e = load(src, '@{name}')\nlocal ok, err = false, e\nif f then ok, err = pcall(f) end\n\
         if not ok then __out[#__out + 1] = '<error> ' .. tostring(err) .. '\\n' end\nw(table.concat(__out))\n"
    ));
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let drv = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "interp-{}-{}.lua",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&drv, prog).ok()?;
    let out = std::process::Command::new(bin)
        .arg(&drv)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    let _ = std::fs::remove_file(&drv);
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run_shard(shard: usize, mode: Mode) {
    let all = scripts();
    assert!(all.len() > 80, "{} scripts", all.len());
    let mut n = 0;
    for (i, p) in all.iter().enumerate() {
        if i % SHARDS != shard {
            continue;
        }
        // scripts written for the other version (e.g. 5.5 syntax under the 5.4 cfg) are skipped
        let src = std::fs::read(p).unwrap();
        let mut host = rn_compiler::testhost::TestHost::default();
        if rn_compiler::load(&mut host, &src, b"=x", rn_compiler::LoadMode::BT).is_err() {
            continue;
        }
        // `INTERP_ONLY=<substring>`: only the scripts whose name contains it (a single script of a shard under Miri, where a whole
        // shard takes hours: `-Zmiri-env-forward=INTERP_ONLY`); progress lines show how far a killed shard got.
        if let Ok(o) = std::env::var("INTERP_ONLY") {
            if !p.to_string_lossy().contains(&o) {
                continue;
            }
        }
        eprintln!(
            "[shard {shard}] script {}",
            p.file_name().unwrap().to_string_lossy()
        );
        let out = run_script(p, mode);
        assert!(
            !out.contains("<error>"),
            "{}: top-level error\n{out}",
            p.display()
        );
        #[cfg(not(miri))]
        if let Some(want) = reference(p, mode) {
            assert_eq!(
                // (tracebacks of the reference show the driver chunk's frame)
                normalize(
                    &want
                        .split_inclusive('\n')
                        .filter(|l| !l.contains("/tmp/interp-"))
                        .collect::<String>()
                ),
                normalize(&out),
                "{}: output differs from the reference",
                p.display()
            );
        }
        n += 1;
    }
    eprintln!("shard {shard}: {n} scripts");
}

macro_rules! shards {
    ($($name:ident: $mode:expr, $k:expr;)*) => {
        $(
            #[test]
            fn $name() {
                run_shard($k, $mode);
            }
        )*
    };
}

shards! {
    inc_00: Mode::Inc, 0; inc_01: Mode::Inc, 1; inc_02: Mode::Inc, 2; inc_03: Mode::Inc, 3;
    inc_04: Mode::Inc, 4; inc_05: Mode::Inc, 5; inc_06: Mode::Inc, 6; inc_07: Mode::Inc, 7;
    inc_08: Mode::Inc, 8; inc_09: Mode::Inc, 9; inc_10: Mode::Inc, 10; inc_11: Mode::Inc, 11;
    gen_00: Mode::Gen, 0; gen_01: Mode::Gen, 1; gen_02: Mode::Gen, 2; gen_03: Mode::Gen, 3;
    gen_04: Mode::Gen, 4; gen_05: Mode::Gen, 5; gen_06: Mode::Gen, 6; gen_07: Mode::Gen, 7;
    gen_08: Mode::Gen, 8; gen_09: Mode::Gen, 9; gen_10: Mode::Gen, 10; gen_11: Mode::Gen, 11;
    pool_00: Mode::Pool, 0; pool_01: Mode::Pool, 1; pool_02: Mode::Pool, 2; pool_03: Mode::Pool, 3;
    pool_04: Mode::Pool, 4; pool_05: Mode::Pool, 5; pool_06: Mode::Pool, 6; pool_07: Mode::Pool, 7;
    pool_08: Mode::Pool, 8; pool_09: Mode::Pool, 9; pool_10: Mode::Pool, 10; pool_11: Mode::Pool, 11;
}
