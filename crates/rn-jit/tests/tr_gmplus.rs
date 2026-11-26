//! S6 `gmplus` (docs/JIT_TRACE.md section 64; rn-std `STR_FAST` bit `GMPLUS`): `string.gmatch` of `%x+` / `[^c]+` patterns scans the class
//! directly. The script `tests/vm/s6_01_gmplus.lua` is run in-process with `print` captured, with the bit on and off, and its output compared
//! with the reference interpreter's (`tests/data/s64_gmplus.expected`; 5.4 and 5.5 print the same).
//! Detector of the mutant `gmplus_set_neg_skip`.
#![cfg(feature = "trace")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;

const SCRIPT: &str = include_str!("../../../tests/vm/s6_01_gmplus.lua");
const EXPECTED: &str = include_str!("data/s64_gmplus.expected");

const PRE: &str = "local __buf = {}\n\
local function print(...) local n = select('#', ...) local t = {} \
for i = 1, n do t[i] = tostring((select(i, ...))) end __buf[#__buf + 1] = table.concat(t, '\\t') end\n";
const POST: &str = "\nreturn table.concat(__buf, '\\n') .. '\\n'";

fn run(src: &str) -> String {
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    #[cfg(feature = "tr_mutants")]
    rn_jit::tr::mutants::apply_vm_mutants();
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=s64", None),
        Status::Ok,
        "load"
    );
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    assert_eq!(status, Status::Ok, "script failed: {v:?}");
    let s = String::from_utf8_lossy(v.as_string().expect("string result").as_bytes()).into_owned();
    st.set_top(base);
    s
}

#[test]
fn gmplus_matches_reference() {
    use rn_std::strfast::{mask, set_mask, GMPLUS};
    let saved = mask();
    let want: Vec<&str> = EXPECTED.lines().collect();
    for m in [saved | GMPLUS, saved & !GMPLUS] {
        set_mask(m);
        let out = run(&format!("{PRE}{SCRIPT}{POST}"));
        let got: Vec<&str> = out.lines().collect();
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g, w, "STR_FAST {m}: output line {}", i + 1);
        }
        assert_eq!(got.len(), want.len(), "STR_FAST {m}");
    }
    set_mask(saved);
}
