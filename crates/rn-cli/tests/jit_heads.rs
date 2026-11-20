//! H8 diagnostics (docs/STUDY_COVERAGE.md): the per-head table of `RUSTYNAIL_JIT_VERBOSE=1`, the sampler
//! (`RUSTYNAIL_JIT_SAMPLE`) and the perf-map names (`RUSTYNAIL_JIT_PERFMAP=2`). The tests check the output
//! *format*, never the counters' values.
#![cfg(feature = "jit")]
use std::process::{Command, Stdio};

const PROG: &str = r#"
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local V = {}
V.__index = V
local function vec(x, y) return setmetatable({ x = x, y = y }, V) end
function V:add(o) return vec(self.x + o.x, self.y + o.y) end
local s, acc = 0, vec(0, 0)
local one = vec(1, 1)
for r = 1, 40 do
  s = s + fib(18)
  for i = 1, 3000 do acc = acc:add(one) end
end
print(s, acc.x)
"#;

/// A loop the recorder refuses (`math.max` of an integer and a float): the abort site names the recorder's source line.
const NYI_PROG: &str =
    "local max, s = math.max, 0\nfor i = 1, 200000 do s = s + max(0, i * 0.5 - 3) end\nprint(s)\n";

/// Runs [`PROG`] with a scrubbed JIT environment plus `env`; returns (stdout, stderr, child pid).
fn run(env: &[(&str, &str)]) -> (String, String, u32) {
    run_prog(PROG, env)
}

fn run_prog(prog: &str, env: &[(&str, &str)]) -> (String, String, u32) {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rn_jitheads_{}_{seq}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.lua");
    std::fs::write(&path, prog).unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_rnlua"));
    c.arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, _) in std::env::vars() {
        if k.starts_with("RUSTYNAIL_JIT") {
            c.env_remove(k);
        }
    }
    for (k, v) in env {
        c.env(k, v);
    }
    let ch = c.spawn().unwrap();
    let pid = ch.id();
    let o = ch.wait_with_output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
        pid,
    )
}

#[test]
fn head_table_format() {
    let (out, err, _) = run(&[("RUSTYNAIL_JIT_VERBOSE", "1")]);
    assert!(!out.is_empty(), "{out}");
    let header = err
        .lines()
        .find(|l| l.starts_with("[heads] ") && l.ends_with(" heads with activity"))
        .unwrap_or_else(|| panic!("no [heads] header:\n{err}"));
    let n: usize = header["[heads] ".len()..]
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let rows: Vec<&str> = err
        .lines()
        .filter(|l| l.starts_with("[heads] s.lua:") && l.contains(" pc "))
        .collect();
    assert_eq!(rows.len(), n, "{err}");
    for r in rows {
        // `[heads] s.lua:<line> pc <pc> <fn|loop|cont|?> <state>: rec <n> inst <n> ... entries <n> (fail <n>)`
        let rest = r.strip_prefix("[heads] s.lua:").unwrap();
        let (line, rest) = rest.split_once(" pc ").unwrap();
        line.parse::<u32>().unwrap();
        let mut it = rest.split(' ');
        it.next().unwrap().parse::<u32>().unwrap();
        assert!(
            matches!(it.next().unwrap(), "fn" | "loop" | "cont" | "?"),
            "{r}"
        );
        let state = it.next().unwrap();
        assert!(
            ["Warm:", "Recording:", "Traced:", "Black:", "gone:"].contains(&state),
            "{r}"
        );
        assert!(
            r.contains(": rec ") && r.contains(" inst ") && r.contains(" entries "),
            "{r}"
        );
    }
    assert!(
        !err.contains("[heads] samples"),
        "no sampler without RUSTYNAIL_JIT_SAMPLE:\n{err}"
    );
}

#[test]
fn abort_sites_name_reason_line_op_and_source() {
    let (_, err, _) = run_prog(NYI_PROG, &[("RUSTYNAIL_JIT_VERBOSE", "1")]);
    let row = err
        .lines()
        .find(|l| l.starts_with("[heads] s.lua:") && l.contains("aborts {"))
        .unwrap_or_else(|| panic!("no abort row:\n{err}"));
    // `... at {<reason>@L<line>:<pc>:<op>[record.rs:<n>]:<count>,...}`
    let at = row
        .split(" at {")
        .nth(1)
        .unwrap_or_else(|| panic!("no sites: {row}"));
    let site = at.split(['}', ',']).next().unwrap();
    assert!(site.starts_with("Nyi(Call)@L"), "{row}");
    assert!(site.contains(":Call[record.rs:"), "{row}");
    assert!(row.contains("black@rec#"), "{row}");
}

#[test]
fn head_table_is_off_by_default() {
    let (_, err, _) = run(&[]);
    assert!(!err.contains("[heads]"), "{err}");
    assert!(!err.contains("[trace]"), "{err}");
}

#[test]
fn sampler_lines() {
    let (_, err, _) = run(&[
        ("RUSTYNAIL_JIT_VERBOSE", "1"),
        ("RUSTYNAIL_JIT_SAMPLE", "on"),
    ]);
    let l = err
        .lines()
        .find(|l| l.starts_with("[heads] samples "))
        .unwrap_or_else(|| panic!("no samples line:\n{err}"));
    // `[heads] samples <n>: tier1 <x>% traces <y>% runtime (...) <z>%`
    assert!(
        l.contains(": tier1 ") && l.contains("% traces ") && l.contains("% runtime ("),
        "{l}"
    );
    for p in err.lines().filter(|l| l.starts_with("[heads] proto ")) {
        assert!(p.contains(": tier1 ") && p.contains("% traces "), "{p}");
    }
}

#[test]
fn perfmap_levels() {
    let read = |pid: u32| {
        let f = format!("/tmp/perf-{pid}.map");
        let s = std::fs::read_to_string(&f).unwrap_or_default();
        let _ = std::fs::remove_file(&f);
        s
    };
    let (_, _, pid) = run(&[("RUSTYNAIL_JIT_PERFMAP", "1")]);
    let m1 = read(pid);
    assert!(m1.contains(" lua_proto@"), "{m1}");
    assert!(!m1.contains("TRACE-") && !m1.contains("lua_proto:"), "{m1}");
    let (_, _, pid) = run(&[("RUSTYNAIL_JIT_PERFMAP", "2")]);
    let m2 = read(pid);
    assert!(
        m2.lines().any(|l| l
            .rsplit(" ")
            .next()
            .is_some_and(|n| n.starts_with("TRACE-"))),
        "{m2}"
    );
    assert!(m2.contains(" lua_proto:"), "{m2}");
    let (_, _, pid) = run(&[]);
    assert!(read(pid).is_empty());
}
