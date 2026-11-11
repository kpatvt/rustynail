//! `cargo xtask official` / `cargo xtask diff-run`: run the official Lua test suite and our own
//! script corpora with `rnlua` and compare with the reference interpreters.
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::root;

fn short(ver: &str) -> String {
    ver.replace('.', "")
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Build `rnlua` for `ver`; returns the path of the binary.
fn build_cli(ver: &str, release: bool) -> PathBuf {
    let cfg = if ver == "5.4" {
        "rustynail_lua54"
    } else {
        "rustynail_lua55"
    };
    let base = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("target"));
    let target = if ver == "5.4" {
        let mut s = base.clone().into_os_string();
        s.push("-54");
        PathBuf::from(s)
    } else {
        base
    };
    let mut cmd = Command::new("cargo");
    cmd.args(["build", "-p", "rn-cli", "--bin", "rnlua"]);
    if release {
        cmd.arg("--release");
    }
    cmd.env("RUSTFLAGS", format!("--cfg {cfg}"))
        .env("CARGO_TARGET_DIR", &target);
    eprintln!("+ {cmd:?}");
    let st = cmd.status().expect("spawn cargo");
    if !st.success() {
        eprintln!("build failed");
        std::process::exit(st.code().unwrap_or(1));
    }
    target
        .join(if release { "release" } else { "debug" })
        .join("rnlua")
}

struct Run {
    code: i32, // 124 = timeout, -1 = killed by signal / spawn failure
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    secs: f64,
}

fn run_with_timeout(cmd: Command, secs: u64) -> Run {
    let start = std::time::Instant::now();
    let mut t = Command::new("timeout");
    t.args(["-k", "5", &secs.to_string()]);
    t.arg(cmd.get_program());
    t.args(cmd.get_args());
    if let Some(d) = cmd.get_current_dir() {
        t.current_dir(d);
    }
    for (k, v) in cmd.get_envs() {
        match v {
            Some(v) => t.env(k, v),
            None => t.env_remove(k),
        };
    }
    t.stdin(Stdio::null());
    let out: std::io::Result<Output> = t.output();
    let secs = start.elapsed().as_secs_f64();
    match out {
        Ok(o) => Run {
            code: o.status.code().unwrap_or(-1),
            stdout: o.stdout,
            stderr: o.stderr,
            secs,
        },
        Err(e) => Run {
            code: -1,
            stdout: vec![],
            stderr: format!("spawn failed: {e}").into_bytes(),
            secs,
        },
    }
}

fn copy_dir(src: &Path, dst: &Path) {
    let _ = std::fs::remove_dir_all(dst);
    std::fs::create_dir_all(dst.parent().expect("parent")).expect("mkdir");
    let st = Command::new("cp")
        .arg("-r")
        .arg(src)
        .arg(dst)
        .status()
        .expect("cp");
    assert!(st.success());
}

fn first_failure(r: &Run) -> String {
    if r.code == 0 {
        return String::new();
    }
    if r.code == 124 || r.code == 137 {
        return "timeout".into();
    }
    let err = String::from_utf8_lossy(&r.stderr);
    let line = err
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("(no stderr)");
    let mut s: String = line.chars().take(300).collect();
    if s.is_empty() {
        s = format!("exit code {}", r.code);
    }
    s
}

fn toml_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Extract `NNN` from `... file.lua:NNN: ...`.
fn failing_line(msg: &str) -> Option<u32> {
    let i = msg.find(".lua:")? + 5;
    let digits: String = msg[i..].chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

pub fn run(ver: &str, args: &[String]) {
    let release = args.iter().any(|a| a == "--release");
    let timeout: u64 = flag_value(args, "--timeout")
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let only: Option<Vec<String>> =
        flag_value(args, "--only").map(|s| s.split(',').map(String::from).collect());
    let ours_bin = build_cli(ver, release);
    let ref_bin = root().join(format!("ref/{}/build/lua", short(ver)));
    assert!(
        ref_bin.exists(),
        "missing {ref_bin:?}; run `cargo xtask ref-build`"
    );
    let src = root().join(format!("vendor/lua-tests-{ver}"));
    assert!(src.is_dir(), "missing {src:?}");

    let work = PathBuf::from(std::env::var("TMPDIR").unwrap_or("/tmp".into()))
        .join(format!("rustynail-official-{}", short(ver)));
    let dir_ref = work.join("ref").join("tests");
    let dir_ours = work.join("ours").join("tests");
    copy_dir(&src, &dir_ref);
    copy_dir(&src, &dir_ours);

    const SKIP: &[&str] = &["all.lua", "heavy.lua", "verybig.lua", "tracegc.lua"];
    let mut files: Vec<String> = std::fs::read_dir(&src)
        .expect("read test dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".lua") && !SKIP.contains(&n.as_str()))
        .collect();
    files.sort();
    files.insert(0, "all.lua".into());
    if let Some(o) = &only {
        files.retain(|f| o.contains(f));
    }

    let mut toml = String::new();
    let _ = writeln!(
        toml,
        "# generated by `cargo xtask official --lua-version {ver}`"
    );
    let _ = writeln!(toml, "lua_version = \"{ver}\"");
    let _ = writeln!(toml, "timeout_secs = {timeout}");
    let mut rows = Vec::new();
    for f in &files {
        let mk = |bin: &Path, dir: &Path| {
            let mut c = Command::new(bin);
            c.args(["-e", "_port=true;_soft=true", f])
                .current_dir(dir)
                .env("LC_ALL", "C");
            c
        };
        // reference and ours run concurrently, each in its own copy of the test directory
        let (r, o) = std::thread::scope(|s| {
            let a = s.spawn(|| run_with_timeout(mk(&ref_bin, &dir_ref), timeout));
            let b = s.spawn(|| run_with_timeout(mk(&ours_bin, &dir_ours), timeout));
            (a.join().expect("ref thread"), b.join().expect("our thread"))
        });
        let st = |r: &Run| match r.code {
            0 => "pass",
            124 | 137 => "timeout",
            _ => "fail",
        };
        let ff = first_failure(&o);
        let _ = writeln!(toml, "\n[[file]]\nname = {}", toml_str(f));
        let _ = writeln!(toml, "reference = {}", toml_str(st(&r)));
        let _ = writeln!(toml, "ours = {}", toml_str(st(&o)));
        if !ff.is_empty() {
            let _ = writeln!(toml, "first_failure = {}", toml_str(&ff));
            if let Some(l) = failing_line(&ff) {
                let _ = writeln!(toml, "first_failure_line = {l}");
            }
        }
        if st(&r) != "pass" {
            let _ = writeln!(toml, "reference_failure = {}", toml_str(&first_failure(&r)));
        }
        // timings are not recorded in the status file (they are noise and would make `git diff --exit-code tests/` flaky)
        eprintln!(
            "{f:<16} ref={:<7} ours={:<7} ({:.1}s / {:.1}s) {ff}",
            st(&r),
            st(&o),
            r.secs,
            o.secs
        );
        rows.push((f.clone(), st(&r), st(&o), ff));
    }
    let out = root().join(if ver == "5.5" {
        "tests/official-status.toml".to_string()
    } else {
        format!("tests/official-status-{ver}.toml")
    });
    std::fs::write(&out, toml).expect("write status");
    let ref_pass = rows.iter().filter(|r| r.1 == "pass").count();
    let ours_pass = rows.iter().filter(|r| r.2 == "pass").count();
    let both = rows
        .iter()
        .filter(|r| r.1 == "pass" && r.2 == "pass")
        .count();
    println!("\nfile             reference  ours");
    for (f, r, o, ff) in &rows {
        println!("{f:<16} {r:<10} {o:<8} {ff}");
    }
    println!(
        "\nreference passes {ref_pass}/{n}; ours passes {ours_pass}/{n}; both pass {both}; \
         reference-pass but ours-fail: {}\nwritten {}",
        rows.iter()
            .filter(|r| r.1 == "pass" && r.2 != "pass")
            .count(),
        out.display(),
        n = rows.len()
    );
}

/// Replace every `0x<hex>` run by `0xADDR`.
fn normalize(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'0'
            && s.get(i + 1) == Some(&b'x')
            && s.get(i + 2).is_some_and(u8::is_ascii_hexdigit)
        {
            let mut j = i + 2;
            while j < s.len() && s[j].is_ascii_hexdigit() {
                j += 1;
            }
            out.extend_from_slice(b"0xADDR");
            i = j;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

/// stderr of the reference runner is the bare message; ours is `lua: msg` plus a traceback.
fn normalize_ours_stderr(s: &[u8], bin: &Path) -> Vec<u8> {
    let t = String::from_utf8_lossy(s).into_owned();
    let pre = format!("{}: ", bin.display());
    let t = t.strip_prefix(pre.as_str()).unwrap_or(&t);
    let t = t.strip_prefix("lua: ").unwrap_or(t);
    let t = match t.find("\nstack traceback:") {
        Some(i) => &t[..i + 1],
        None => t,
    };
    t.as_bytes().to_vec()
}

pub fn diff_run(ver: &str, args: &[String]) {
    let release = args.iter().any(|a| a == "--release");
    let timeout: u64 = flag_value(args, "--timeout")
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let ours_bin = build_cli(ver, release);
    let refrun = root().join(format!("ref/{}/build-seed0/refrun", short(ver)));
    assert!(
        refrun.exists(),
        "missing {refrun:?}; run `cargo xtask ref-build`"
    );
    let mut scripts: Vec<PathBuf> = Vec::new();
    for d in ["tests/vm", "tests/lua/run"] {
        if let Ok(rd) = std::fs::read_dir(root().join(d)) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "lua") && p.is_file() {
                    scripts.push(p);
                }
            }
        }
    }
    scripts.sort();
    let mut bad = 0;
    for s in &scripts {
        let dir = s.parent().expect("parent");
        let name = s.file_name().expect("name");
        let mut a = Command::new(&refrun);
        a.arg(name).current_dir(dir).env("LC_ALL", "C");
        let mut b = Command::new(&ours_bin);
        b.arg(name)
            .current_dir(dir)
            .env("LC_ALL", "C")
            .env("RUSTYNAIL_SEED", "0");
        let (ra, rb) = std::thread::scope(|sc| {
            let x = sc.spawn(|| run_with_timeout(a, timeout));
            let y = sc.spawn(|| run_with_timeout(b, timeout));
            (x.join().expect("ref"), y.join().expect("ours"))
        });
        // `rnlua` runs the script under a C-like `pmain` frame that `refrun` does not have
        let strip_main = |b: &[u8]| -> Vec<u8> {
            String::from_utf8_lossy(b)
                .replace("\n\t[C]: in ?\n", "\n")
                .into_bytes()
        };
        let (oa, ob) = (normalize(&ra.stdout), normalize(&strip_main(&rb.stdout)));
        let (ea, eb) = (
            normalize(&ra.stderr),
            normalize(&normalize_ours_stderr(&rb.stderr, &ours_bin)),
        );
        let mut why = Vec::new();
        if ra.code != rb.code {
            why.push(format!("exit code {} vs {}", ra.code, rb.code));
        }
        if oa != ob {
            why.push(format!("stdout differs: {}", first_diff(&oa, &ob)));
        }
        if ea.trim_ascii_end() != eb.trim_ascii_end() {
            why.push(format!("stderr differs: {}", first_diff(&ea, &eb)));
        }
        let rel = s.strip_prefix(root()).unwrap_or(s);
        if why.is_empty() {
            println!("ok    {}", rel.display());
        } else {
            bad += 1;
            println!("FAIL  {}: {}", rel.display(), why.join("; "));
        }
    }
    println!("\n{} scripts, {} mismatches", scripts.len(), bad);
    if bad > 0 {
        std::process::exit(1);
    }
}

fn first_diff(a: &[u8], b: &[u8]) -> String {
    let (sa, sb) = (String::from_utf8_lossy(a), String::from_utf8_lossy(b));
    let mut la = sa.lines();
    let mut lb = sb.lines();
    let mut n = 1;
    loop {
        match (la.next(), lb.next()) {
            (Some(x), Some(y)) if x == y => n += 1,
            (None, None) => return "(trailing whitespace only)".into(),
            (x, y) => {
                return format!(
                    "line {n}: ref {:?} vs ours {:?}",
                    x.unwrap_or("<eof>").chars().take(120).collect::<String>(),
                    y.unwrap_or("<eof>").chars().take(120).collect::<String>()
                )
            }
        }
    }
}
