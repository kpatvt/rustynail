//! C API conformance (V6): C programs compiled against the reference `liblua.a` and against
//! `librn_capi.a` must behave identically; the vendored `lua.c` linked with our library must pass
//! the official test suite; struct layouts must match the C headers.
//!
//! Needs a C compiler (`cc`); the reference comparisons are skipped when `ref/55` is not built.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// Version under test (`--cfg rustynail_lua54` selects 5.4).
const VER: &str = if cfg!(rustynail_lua54) { "54" } else { "55" };
const DOTVER: &str = if cfg!(rustynail_lua54) { "5.4" } else { "5.5" };

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// `target/<profile>` of this test binary.
fn target_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().parent().unwrap().to_path_buf()
}

/// A scratch directory unique to this test process.
fn scratch() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("rn_capi_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    })
    .clone()
}

/// Build (once) and locate `librn_capi.a` for this profile.
fn our_lib() -> PathBuf {
    static LIB: OnceLock<PathBuf> = OnceLock::new();
    LIB.get_or_init(|| {
        let dir = target_dir();
        let mut cmd = Command::new(env!("CARGO"));
        cmd.args(["build", "-p", "rn-capi"]);
        if dir.file_name().is_some_and(|d| d == "release") {
            cmd.arg("--release");
        }
        // a separate target dir would not contain the library we are looking for: use ours
        let ok = cmd.current_dir(root()).status().is_ok_and(|s| s.success());
        assert!(ok, "could not build rn-capi");
        let lib = dir.join("librn_capi.a");
        assert!(lib.exists(), "missing {}", lib.display());
        lib
    })
    .clone()
}

fn ref_lib() -> Option<PathBuf> {
    let p = root().join(format!("ref/{VER}/build-seed0/liblua.a"));
    p.exists().then_some(p)
}

fn include() -> PathBuf {
    root().join(format!("vendor/lua-{DOTVER}/src"))
}

/// Compile `sources` against `lib` into `name`; returns the executable.
fn cc(name: &str, sources: &[PathBuf], lib: &Path, extra: &[&str]) -> PathBuf {
    let out = scratch().join(name);
    let status = Command::new("cc")
        .args(["-O1", "-w", "-DLUA_USE_POSIX"])
        .args(extra)
        .arg(format!("-I{}", include().display()))
        .arg("-o")
        .arg(&out)
        .args(sources)
        .arg(lib)
        .args(["-lm", "-ldl", "-lpthread"])
        .status()
        .expect("cc not available");
    assert!(status.success(), "compiling {name} failed");
    out
}

fn run(exe: &Path, args: &[&str], timeout_secs: u32) -> (Option<i32>, String, String) {
    run_env(exe, args, timeout_secs, &[])
}

fn run_env(
    exe: &Path,
    args: &[&str],
    timeout_secs: u32,
    env: &[(&str, &str)],
) -> (Option<i32>, String, String) {
    let mut cmd = Command::new("timeout");
    cmd.arg(timeout_secs.to_string())
        .arg(exe)
        .args(args)
        .stdin(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let o = cmd.output().expect("run");
    use std::os::unix::process::ExitStatusExt;
    (
        o.status
            .code()
            .or_else(|| o.status.signal().map(|s| 128 + s)),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

const SECTIONS: &[&str] = &[
    "stack",
    "types",
    "arith",
    "tables",
    "calls",
    "load",
    "coroutines",
    "auxcheck",
    "auxmisc",
    "auxresults",
    "userdata",
    "debug",
    "gc",
    "misc",
    "alloc",
    "iostream",
];

#[test]
fn c_api_behaves_like_the_reference() {
    let Some(rl) = ref_lib() else {
        eprintln!("note: ref/55 not built; differential C API test skipped");
        return;
    };
    let src = root().join("tests/c/capi_diff.c");
    let ref_exe = cc("diff_ref", std::slice::from_ref(&src), &rl, &[]);
    let our_exe = cc("diff_ours", &[src], &our_lib(), &[]);
    let mut bad = Vec::new();
    for s in SECTIONS {
        let r = run(&ref_exe, &[s], 120);
        let o = run(&our_exe, &[s], 120);
        if r.0 != o.0 || r.1 != o.1 {
            let first =
                r.1.lines()
                    .zip(o.1.lines())
                    .find(|(a, b)| a != b)
                    .map(|(a, b)| format!("\n   ref:  {a}\n   ours: {b}"))
                    .unwrap_or_default();
            bad.push(format!("section {s}: exit {:?} vs {:?}{first}", r.0, o.0));
        }
        assert_eq!(r.0, Some(0), "the reference run of section {s} failed");
    }
    assert!(
        bad.is_empty(),
        "{} sections differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The vendored `lua.c`, linked with our library, runs the official test suite.
#[test]
fn official_suite_through_lua_c() {
    let tests = root().join(format!("vendor/lua-tests-{DOTVER}"));
    if !tests.exists() {
        eprintln!("note: vendor/lua-tests-5.5 missing; skipped");
        return;
    }
    let lua_c = include().join("lua.c");
    let exe = cc("lua_c", &[lua_c], &our_lib(), &[]);
    let mut failed = Vec::new();
    let mut n = 0;
    let mut files: Vec<_> = std::fs::read_dir(&tests)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|f| f.ends_with(".lua") && f != "all.lua" && f != "heavy.lua")
        .collect();
    files.sort();
    for f in files {
        let o = Command::new("timeout")
            .arg("300")
            .arg(&exe)
            .args(["-e", "_port=true;_soft=true", &f])
            .current_dir(&tests)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        n += 1;
        if !o.status.success() {
            let err = String::from_utf8_lossy(&o.stderr);
            failed.push(format!(
                "{f}: {:?} {}",
                o.status.code(),
                err.lines().next().unwrap_or("")
            ));
        }
    }
    assert!(n >= 28, "only {n} files ran");
    assert!(
        failed.is_empty(),
        "{} files failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

/// An unprotected error calls the panic function and aborts, like the reference.
#[test]
fn unprotected_error_panics_like_the_reference() {
    let src = root().join("tests/c/panic.c");
    let our_exe = cc("panic_ours", std::slice::from_ref(&src), &our_lib(), &[]);
    let norm = |s: &str| s.replace("\r", "");
    for args in [&[][..], &["custom"][..]] {
        let o = run(&our_exe, args, 30);
        assert_eq!(o.0, Some(134), "must abort (SIGABRT): {o:?}"); // timeout reports 128+6
        if args.is_empty() {
            assert!(
                norm(&o.2)
                    .contains("PANIC: unprotected error in call to Lua API (unprotected failure)"),
                "{o:?}"
            );
        } else {
            assert!(
                norm(&o.2).contains("custom panic: unprotected failure"),
                "{o:?}"
            );
        }
        if let Some(rl) = ref_lib() {
            let ref_exe = cc("panic_ref", std::slice::from_ref(&src), &rl, &[]);
            let r = run(&ref_exe, args, 30);
            assert_eq!(
                (r.0, norm(&r.2)),
                (o.0, norm(&o.2)),
                "differs from the reference"
            );
        }
    }
}

/// The struct layouts match the C headers.
#[test]
fn struct_layouts_match_the_c_headers() {
    use rn_capi::*;
    use std::mem::{align_of, offset_of, size_of};
    let src = root().join("tests/c/layout.c");
    // layout.c needs only the headers: link against our library (unused symbols are fine)
    let exe = cc("layout", &[src], &our_lib(), &[]);
    let (code, out, _) = run(&exe, &[], 30);
    assert_eq!(code, Some(0));
    let mut want = String::new();
    macro_rules! f {
        ($s:literal, $t:ty, $f:ident, $ft:ty) => {
            want += &format!(
                "{}.{} offset={} size={}\n",
                $s,
                stringify!($f),
                offset_of!($t, $f),
                size_of::<$ft>()
            );
        };
    }
    want += &format!(
        "lua_Debug size={} align={}\n",
        size_of::<lua_Debug>(),
        align_of::<lua_Debug>()
    );
    f!("lua_Debug", lua_Debug, event, i32);
    f!("lua_Debug", lua_Debug, name, *const u8);
    f!("lua_Debug", lua_Debug, namewhat, *const u8);
    f!("lua_Debug", lua_Debug, what, *const u8);
    f!("lua_Debug", lua_Debug, source, *const u8);
    f!("lua_Debug", lua_Debug, srclen, usize);
    f!("lua_Debug", lua_Debug, currentline, i32);
    f!("lua_Debug", lua_Debug, linedefined, i32);
    f!("lua_Debug", lua_Debug, lastlinedefined, i32);
    f!("lua_Debug", lua_Debug, nups, u8);
    f!("lua_Debug", lua_Debug, nparams, u8);
    f!("lua_Debug", lua_Debug, isvararg, u8);
    #[cfg(not(rustynail_lua54))]
    f!("lua_Debug", lua_Debug, extraargs, u8);
    f!("lua_Debug", lua_Debug, istailcall, u8);
    #[cfg(not(rustynail_lua54))]
    f!("lua_Debug", lua_Debug, ftransfer, i32);
    #[cfg(not(rustynail_lua54))]
    f!("lua_Debug", lua_Debug, ntransfer, i32);
    #[cfg(rustynail_lua54)]
    f!("lua_Debug", lua_Debug, ftransfer, u16);
    #[cfg(rustynail_lua54)]
    f!("lua_Debug", lua_Debug, ntransfer, u16);
    f!("lua_Debug", lua_Debug, short_src, [u8; LUA_IDSIZE]);
    f!("lua_Debug", lua_Debug, i_ci, *const u8);
    want += &format!(
        "luaL_Buffer size={} align={}\n",
        size_of::<luaL_Buffer>(),
        align_of::<luaL_Buffer>()
    );
    f!("luaL_Buffer", luaL_Buffer, b, *const u8);
    f!("luaL_Buffer", luaL_Buffer, size, usize);
    f!("luaL_Buffer", luaL_Buffer, n, usize);
    f!("luaL_Buffer", luaL_Buffer, L, *const u8);
    f!("luaL_Buffer", luaL_Buffer, init, BufInit);
    want += &format!("luaL_Reg size={}\n", size_of::<luaL_Reg>());
    f!("luaL_Reg", luaL_Reg, name, *const u8);
    f!("luaL_Reg", luaL_Reg, func, lua_CFunction);
    want += &format!("luaL_Stream size={}\n", size_of::<luaL_Stream>());
    f!("luaL_Stream", luaL_Stream, f, *const u8);
    f!("luaL_Stream", luaL_Stream, closef, lua_CFunction);
    want += &format!(
        "LUAL_BUFFERSIZE={} LUA_IDSIZE={} LUA_EXTRASPACE={} LUA_REGISTRYINDEX={}\n",
        LUAL_BUFFERSIZE, LUA_IDSIZE, 8, LUA_REGISTRYINDEX
    );
    want += &format!(
        "lua_Integer={} lua_Number={} lua_KContext={} LUA_MINSTACK={} MULTRET={}\n",
        size_of::<lua_Integer>(),
        size_of::<lua_Number>(),
        size_of::<lua_KContext>(),
        LUA_MINSTACK,
        LUA_MULTRET
    );
    want += &format!(
        "RIDX globals={} mainthread={}\n",
        LUA_RIDX_GLOBALS, LUA_RIDX_MAINTHREAD
    );
    assert_eq!(out, want, "struct layouts differ from the C headers");
}
