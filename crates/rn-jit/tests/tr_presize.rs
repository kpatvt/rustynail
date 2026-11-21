//! P1 (`presize`, docs/JIT_TRACE.md section 32): the script `tests/vm_seed0/p1_presize_order.lua` (insertion groups of 2..9 constant keys
//! into fresh tables, the object idiom, a guard inside a group, metatables changed between the stores, a table that is its own metatable,
//! nil and dynamic keys inside a group, growth past the presized part) run in-process by the interpreter with seed 0 and `print`
//! captured: its output (`pairs` order and borders of every table) equals the reference interpreter's (checked in as
//! `tests/data/p1_presize_order.expected`, produced by `ref/55/build-seed0/lua`; `ref/54/build-seed0/lua` prints the same). The traced
//! runs of the same script are compared with this interpreter output by `tr_core::p1_presize_programs`, so together the traced layouts
//! are the reference's.
#![cfg(feature = "trace")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;

const SCRIPT: &str = include_str!("../../../tests/vm_seed0/p1_presize_order.lua");
const EXPECTED: &str = include_str!("data/p1_presize_order.expected");

const PRE: &str = "local __buf = {}\n\
local function print(...) local n = select('#', ...) local t = {} \
for i = 1, n do t[i] = tostring((select(i, ...))) end __buf[#__buf + 1] = table.concat(t, '\\t') end\n";
const POST: &str = "\nreturn table.concat(__buf, '\\n') .. '\\n'";

#[test]
fn presize_pairs_order_matches_the_reference() {
    let mut st: LuaState = rn_std::new_state(0).expect("state");
    rn_std::open_libs(&mut st).expect("libs");
    #[cfg(feature = "tr_mutants")]
    rn_jit::tr::mutants::apply_vm_mutants();
    let src = format!("{PRE}{SCRIPT}{POST}");
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=p1", None),
        Status::Ok,
        "load"
    );
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    assert_eq!(status, Status::Ok, "script failed: {v:?}");
    let out =
        String::from_utf8_lossy(v.as_string().expect("string result").as_bytes()).into_owned();
    st.set_top(base);
    let (got, want): (Vec<&str>, Vec<&str>) = (out.lines().collect(), EXPECTED.lines().collect());
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "output line {}", i + 1);
    }
    assert_eq!(got.len(), want.len());
}
