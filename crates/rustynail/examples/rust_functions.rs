//! Calling Rust from Lua: global functions, a module table, closures that capture Rust state
//! and Lua upvalues.
//!
//! `cargo run -p rustynail --example rust_functions`
use rustynail::{Error, Lua, Stack};
use std::cell::RefCell;
use std::rc::Rc;

/// `mathx.clamp(x, lo, hi)`: argument checking in the style of `luaL_check*`.
fn clamp(s: &mut dyn Stack) -> rustynail::Result<usize> {
    let arg = |s: &mut dyn Stack, i: i32| {
        s.to_number(i).ok_or_else(|| {
            Error::runtime(format!(
                "bad argument #{i} to 'clamp' (number expected, got {})",
                s.type_name(i)
            ))
        })
    };
    let (x, lo, hi) = (arg(s, 1)?, arg(s, 2)?, arg(s, 3)?);
    s.push_number(x.max(lo).min(hi))?;
    Ok(1)
}

fn run() -> rustynail::Result<Vec<String>> {
    let mut lua = Lua::new()?;

    // a module table with Rust functions
    lua.new_table()?;
    lua.push_function(clamp)?;
    lua.set_field(-2, "clamp")?;
    lua.push_function(|s| {
        // any number of results: split a string on commas
        let text = s.to_bytes(1).unwrap_or_default();
        let parts: Vec<&[u8]> = text.split(|&b| b == b',').collect();
        for p in &parts {
            s.push_bytes(p)?;
        }
        Ok(parts.len())
    })?;
    lua.set_field(-2, "split")?;
    lua.set_global("mathx")?;

    // a closure capturing Rust state: Lua appends to a Rust vector
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    lua.push_function(move |s| {
        let line = s.tostring(1)?; // `tostring` semantics, metamethods included
        sink.borrow_mut()
            .push(String::from_utf8_lossy(&line).into_owned());
        Ok(0)
    })?;
    lua.set_global("log")?;

    // a closure with a Lua upvalue (lua_pushcclosure): a counter that lives in Lua
    lua.push_integer(0)?;
    lua.push_closure(
        |s| {
            s.push_upvalue(1)?;
            let n = s.to_integer(-1).unwrap_or(0) + 1;
            s.push_integer(n)?;
            s.replace_upvalue(1)?;
            s.push_integer(n)?;
            Ok(1)
        },
        1,
    )?;
    lua.set_global("next_id")?;

    lua.do_string(
        r#"
        log(mathx.clamp(15, 0, 10))
        log(select('#', mathx.split("a,b,c")))
        log(next_id() + next_id())          -- 1 + 2
        local ok, err = pcall(mathx.clamp, "x", 0, 1)
        log(err)
        "#,
    )?;
    let out = log.borrow().clone();
    for line in &out {
        println!("{line}");
    }
    Ok(out)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[test]
fn example_runs() {
    let out = run().unwrap();
    assert_eq!(out[0], "10.0");
    assert_eq!(out[1], "3");
    assert_eq!(out[2], "3");
    assert_eq!(
        out[3],
        "bad argument #1 to 'clamp' (number expected, got string)"
    );
}
