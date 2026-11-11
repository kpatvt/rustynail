//! The smallest embedding: create a state, run a chunk, read a result.
//!
//! `cargo run -p rustynail --example hello`
use rustynail::Lua;

fn run() -> rustynail::Result<String> {
    let mut lua = Lua::new()?;
    lua.do_string("print('hello from ' .. _VERSION)")?;
    let n = lua.do_string("return ('hello'):rep(3, ' ')")?;
    assert_eq!(n, 1);
    let s = lua.to_string_lossy(-1).unwrap_or_default();
    lua.pop(1)?;
    Ok(s)
}

fn main() {
    match run() {
        Ok(s) => println!("{s}"),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

#[test]
fn example_runs() {
    assert_eq!(run().unwrap(), "hello hello hello");
}
