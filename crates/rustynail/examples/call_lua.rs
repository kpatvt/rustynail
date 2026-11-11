//! Calling Lua from Rust: functions, multiple results, tables in and out, keeping a Lua function
//! in the registry, and driving a Lua coroutine (a generator).
//!
//! `cargo run -p rustynail --example call_lua`
use rustynail::{Lua, Resume, MULTRET};

type Outcome = (i64, Vec<i64>, Vec<i64>);

fn run() -> rustynail::Result<Outcome> {
    let mut lua = Lua::new()?;
    lua.do_string(
        r#"
        function stats(t)
          local sum, max = 0, -math.huge
          for _, v in ipairs(t) do sum = sum + v; max = math.max(max, v) end
          return sum, max
        end
        function squares(n)
          local r = {}
          for i = 1, n do r[i] = i * i end
          return r
        end
        function range(n)      -- a generator
          for i = 1, n do coroutine.yield(i) end
        end
        "#,
    )?;

    // stats({4, 8, 15}) -> two results
    lua.get_global("stats")?;
    lua.create_table(3, 0)?;
    for (i, v) in [4, 8, 15].into_iter().enumerate() {
        lua.push_integer(v)?;
        lua.set_i(-2, i as i64 + 1)?;
    }
    lua.call(1, 2)?;
    let sum = lua.to_integer(-2).unwrap_or(0);
    println!("sum = {sum}, max = {:?}", lua.to_integer(-1));
    lua.pop(2)?;

    // squares(5) -> read the returned table by index
    lua.get_global("squares")?;
    lua.push_integer(5)?;
    lua.call(1, 1)?;
    let n = lua.len_integer(-1)?;
    let mut sq = Vec::new();
    for i in 1..=n {
        lua.get_i(-1, i)?;
        sq.push(lua.to_integer(-1).unwrap_or(0));
        lua.pop(1)?;
    }
    lua.pop(1)?;
    println!("squares = {sq:?}");

    // keep a function alive in the registry and call it later
    lua.get_global("stats")?;
    let key = lua.reference()?;
    lua.push_reference(&key)?;
    lua.new_table()?;
    lua.call(1, MULTRET)?;
    lua.set_top(0)?;
    lua.unreference(key)?;

    // a generator: resume a coroutine until it returns
    lua.get_global("range")?;
    lua.create_coroutine()?;
    lua.push_integer(4)?;
    let mut nargs = 1;
    let mut gen = Vec::new();
    while let Resume::Yield(n) = lua.resume(1, nargs)? {
        assert_eq!(n, 1);
        gen.push(lua.to_integer(-1).unwrap_or(0));
        lua.pop(1)?;
        nargs = 0;
    }
    println!("generated = {gen:?}");
    Ok((sum, sq, gen))
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[test]
fn example_runs() {
    let (sum, sq, gen) = run().unwrap();
    assert_eq!(sum, 27);
    assert_eq!(sq, [1, 4, 9, 16, 25]);
    assert_eq!(gen, [1, 2, 3, 4]);
}
