//! L2: the first two keys of a fresh table (rn-core `insert_small`, the short path past `newkey` -> `rehash` -> `resize_hash`). The script
//! `tests/vm_seed0/l2_small_insert_order.lua` is run in-process with seed 0 and `print` captured; its output (the `pairs` order of tables
//! built through the object idiom, constructors with a one-node part, deletions, integer keys, `rawset`) is compared with the reference
//! interpreter's (checked in as `tests/data/l2_small_insert_order.expected`, produced by `ref/55/build-seed0/refrun`). The layout
//! equality itself is a unit test of rn-core (`tests_table::smallins_matches_generic`); this test is the detector of the mutant
//! `smallins_order_swap` (rn-core mutants are applied by `apply_vm_mutants`). 5.5 only: 5.4 keeps the general path.
#![cfg(feature = "trace")]
#![cfg(not(rustynail_lua54))]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;

const SCRIPT: &str = include_str!("../../../tests/vm_seed0/l2_small_insert_order.lua");
const EXPECTED: &str = include_str!("data/l2_small_insert_order.expected");

const PRE: &str = "local __buf = {}\n\
local function print(...) local n = select('#', ...) local t = {} \
for i = 1, n do t[i] = tostring((select(i, ...))) end __buf[#__buf + 1] = table.concat(t, '\\t') end\n";
const POST: &str = "\nreturn table.concat(__buf, '\\n') .. '\\n'";

#[test]
fn smallins_pairs_order_matches_the_reference() {
    let mut st: LuaState = rn_std::new_state(0).expect("state");
    rn_std::open_libs(&mut st).expect("libs");
    #[cfg(feature = "tr_mutants")]
    rn_jit::tr::mutants::apply_vm_mutants();
    let src = format!("{PRE}{SCRIPT}{POST}");
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=l2", None),
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
