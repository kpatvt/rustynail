//! Shared helpers for the LIBS1 differential tests: run a snippet through our interpreter and the
//! reference binary and compare the resulting strings.
#![allow(dead_code)]
use std::process::Command;

pub fn ref_bin() -> Option<std::path::PathBuf> {
    #[cfg(not(rustynail_lua54))]
    let p = "ref/55/build/lua";
    #[cfg(rustynail_lua54)]
    let p = "ref/54/build/lua";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let b = root.join(p);
    b.exists().then_some(b)
}

fn wrap(src: &str) -> String {
    format!(
        "(function() local ok, r = pcall(function() {src}\n end) if ok then return tostring(r) else return 'ERR:'..tostring(r) end end)()"
    )
}

pub fn run_ours(src: &str) -> String {
    let mut st = rn_std::new_state_with_libs().expect("state");
    let chunk = format!("return {}", wrap(src));
    let status = st.load_chunk(chunk.as_bytes(), b"=(command line)", None);
    if status != rn_core::status::Status::Ok {
        let m = st.get(st.top() - 1);
        return format!(
            "LOADERR:{}",
            String::from_utf8_lossy(m.as_string().unwrap().as_bytes())
        );
    }
    let f = st.top() - 1;
    let s = st.pcall(f, 1, 0);
    let v = st.get(st.top() - 1);
    let text = v
        .as_string()
        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
        .unwrap_or_else(|| format!("{v:?}"));
    if s == rn_core::status::Status::Ok {
        text
    } else {
        format!("OUTER-ERR:{text}")
    }
}

pub fn run_ref(src: &str) -> Option<String> {
    let b = ref_bin()?;
    let chunk = format!("io.write({})", wrap(src));
    let out = Command::new(b).arg("-e").arg(chunk).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Compare all cases; returns the list of mismatches. Addresses (`0x...`) are normalised.
pub fn compare(cases: &[&str]) -> Vec<String> {
    let mut bad = Vec::new();
    for c in cases {
        let ours = norm(&run_ours(c));
        if let Some(r) = run_ref(c) {
            let r = norm(&r);
            if r != ours {
                bad.push(format!("MISMATCH for {c}\n  ours: {ours:?}\n  ref:  {r:?}"));
            }
        }
    }
    bad
}

fn norm(s: &str) -> String {
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'0'
            && i + 1 < b.len()
            && b[i + 1] == b'x'
            && i + 2 < b.len()
            && b[i + 2].is_ascii_hexdigit()
        {
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_hexdigit() {
                j += 1;
            }
            if j - i > 8 {
                out.push_str("0xADDR");
                i = j;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    // 5.4 (LUA_COMPAT_MATHLIB): `math.atan2` is the same C function as `math.atan`, so which
    // name an error message gets (`pushglobalfuncname`, first match in `package.loaded.math`)
    // depends on the random string-hash seed, in the reference as well
    if cfg!(rustynail_lua54) {
        out = out.replace("'math.atan2'", "'math.atan'");
    }
    out
}

pub fn assert_no_mismatch(cases: &[&str]) {
    if ref_bin().is_none() {
        eprintln!("reference binary missing; skipping");
        return;
    }
    let bad = compare(cases);
    assert!(
        bad.is_empty(),
        "{} mismatches:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

const PRELUDE: &str = "local function J(...) local t={} for i=1,select('#',...) do t[#t+1]=tostring((select(i,...))) end return table.concat(t,',') end local function P(f,...) return J(pcall(f,...)) end ";

fn long_str(s: &str) -> String {
    let mut n = 2;
    loop {
        let close = format!("]{}]", "=".repeat(n));
        if !s.contains(&close) {
            return format!("[{}[{}]{}]", "=".repeat(n), s, "=".repeat(n));
        }
        n += 1;
    }
}

fn harness(cases: &[String]) -> String {
    let mut h = String::from("local cases = {\n");
    for c in cases {
        h.push_str(&long_str(c));
        h.push_str(",\n");
    }
    h.push_str("}\nlocal out = {}\n");
    h.push_str(&format!("local prelude = {}\n", long_str(PRELUDE)));
    h.push_str(
        "for i, src in ipairs(cases) do\n local f, e = load(prelude .. src, '=(command line)')\n if not f then out[#out+1] = 'LOADERR:' .. tostring(e) else\n  local ok, r = pcall(f)\n  out[#out+1] = ok and tostring(r) or ('ERR:' .. tostring(r))\n end\nend\nreturn table.concat(out, '\\30')\n",
    );
    h
}

/// Run all `cases` in one interpreter / one reference process; returns the mismatching cases.
pub fn compare_batch(cases: &[String]) -> Vec<String> {
    let Some(b) = ref_bin() else {
        eprintln!("reference binary missing; skipping");
        return Vec::new();
    };
    let h = harness(cases);
    let mut st = rn_std::new_state_with_libs().expect("state");
    let status = st.load_chunk(h.as_bytes(), b"=harness", None);
    assert!(status == rn_core::status::Status::Ok, "harness load failed");
    let f = st.top() - 1;
    let s = st.pcall(f, 1, 0);
    let v = st.get(st.top() - 1);
    let ours = v
        .as_string()
        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
        .unwrap_or_else(|| format!("harness failed: {v:?}"));
    if ours.contains("interpreter is not implemented yet") {
        eprintln!("5.4 interpreter missing; skipping");
        return Vec::new();
    }
    assert!(s == rn_core::status::Status::Ok, "harness error: {ours}");
    let path = std::env::temp_dir().join(format!(
        "libs1_harness_{}_{}.lua",
        std::process::id(),
        cases.len()
    ));
    std::fs::write(&path, format!("io.write((function() {} end)())", h)).unwrap();
    let out = Command::new(b).arg(&path).output().expect("run ref");
    let _ = std::fs::remove_file(&path);
    let r = String::from_utf8_lossy(&out.stdout).into_owned();
    let (ours, r) = (norm(&ours), norm(&r));
    let ov: Vec<&str> = ours.split('\x1e').collect();
    let rv: Vec<&str> = r.split('\x1e').collect();
    let mut bad = Vec::new();
    if ov.len() != rv.len() {
        bad.push(format!(
            "different number of results: ours {} ref {}\nref stderr: {}",
            ov.len(),
            rv.len(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    for (i, c) in cases.iter().enumerate() {
        let (a, b) = (
            ov.get(i).copied().unwrap_or("<missing>"),
            rv.get(i).copied().unwrap_or("<missing>"),
        );
        if a != b {
            bad.push(format!("MISMATCH for {c}\n  ours: {a:?}\n  ref:  {b:?}"));
        }
    }
    bad
}

pub fn assert_batch(cases: &[String]) {
    let bad = compare_batch(cases);
    let n = bad.len();
    assert!(
        bad.is_empty(),
        "{n} mismatches (of {}):\n{}",
        cases.len(),
        bad.iter().take(40).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// `P(f, args)` / direct-call / method-call cases for every function name x argument list.
pub fn call_matrix(prefix: &str, names: &[&str], args: &[&str]) -> Vec<String> {
    let mut v = Vec::new();
    for n in names {
        for a in args {
            v.push(format!(
                "return P({prefix}.{n}{}{a})",
                if a.is_empty() { "" } else { "," }
            ));
            v.push(format!(
                "return select(2, pcall(function() return {prefix}.{n}({a}) end))"
            ));
        }
    }
    v
}
