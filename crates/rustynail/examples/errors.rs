//! Error handling: error kinds, messages, tracebacks, error objects that are tables, and errors
//! crossing Rust functions.
//!
//! `cargo run -p rustynail --example errors`
use rustynail::{Error, ErrorKind, Lua};

fn run() -> rustynail::Result<Vec<String>> {
    let mut lua = Lua::new()?;
    let mut out = Vec::new();

    // 1. syntax errors come from `load`
    let e = lua.do_string("x = = 1").unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Syntax);
    out.push(e.to_string());

    // 2. runtime errors carry the position
    let e = lua.do_string("local t = nil; return t.field").unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Runtime);
    out.push(e.to_string());

    // 3. a traceback via a message handler, as the standalone interpreter prints it
    lua.push_traceback_handler()?;
    lua.load_string("local function inner() error('deep failure') end inner()")?;
    if let Err(e) = lua.pcall(0, 0, 1) {
        out.push(e.message_lossy().lines().next().unwrap_or("").to_string());
        assert!(e.message_lossy().contains("stack traceback:"));
    }
    lua.set_top(0)?;

    // 4. error objects that are not strings: pcall leaves the object on the stack
    lua.load_string("error({code = 404, reason = 'not found'})")?;
    if lua.pcall(0, 0, 0).is_err() {
        lua.get_field(-1, "code")?;
        out.push(format!("code {}", lua.to_integer(-1).unwrap_or(0)));
        lua.set_top(0)?;
    }

    // 5. a Rust function fails; Lua code catches it
    lua.push_function(|s| {
        let n = s
            .to_integer(1)
            .ok_or_else(|| Error::runtime("integer expected"))?;
        if n < 0 {
            return Err(Error::runtime(format!("negative input: {n}")));
        }
        s.push_integer(n * 2)?;
        Ok(1)
    })?;
    lua.set_global("double")?;
    lua.do_string("local ok, err = pcall(double, -1); return err")?;
    out.push(lua.to_string_lossy(-1).unwrap_or_default());
    lua.pop(1)?;

    // 6. errors pass through Rust unchanged: Lua -> Rust -> Lua error(table) -> Rust `?` -> Lua
    lua.push_function(|s| {
        s.push_value(1)?;
        s.call(0, 0)?; // the error object of the inner function propagates with `?`
        Ok(0)
    })?;
    lua.set_global("run_callback")?;
    lua.do_string(
        "local ok, err = pcall(run_callback, function() error({retry = true}) end)
         return type(err), err.retry",
    )?;
    out.push(format!(
        "{} {}",
        lua.to_string_lossy(-2).unwrap_or_default(),
        lua.to_boolean(-1)
    ));
    lua.set_top(0)?;
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
    assert!(out[0].contains("unexpected symbol"), "{}", out[0]);
    assert!(
        out[1].contains("attempt to index a nil value (local 't')"),
        "{}",
        out[1]
    );
    assert!(out[2].ends_with("deep failure"), "{}", out[2]);
    assert_eq!(out[3], "code 404");
    assert_eq!(out[4], "negative input: -1");
    assert_eq!(out[5], "table true");
}
