//! N2: the bulk element move behind positional `table.insert` / `table.remove` / `table.move` (rn-core
//! `TableRef::arr_move`). The script `tests/vm/tables_09_insert_remove_move.lua` is run in-process with `print` captured and its
//! output compared with the reference interpreter's (checked in as `tests/data/n2_table_move.expected`).
//! Detector of the mutant `tbl_move_meta_skip` (the bulk move ignores the metatable guard).
#![cfg(feature = "trace")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;

const SCRIPT: &str = include_str!("../../../tests/vm/tables_09_insert_remove_move.lua");
#[cfg(not(rustynail_lua54))]
const EXPECTED: &str = include_str!("data/n2_table_move.expected");
#[cfg(rustynail_lua54)]
const EXPECTED: &str = include_str!("data/n2_table_move54.expected");

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
        st.load_buffer(src.as_bytes(), b"=n2", None),
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
fn bulk_move_honours_metamethods() {
    let out = run(&format!("{PRE}{SCRIPT}{POST}"));
    let (got, want): (Vec<&str>, Vec<&str>) = (out.lines().collect(), EXPECTED.lines().collect());
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "output line {}", i + 1);
    }
    assert_eq!(got.len(), want.len());
}
