//! Runs Lua snippets through the real front end + interpreter and compares with the reference binary.
use std::process::Command;

fn ref_bin() -> Option<std::path::PathBuf> {
    #[cfg(not(rustynail_lua54))]
    let p = "ref/55/build/lua";
    #[cfg(rustynail_lua54)]
    let p = "ref/54/build/lua";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let b = root.join(p);
    b.exists().then_some(b)
}

/// Run `src` in a fresh state with all libs; returns the output of `print`-free evaluation as the
/// string result of the chunk (the chunk should `return` a string).
fn run_ours(src: &str) -> Result<String, String> {
    let mut st = rn_std::new_state_with_libs().map_err(|e| format!("{e:?}"))?;
    let status = st.load_chunk(src.as_bytes(), b"=(command line)", None);
    if status != rn_core::status::Status::Ok {
        let m = st.get(st.top() - 1);
        return Err(String::from_utf8_lossy(m.as_string().unwrap().as_bytes()).into_owned());
    }
    let f = st.top() - 1;
    let s = st.pcall(f, 1, 0);
    let v = st.get(st.top() - 1);
    let text = v
        .as_string()
        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
        .unwrap_or_else(|| format!("{v:?}"));
    if s == rn_core::status::Status::Ok {
        Ok(text)
    } else {
        Err(text)
    }
}

fn run_ref(src: &str) -> Option<String> {
    let b = ref_bin()?;
    let wrapped = format!("local ok, r = pcall(function() {src} end) if ok then io.write(tostring(r)) else io.write('ERR:', tostring(r)) end");
    let out = Command::new(b).arg("-e").arg(wrapped).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn base_smoke() {
    let cases = [
        "return tostring(1)..type(print)..tostring(nil)",
        "return select('#', 1, 2, 3)",
        "return tostring(select(-1, 1, 2, 3))",
        "return tostring(tonumber('0x10'))..tostring(tonumber('z', 36))",
        "local t = setmetatable({}, {__index = function(_, k) return k end}) return t.x",
        "return table.concat({pcall(error, 'x', 0)}, '|', 1, 2)",
        "return select(2, pcall(string.rep))",
        "local n = 0 for k, v in pairs({1, 2, 3, a = 1}) do n = n + 1 end return n",
        "local n = 0 for i, v in ipairs({1, 2, 3}) do n = n + v end return n",
        "return select(2, pcall(setmetatable, 1, 2))",
        "return select(2, pcall(assert, false, 'boom'))",
        "return tostring(rawequal('a', 'a'))..rawlen({1,2})",
        "return select(2, load('x ='))",
        "return load('return 1 + 1')()",
    ];
    let mut bad = 0;
    for c in cases {
        let ours = run_ours(c);
        let ours_s = match &ours {
            Ok(s) => s.clone(),
            Err(e) => format!("ERR:{e}"),
        };
        if let Some(r) = run_ref(c) {
            if r != ours_s {
                bad += 1;
                eprintln!("MISMATCH for {c}\n  ours: {ours_s:?}\n  ref:  {r:?}");
            }
        }
    }
    assert_eq!(bad, 0);
}
