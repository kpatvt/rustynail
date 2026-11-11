//! rnlua — the stand-alone interpreter, a port of `lua.c` (5.5; the 5.4 differences are selected
//! with `cfg!(rustynail_lua54)`). See docs/ARCH.md.
//!
//! SPDX-License-Identifier: MIT
//! Copyright (c) 2026 Kiran Pashikanti <kpatvt@gmail.com> (derived from Lua, Copyright (C) 1994-2026 Lua.org,
//! PUC-Rio; see LICENSE and LICENSE-LUA).
//!
//! Extensions to `lua.c`: `--version` (like `-v`) and `--license` (full licence texts and third-party notices).
//!
//! Deviations from the reference: no `readline` support (plain stdin lines, as the reference does
//! without readline); SIGINT sets `rn_core::state::INTERRUPT`, which the VM polls (the reference
//! installs a hook from the handler instead).
use rn_core::state::{State, StkId, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::value::Value;
use std::io::{BufRead, IsTerminal, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};

const LUA54: bool = cfg!(rustynail_lua54);

#[cfg(not(rustynail_lua54))]
const COPYRIGHT: &str = "Lua 5.5.1  Copyright (C) 1994-2026 Lua.org, PUC-Rio";
#[cfg(rustynail_lua54)]
const COPYRIGHT: &str = "Lua 5.4.9  Copyright (C) 1994-2026 Lua.org, PUC-Rio";
#[cfg(not(rustynail_lua54))]
const INIT_VERSION: &str = "LUA_INIT_5_5";
#[cfg(rustynail_lua54)]
const INIT_VERSION: &str = "LUA_INIT_5_4";

#[path = "license.rs"]
mod license;

/// The rustynail line printed after the reference copyright line by `-v` / `--version`.
fn rn_version_line() -> String {
    format!(
        "rnlua {} (Lua {} compatible; JIT: {}) {}",
        env!("CARGO_PKG_VERSION"),
        license::LUA_SERIES,
        jit_status(),
        license::AUTHOR_LINE
    )
}

/// `on` (compiled in, available and not disabled by `RUSTYNAIL_JIT=off`), `off`, or `not built`.
#[cfg(all(any(feature = "jit", feature = "jit_j4"), target_arch = "x86_64"))]
fn jit_status() -> &'static str {
    if std::env::var("RUSTYNAIL_JIT").as_deref() == Ok("off") || !rn_jit::available() {
        "off"
    } else {
        "on"
    }
}
#[cfg(not(all(any(feature = "jit", feature = "jit_j4"), target_arch = "x86_64")))]
fn jit_status() -> &'static str {
    "not built"
}

const PROMPT: &[u8] = b"> ";
const PROMPT2: &[u8] = b">> ";

const HAS_ERROR: u32 = 1;
const HAS_I: u32 = 2;
const HAS_V: u32 = 4;
const HAS_E_LUA: u32 = 8;
const HAS_E_ENV: u32 = 16;
const HAS_LICENSE: u32 = 32;

struct Cli {
    progname: Option<Vec<u8>>,
    use_env: bool,
}

fn write_err(parts: &[&[u8]]) {
    let mut e = std::io::stderr();
    for p in parts {
        let _ = e.write_all(p);
    }
    let _ = e.flush();
}

/// `lua_tostring` on a value: strings and numbers.
fn tostr(v: &Value) -> Option<Vec<u8>> {
    if let Some(s) = v.as_string() {
        return Some(s.as_bytes().to_vec());
    }
    v.as_number()
        .map(|n| rn_core::number::tostring(n).as_bytes().to_vec())
}

fn l_message(pname: Option<&[u8]>, msg: &[u8]) {
    match pname {
        Some(p) => write_err(&[p, b": ", msg, b"\n"]),
        None => write_err(&[msg, b"\n"]),
    }
}

impl Cli {
    /// Print the error message on top of the stack when `status` is not OK; pops it.
    fn report(&self, st: &mut State, status: Status) -> Status {
        if status != Status::Ok {
            let v = st.get(st.top() - 1);
            let msg = tostr(&v).unwrap_or_else(|| b"(error message not a string)".to_vec());
            l_message(self.progname.as_deref(), &msg);
            let t = st.top();
            st.set_top(t - 1);
        }
        status
    }
}

/// The message handler used to run all chunks.
fn msghandler(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    let msg: Vec<u8> = match tostr(&v) {
        Some(m) => m,
        None => {
            if let Some(r) = st.call_meta(&v, b"__tostring")? {
                if r.is_string() {
                    st.push(r);
                    return Ok(1);
                }
            }
            let mut m = b"(error object is a ".to_vec();
            m.extend_from_slice(rn_core::auxlib::typename_of(&v));
            m.extend_from_slice(b" value)");
            m
        }
    };
    let tb = st.traceback(Some(&msg), 1);
    st.push_bytes(&tb)?;
    Ok(1)
}

/// `docall`: the function and `narg` arguments are on top of the stack.
extern "C" fn laction(sig: libc::c_int) {
    // SAFETY: `signal` is async-signal-safe.
    unsafe { libc::signal(sig, libc::SIG_DFL) };
    rn_core::state::request_interrupt();
}

fn docall(st: &mut State, narg: usize, nres: i32) -> LuaResult<Status> {
    let top = st.top();
    let base = top.0 - narg - 1;
    st.check_stack(2)?;
    for i in (base..top.0).rev() {
        let v = st.get(StkId(i));
        st.set(StkId(i + 1), v);
    }
    st.set(StkId(base), Value::native(msghandler));
    st.set_top(top + 1);
    // Ctrl-C interrupts the running script (`laction`): the handler only sets a flag that the VM
    // polls, then restores the default disposition so a second Ctrl-C terminates the process.
    // SAFETY: `laction` is async-signal-safe (an atomic store and `signal`).
    unsafe {
        libc::signal(
            libc::SIGINT,
            laction as extern "C" fn(libc::c_int) as libc::sighandler_t,
        )
    };
    let status = st.pcall(StkId(base + 1), nres, base);
    // SAFETY: restoring the default disposition.
    unsafe { libc::signal(libc::SIGINT, libc::SIG_DFL) };
    rn_core::state::INTERRUPT.store(false, std::sync::atomic::Ordering::Relaxed);
    // remove the message handler
    let t = st.top();
    for i in base + 1..t.0 {
        let v = st.get(StkId(i));
        st.set(StkId(i - 1), v);
    }
    st.set_top(t - 1);
    Ok(status)
}

fn dochunk(cli: &Cli, st: &mut State, status: Status) -> LuaResult<Status> {
    let status = if status == Status::Ok {
        docall(st, 0, 0)?
    } else {
        status
    };
    Ok(cli.report(st, status))
}

fn dofile(cli: &Cli, st: &mut State, name: Option<&[u8]>) -> LuaResult<Status> {
    let mode: Option<&[u8]> = if LUA54 { None } else { Some(b"bt") };
    let s = st.load_filex(name, mode);
    dochunk(cli, st, s)
}

fn dostring(cli: &Cli, st: &mut State, s: &[u8], name: &[u8]) -> LuaResult<Status> {
    let mode: Option<&[u8]> = if LUA54 { None } else { Some(b"t") };
    let status = st.load_chunk(s, name, mode);
    dochunk(cli, st, status)
}

fn global_get(st: &mut State, name: &[u8]) -> LuaResult<Value> {
    let g = Value::table(st.globals());
    st.index_str(g, name)
}

fn global_set(st: &mut State, name: &[u8], v: Value) -> LuaResult<()> {
    let g = Value::table(st.globals());
    st.set_index_str(g, name, v)
}

/// `globname[=modname]`: `globname = require(modname)`.
fn dolibrary(cli: &Cli, st: &mut State, arg: &[u8]) -> LuaResult<Status> {
    let (glob, modname): (Vec<u8>, Vec<u8>) = match arg.iter().position(|&c| c == b'=') {
        Some(i) => (arg[..i].to_vec(), arg[i + 1..].to_vec()),
        None => {
            // no explicit name: a '-' suffix (version) is cut from the global name
            let g = match arg.iter().position(|&c| c == b'-') {
                Some(i) => arg[..i].to_vec(),
                None => arg.to_vec(),
            };
            (g, arg.to_vec())
        }
    };
    let req = global_get(st, b"require")?;
    st.push(req);
    st.push_bytes(&modname)?;
    let status = docall(st, 1, 1)?;
    if status == Status::Ok {
        let v = st.get(st.top() - 1);
        global_set(st, &glob, v)?;
        let t = st.top();
        st.set_top(t - 1);
    }
    Ok(cli.report(st, status))
}

/// Push `arg[1..#arg]`; returns the count or the error status.
fn pushargs(st: &mut State) -> LuaResult<Result<usize, ()>> {
    let a = global_get(st, b"arg")?;
    if !a.is_table() {
        st.push_bytes(b"'arg' is not a table")?;
        return Ok(Err(()));
    }
    let n = st.len_int(a)?;
    st.check_stack_msg(n.max(0) as usize + 3, "too many arguments to script")?;
    for i in 1..=n {
        let v = st.index_int(a, i)?;
        st.push(v);
    }
    Ok(Ok(n.max(0) as usize))
}

fn handle_script(cli: &Cli, st: &mut State, argv: &[Vec<u8>], script: usize) -> LuaResult<Status> {
    let mut fname: Option<&[u8]> = Some(&argv[script]);
    if argv[script] == b"-" && argv[script - 1] != b"--" {
        fname = None;
    }
    let mode: Option<&[u8]> = if LUA54 { None } else { Some(b"bt") };
    let mut status = st.load_filex(fname, mode);
    if status == Status::Ok {
        match pushargs(st)? {
            Ok(n) => status = docall(st, n, LUA_MULTRET)?,
            Err(()) => status = Status::ErrRun,
        }
    }
    Ok(cli.report(st, status))
}

/// `collectargs`: returns (mask, first) like the reference; `first` is the bad argument index on
/// `HAS_ERROR`.
fn collectargs(argv: &[Vec<u8>], progname: &mut Option<Vec<u8>>) -> (u32, i64) {
    let mut args = 0u32;
    if argv.is_empty() {
        return (0, -1);
    }
    if !argv[0].is_empty() {
        *progname = Some(argv[0].clone());
    }
    let mut first: i64;
    let mut i = 1;
    while i < argv.len() {
        first = i as i64;
        let a = &argv[i];
        if a.first() != Some(&b'-') {
            return (args, first);
        }
        match a.get(1) {
            Some(b'-') => {
                // extensions: `--version` (= `-v`) and `--license`
                if a.as_slice() == b"--version" {
                    args |= HAS_V;
                    i += 1;
                    continue;
                }
                if a.as_slice() == b"--license" {
                    args |= HAS_LICENSE;
                    i += 1;
                    continue;
                }
                if a.len() > 2 {
                    return (HAS_ERROR, first);
                }
                first = if i + 1 < argv.len() {
                    (i + 1) as i64
                } else {
                    0
                };
                return (args, first);
            }
            None => return (args, first),
            Some(b'E') => {
                if a.len() > 2 {
                    return (HAS_ERROR, first);
                }
                args |= HAS_E_ENV;
            }
            Some(b'W') => {
                if a.len() > 2 {
                    return (HAS_ERROR, first);
                }
            }
            Some(&c @ (b'i' | b'v')) => {
                if c == b'i' {
                    args |= HAS_I;
                }
                if a.len() > 2 {
                    return (HAS_ERROR, first);
                }
                args |= HAS_V;
            }
            Some(&c @ (b'e' | b'l')) => {
                if c == b'e' {
                    args |= HAS_E_LUA;
                }
                if a.len() == 2 {
                    i += 1;
                    if i >= argv.len() || argv[i].first() == Some(&b'-') {
                        return (HAS_ERROR, first);
                    }
                }
            }
            _ => return (HAS_ERROR, first),
        }
        i += 1;
    }
    (args, 0)
}

fn runargs(cli: &Cli, st: &mut State, argv: &[Vec<u8>], n: usize) -> LuaResult<bool> {
    if !LUA54 {
        st.warning(b"@off", false); // by default, Lua stand-alone has warnings off
    }
    let mut i = 1;
    while i < n {
        let option = argv[i][1];
        match option {
            b'e' | b'l' => {
                let extra: Vec<u8> = if argv[i].len() > 2 {
                    argv[i][2..].to_vec()
                } else {
                    i += 1;
                    argv[i].clone()
                };
                let status = if option == b'e' {
                    dostring(cli, st, &extra, b"=(command line)")?
                } else {
                    dolibrary(cli, st, &extra)?
                };
                if status != Status::Ok {
                    return Ok(false);
                }
            }
            b'W' => st.warning(b"@on", false),
            _ => {}
        }
        i += 1;
    }
    Ok(true)
}

fn getenv(cli: &Cli, name: &str) -> Option<Vec<u8>> {
    if !cli.use_env {
        return None;
    }
    std::env::var_os(name).map(|v| v.into_vec())
}

fn handle_luainit(cli: &Cli, st: &mut State) -> LuaResult<Status> {
    let mut name = INIT_VERSION;
    let mut init = getenv(cli, name);
    if init.is_none() {
        name = "LUA_INIT";
        init = getenv(cli, name);
    }
    match init {
        None => Ok(Status::Ok),
        Some(i) if i.first() == Some(&b'@') => dofile(cli, st, Some(&i[1..])),
        Some(i) => {
            let mut cn = b"=".to_vec();
            cn.extend_from_slice(name.as_bytes());
            dostring(cli, st, &i, &cn)
        }
    }
}

// ---- REPL -----------------------------------------------------------------------------------

fn get_prompt(st: &mut State, firstline: bool) -> LuaResult<Vec<u8>> {
    let v = global_get(st, if firstline { b"_PROMPT" } else { b"_PROMPT2" })?;
    Ok(tostr(&v).unwrap_or_else(|| if firstline { PROMPT } else { PROMPT2 }.to_vec()))
}

/// Prompt, read one line (without the newline); `None` at end of input.
fn read_line(st: &mut State, firstline: bool) -> LuaResult<Option<Vec<u8>>> {
    let prmt = get_prompt(st, firstline)?;
    {
        let mut o = std::io::stdout();
        let _ = o.write_all(&prmt);
        let _ = o.flush();
    }
    let mut buf = Vec::new();
    let n = std::io::stdin()
        .lock()
        .read_until(b'\n', &mut buf)
        .unwrap_or(0);
    if n == 0 {
        return Ok(None);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
    }
    Ok(Some(buf))
}

fn incomplete(st: &State, status: Status) -> bool {
    if status == Status::ErrSyntax {
        if let Some(s) = st.get(st.top() - 1).as_string() {
            return s.as_bytes().ends_with(b"<eof>");
        }
    }
    false
}

fn checklocal(line: &[u8]) {
    let rest: &[u8] = {
        let n = line
            .iter()
            .take_while(|&&c| c == b' ' || c == b'\t')
            .count();
        &line[n..]
    };
    if rest.starts_with(b"local") && matches!(rest.get(5), Some(b' ' | b'\t')) {
        write_err(&[b"warning: locals do not survive across lines in interactive mode\n"]);
    }
}

/// `loadline`: `None` at EOF, otherwise the load status (function or message on top).
fn loadline(st: &mut State, base: StkId) -> LuaResult<Option<Status>> {
    st.set_top(base);
    let Some(mut line) = read_line(st, true)? else {
        return Ok(None);
    };
    if LUA54 && line.first() == Some(&b'=') {
        let mut l = b"return ".to_vec();
        l.extend_from_slice(&line[1..]);
        line = l;
    }
    // try as an expression
    let mut ret = b"return ".to_vec();
    ret.extend_from_slice(&line);
    ret.push(b';');
    let mode: Option<&[u8]> = if LUA54 { None } else { Some(b"t") };
    let s = st.load_chunk(&ret, b"=stdin", mode);
    if s == Status::Ok {
        return Ok(Some(s));
    }
    st.set_top(base);
    if !LUA54 {
        checklocal(&line);
    }
    loop {
        let status = st.load_chunk(&line, b"=stdin", mode);
        if !incomplete(st, status) {
            return Ok(Some(status));
        }
        match read_line(st, false)? {
            None => return Ok(Some(status)),
            Some(more) => {
                st.set_top(base); // remove error message (from incomplete line)
                line.push(b'\n');
                line.extend_from_slice(&more);
            }
        }
    }
}

fn l_print(cli: &Cli, st: &mut State, base: StkId) -> LuaResult<()> {
    let top = st.top();
    let n = top.0 - base.0;
    if n > 0 {
        st.check_stack_msg(rn_core::state::LUA_MINSTACK, "too many results to print")?;
        let p = global_get(st, b"print")?;
        for i in (base.0..top.0).rev() {
            let v = st.get(StkId(i));
            st.set(StkId(i + 1), v);
        }
        st.set(base, p);
        st.set_top(top + 1);
        let status = st.pcall(base, 0, 0);
        if status != Status::Ok {
            let e = tostr(&st.get(st.top() - 1)).unwrap_or_default();
            let mut m = b"error calling 'print' (".to_vec();
            m.extend_from_slice(&e);
            m.push(b')');
            l_message(cli.progname.as_deref(), &m);
        }
    }
    Ok(())
}

fn do_repl(cli: &mut Cli, st: &mut State) -> LuaResult<()> {
    let old = cli.progname.take(); // no progname on errors in interactive mode
    let base = st.top();
    while let Some(mut status) = loadline(st, base)? {
        if status == Status::Ok {
            status = docall(st, 0, LUA_MULTRET)?;
        }
        if status == Status::Ok {
            l_print(cli, st, base)?;
        } else {
            cli.report(st, status);
        }
    }
    st.set_top(base);
    let _ = std::io::stdout().write_all(b"\n");
    cli.progname = old;
    Ok(())
}

fn print_usage(progname: &[u8], bad: &[u8]) {
    let mut m: Vec<u8> = progname.to_vec();
    m.extend_from_slice(b": ");
    if matches!(bad.get(1), Some(b'e' | b'l')) {
        m.push(b'\'');
        m.extend_from_slice(bad);
        m.extend_from_slice(b"' needs argument\n");
    } else {
        m.extend_from_slice(b"unrecognized option '");
        m.extend_from_slice(bad);
        m.extend_from_slice(b"'\n");
    }
    write_err(&[&m]);
    let usage = format!(
        "usage: {} [options] [script [args]]\n\
Available options are:\n\
\x20 -e stat   execute string 'stat'\n\
\x20 -i        enter interactive mode after executing 'script'\n\
\x20 -l mod    require library 'mod' into global 'mod'\n\
\x20 -l g=mod  require library 'mod' into global 'g'\n\
\x20 -v        show version information\n\
\x20 -E        ignore environment variables\n\
\x20 -W        turn warnings on\n\
\x20 --        stop handling options\n\
\x20 -         stop handling options and execute stdin\n",
        String::from_utf8_lossy(progname)
    );
    write_err(&[usage.as_bytes()]);
}

fn createargtable(st: &mut State, argv: &[Vec<u8>], script: i64) -> LuaResult<()> {
    let argc = argv.len() as i64;
    let narg = (argc - (script + 1)).max(0) as usize;
    let t = st.new_table_sized(narg, (script + 1).max(0) as usize)?;
    st.push(Value::table(t));
    for (i, a) in argv.iter().enumerate() {
        st.push_bytes(a)?;
        let v = st.get(st.top() - 1);
        st.raw_set_int(t, i as i64 - script, &v)?;
        let top = st.top();
        st.set_top(top - 1);
    }
    st.raw_set_str(t_globals(st), b"arg", Value::table(t))?;
    let top = st.top();
    st.set_top(top - 1);
    Ok(())
}

fn t_globals(st: &State) -> rn_core::table::TableRef {
    st.globals()
}

/// `pmain`; returns whether everything succeeded.
fn pmain(cli: &mut Cli, st: &mut State, argv: &[Vec<u8>]) -> LuaResult<bool> {
    let (args, script) = collectargs(argv, &mut cli.progname);
    let argc = argv.len() as i64;
    let optlim = if script > 0 { script } else { argc } as usize;
    if args == HAS_ERROR {
        let pn = cli.progname.clone().unwrap_or_else(|| b"lua".to_vec());
        print_usage(&pn, &argv[script as usize]);
        return Ok(false);
    }
    if args & HAS_LICENSE != 0 {
        license::print_license("rnlua");
        return Ok(true);
    }
    if args & HAS_V != 0 {
        license::quiet_println(COPYRIGHT);
        license::quiet_println(&rn_version_line());
    }
    if args & HAS_E_ENV != 0 {
        cli.use_env = false;
        st.registry_set(b"LUA_NOENV", Value::TRUE)?;
    }
    rn_std::open_libs(st)?;
    // LuaJIT-compatible extensions (docs/EXTENSIONS.md): table.new/clear, string.buffer (require), the `jit` table
    rn_std::lib_ext::open_ext(st)?;
    rn_std::lib_ext::open_jit(st)?;
    createargtable(st, argv, script)?;
    // start the GC in generational mode, like lua.c (RUSTYNAIL_GC_MODE=inc keeps incremental)
    let inc = std::env::var("RUSTYNAIL_GC_MODE").as_deref() == Ok("inc");
    for opt in [&b"restart"[..], b"generational"] {
        if inc && opt == b"generational" {
            continue;
        }
        if let Ok(f) = global_get(st, b"collectgarbage") {
            if f.is_function() {
                if let Ok(a) = st.new_str_value(opt) {
                    let _ = st.pcall_value(f, &[a], 0);
                }
            }
        }
    }
    if LUA54 {
        if args & HAS_E_ENV == 0 && handle_luainit(cli, st)? != Status::Ok {
            return Ok(false);
        }
    } else if handle_luainit(cli, st)? != Status::Ok {
        return Ok(false);
    }
    if !runargs(cli, st, argv, optlim)? {
        return Ok(false);
    }
    if script > 0 && handle_script(cli, st, argv, script as usize)? != Status::Ok {
        return Ok(false);
    }
    if args & HAS_I != 0 {
        do_repl(cli, st)?;
    } else if script < 1 && args & (HAS_E_LUA | HAS_V) == 0 {
        if std::io::stdin().is_terminal() {
            license::quiet_println(COPYRIGHT);
            do_repl(cli, st)?;
        } else {
            dofile(cli, st, None)?;
        }
    }
    Ok(true)
}

static ARGV: std::sync::OnceLock<Vec<Vec<u8>>> = std::sync::OnceLock::new();

thread_local! {
    static CLI: std::cell::RefCell<Cli> = const {
        std::cell::RefCell::new(Cli { progname: None, use_env: true })
    };
}

/// The interpreter body runs as a native function called through `pcall` (like `pmain` in lua.c),
/// so tracebacks end with the same `[C]: in ?` frame as the reference.
fn pmain_native(st: &mut State) -> LuaResult<usize> {
    let argv = ARGV.get().expect("argv set in main");
    let ok = CLI.with(|c| {
        let mut cli = c.borrow_mut();
        cli.progname = Some(b"lua".to_vec());
        pmain(&mut cli, st, argv)
    })?;
    st.push(Value::boolean(ok));
    Ok(1)
}

fn main() {
    // keep the whole C API linked (and exported by `-rdynamic`) for the C modules we load
    {
        std::hint::black_box(rn_capi::exported_symbols());
        rn_capi::ensure_init(); // let Lua call C functions from loaded modules
    }
    let argv: Vec<Vec<u8>> = std::env::args_os().map(|a| a.as_bytes().to_vec()).collect();
    let _ = ARGV.set(argv.clone());
    // RUSTYNAIL_SEED pins the string-hash seed (differential testing against `refrun`)
    let seed = std::env::var("RUSTYNAIL_SEED")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or_else(rn_core::auxlib::make_seed);
    #[cfg(all(any(feature = "jit", feature = "jit_j4"), target_arch = "x86_64"))]
    rn_jit::register();
    let mut state = match rn_std::new_state(seed) {
        Ok(s) => s,
        Err(_) => {
            let n = argv.first().map(|v| v.as_slice());
            l_message(n, b"cannot create state: not enough memory");
            std::process::exit(1);
        }
    };
    state.install_warn_function(!LUA54);
    let func = state.top();
    state.push(Value::native(pmain_native));
    let status = state.pcall(func, 1, 0);
    let result = state.get(state.top() - 1).is_truthy();
    let progname = CLI.with(|c| c.borrow().progname.clone());
    let cli = Cli {
        progname: progname.or_else(|| Some(b"lua".to_vec())),
        use_env: true,
    };
    cli.report(&mut state, status);
    drop(state);
    std::process::exit(if result && status == Status::Ok { 0 } else { 1 });
}
