//! `io` library (port of `liolib.c`). Files are C `FILE*` behind [`crate::io_ffi`]; the userdata
//! payload is a `luaL_Stream`-compatible [`LStream`] (`closef == None` means "closed").
#![forbid(unsafe_code)]
#![allow(clippy::needless_late_init, clippy::explicit_counter_loop)]

use crate::io_ffi::{self, errno, read_stream, set_errno, write_stream, CFile, LStream};
use rn_core::auxlib::{strerror, LUA_FILEHANDLE, MAX_SIZE};
use rn_core::number;
use rn_core::state::{NativeFn, State, LUA_MINSTACK};
use rn_core::status::{LuaResult, Status};
use rn_core::table::TableRef;
use rn_core::udata::Udata;
use rn_core::value::{Gc, LuaType, Value};

const IO_INPUT: &[u8] = b"_IO_input";
const IO_OUTPUT: &[u8] = b"_IO_output";
/// `LUAL_BUFFERSIZE` (16 * sizeof(void*) * sizeof(lua_Number)).
const LUAL_BUFFERSIZE: usize = 16 * 8 * 8;
/// Maximum number of arguments to `f:lines` / `io.lines`.
const MAXARGLINE: usize = 250;
/// Maximum length of a numeral read by `read("n")`.
const L_MAXLENNUM: usize = 200;

// ---- luaL_fileresult / luaL_execresult ------------------------------------------------------

/// `luaL_fileresult`.
pub fn file_result(st: &mut State, ok: bool, fname: Option<&[u8]>) -> LuaResult<usize> {
    let en = errno();
    if ok {
        st.push(Value::TRUE);
        return Ok(1);
    }
    st.push(Value::NIL);
    let msg: Vec<u8> = if en != 0 {
        strerror(en).into_bytes()
    } else {
        b"(no extra info)".to_vec()
    };
    let full = match fname {
        Some(f) => {
            let mut m = f.to_vec();
            m.extend_from_slice(b": ");
            m.extend_from_slice(&msg);
            m
        }
        None => msg,
    };
    st.push_bytes(&full)?;
    st.push(Value::int(en as i64));
    Ok(3)
}

/// `luaL_execresult`.
pub fn exec_result(st: &mut State, stat: i32) -> LuaResult<usize> {
    if stat != 0 && errno() != 0 {
        return file_result(st, false, None);
    }
    let mut what = "exit";
    let mut stat = stat;
    if libc::WIFEXITED(stat) {
        stat = libc::WEXITSTATUS(stat);
    } else if libc::WIFSIGNALED(stat) {
        stat = libc::WTERMSIG(stat);
        what = "signal";
    }
    if what == "exit" && stat == 0 {
        st.push(Value::TRUE);
    } else {
        st.push(Value::NIL);
    }
    st.push_str(what)?;
    st.push(Value::int(stat as i64));
    Ok(3)
}

// ---- stream handles --------------------------------------------------------------------------

fn test_file(st: &mut State, v: &Value) -> Option<Gc<Udata>> {
    let u = v.as_userdata()?;
    let mt = u.metatable()?;
    let want = st.registry_get(LUA_FILEHANDLE).ok()?;
    if Value::table(mt).raw_equal(&want) && u.len() >= std::mem::size_of::<LStream>() {
        Some(u)
    } else {
        None
    }
}

/// `tolstream`: argument `n` must be a file handle.
fn check_file(st: &mut State, n: usize) -> LuaResult<Gc<Udata>> {
    if n <= st.nargs() {
        let v = st.arg(n - 1);
        if let Some(u) = test_file(st, &v) {
            return Ok(u);
        }
    }
    Err(st.type_error_arg(n, LUA_FILEHANDLE))
}

/// `tofile` (argument 1): an open file.
fn tofile(st: &mut State) -> LuaResult<CFile> {
    let u = check_file(st, 1)?;
    let s = read_stream(u);
    if s.closef.is_none() {
        return Err(st.l_error(b"attempt to use a closed file"));
    }
    Ok(CFile(s.f))
}

/// `newprefile`: a closed handle, pushed.
fn new_prefile(st: &mut State) -> LuaResult<Gc<Udata>> {
    let u = st.new_udata(std::mem::size_of::<LStream>(), 0)?;
    st.push(Value::userdata(u));
    let mt = st.registry_get(LUA_FILEHANDLE)?.as_table();
    st.set_metatable(&Value::userdata(u), mt);
    st.check_gc();
    Ok(u)
}

fn new_file(st: &mut State) -> LuaResult<Gc<Udata>> {
    let u = new_prefile(st)?;
    write_stream(
        u,
        LStream {
            f: std::ptr::null_mut(),
            closef: Some(io_fclose),
        },
    );
    Ok(u)
}

fn aux_close(st: &mut State) -> LuaResult<usize> {
    let u = check_file(st, 1)?;
    let mut s = read_stream(u);
    let cf = s.closef;
    s.closef = None;
    write_stream(u, s);
    match cf {
        Some(cf) => {
            let p = cf as usize;
            let ours = [io_fclose as NativeFn, io_pclose, io_noclose].map(|f| f as usize);
            if ours.contains(&p) {
                cf(st) // one of ours
            } else {
                // a `closef` installed by a C module (`luaL_Stream`): a `lua_CFunction`
                let file = st.arg(0);
                let func = st.top();
                st.call_value(
                    Value::light_cfunction(p as *const ()),
                    &[file],
                    rn_core::state::LUA_MULTRET,
                )?;
                Ok(st.top() - func)
            }
        }
        None => Ok(0),
    }
}

fn f_close(st: &mut State) -> LuaResult<usize> {
    tofile(st)?;
    aux_close(st)
}

fn io_close(st: &mut State) -> LuaResult<usize> {
    if st.nargs() == 0 {
        let v = st.registry_get(IO_OUTPUT)?;
        st.push(v);
    }
    f_close(st)
}

fn f_gc(st: &mut State) -> LuaResult<usize> {
    let u = check_file(st, 1)?;
    let s = read_stream(u);
    if s.closef.is_some() && !s.f.is_null() {
        aux_close(st)?;
    }
    Ok(0)
}

fn io_fclose(st: &mut State) -> LuaResult<usize> {
    let u = check_file(st, 1)?;
    let s = read_stream(u);
    set_errno(0);
    let ok = CFile(s.f).close();
    file_result(st, ok, None)
}

fn io_pclose(st: &mut State) -> LuaResult<usize> {
    let u = check_file(st, 1)?;
    let s = read_stream(u);
    set_errno(0);
    let r = CFile(s.f).pclose();
    exec_result(st, r)
}

fn io_noclose(st: &mut State) -> LuaResult<usize> {
    let u = check_file(st, 1)?;
    let mut s = read_stream(u);
    s.closef = Some(io_noclose);
    write_stream(u, s);
    st.push(Value::NIL);
    st.push_str("cannot close standard file")?;
    Ok(2)
}

fn io_type(st: &mut State) -> LuaResult<usize> {
    let v = st.check_any(1)?;
    match test_file(st, &v) {
        None => st.push(Value::NIL),
        Some(u) => {
            let closed = read_stream(u).closef.is_none();
            st.push_str(if closed { "closed file" } else { "file" })?;
        }
    }
    Ok(1)
}

fn f_tostring(st: &mut State) -> LuaResult<usize> {
    let u = check_file(st, 1)?;
    let s = read_stream(u);
    if s.closef.is_none() {
        st.push_str("file (closed)")?;
    } else {
        st.push_bytes(format!("file ({:p})", s.f).as_bytes())?;
    }
    Ok(1)
}

fn l_checkmode(m: &[u8]) -> bool {
    let mut i = 0;
    if m.is_empty() || !b"rwa".contains(&m[0]) {
        return false;
    }
    i += 1;
    if m.get(i) == Some(&b'+') {
        i += 1;
    }
    m[i..].iter().all(|&c| c == b'b')
}

fn io_open(st: &mut State) -> LuaResult<usize> {
    let filename = st.check_str(1)?.get().as_bytes();
    let mode: &[u8] = match st.opt_str(2)? {
        Some(s) => s.get().as_bytes(),
        None => b"r",
    };
    let u = new_file(st)?;
    st.arg_check(l_checkmode(mode), 2, b"invalid mode")?;
    set_errno(0);
    match CFile::open(filename, mode) {
        None => file_result(st, false, Some(filename)),
        Some(f) => {
            write_stream(
                u,
                LStream {
                    f: f.0,
                    closef: Some(io_fclose),
                },
            );
            Ok(1)
        }
    }
}

fn io_popen(st: &mut State) -> LuaResult<usize> {
    let filename = st.check_str(1)?.get().as_bytes();
    let mode: &[u8] = match st.opt_str(2)? {
        Some(s) => s.get().as_bytes(),
        None => b"r",
    };
    let u = new_prefile(st)?;
    st.arg_check(
        (mode == b"r" || mode == b"w") && mode.len() == 1,
        2,
        b"invalid mode",
    )?;
    set_errno(0);
    let f = CFile::popen(filename, mode);
    write_stream(
        u,
        LStream {
            f: f.map_or(std::ptr::null_mut(), |f| f.0),
            closef: Some(io_pclose),
        },
    );
    if f.is_none() {
        return file_result(st, false, Some(filename));
    }
    Ok(1)
}

fn io_tmpfile(st: &mut State) -> LuaResult<usize> {
    let u = new_file(st)?;
    set_errno(0);
    match CFile::tmpfile() {
        None => file_result(st, false, None),
        Some(f) => {
            write_stream(
                u,
                LStream {
                    f: f.0,
                    closef: Some(io_fclose),
                },
            );
            Ok(1)
        }
    }
}

/// `opencheck`: open `fname` or raise; the handle is pushed.
fn opencheck(st: &mut State, fname: &[u8], mode: &[u8]) -> LuaResult<Value> {
    let u = new_file(st)?;
    match CFile::open(fname, mode) {
        Some(f) => {
            write_stream(
                u,
                LStream {
                    f: f.0,
                    closef: Some(io_fclose),
                },
            );
            Ok(Value::userdata(u))
        }
        None => {
            let mut m = b"cannot open file '".to_vec();
            m.extend_from_slice(fname);
            m.extend_from_slice(b"' (");
            m.extend_from_slice(strerror(errno()).as_bytes());
            m.push(b')');
            Err(st.l_error(&m))
        }
    }
}

/// `getiofile`: the default file stored under `key` (also returns its value).
fn getiofile(st: &mut State, key: &[u8]) -> LuaResult<(Value, CFile)> {
    let v = st.registry_get(key)?;
    st.push(v);
    let closed_msg = |st: &mut State| {
        let mut m = b"default ".to_vec();
        m.extend_from_slice(&key[4..]);
        m.extend_from_slice(b" file is closed");
        st.l_error(&m)
    };
    match v.as_userdata() {
        Some(u) if u.len() >= std::mem::size_of::<LStream>() => {
            let s = read_stream(u);
            if s.closef.is_none() {
                return Err(closed_msg(st));
            }
            Ok((v, CFile(s.f)))
        }
        _ => Err(closed_msg(st)),
    }
}

fn g_iofile(st: &mut State, key: &[u8], mode: &[u8]) -> LuaResult<usize> {
    if !st.arg_is_none_or_nil(1) {
        let a = st.arg(0);
        let fname: Option<Vec<u8>> = if let Some(s) = a.as_string() {
            Some(s.as_bytes().to_vec())
        } else {
            a.as_number()
                .map(|n| number::tostring(n).as_bytes().to_vec())
        };
        let v = match fname {
            Some(f) => opencheck(st, &f, mode)?,
            None => {
                tofile(st)?;
                a
            }
        };
        st.registry_set(key, v)?;
    }
    let v = st.registry_get(key)?;
    st.push(v);
    Ok(1)
}

fn io_input(st: &mut State) -> LuaResult<usize> {
    g_iofile(st, IO_INPUT, b"r")
}
fn io_output(st: &mut State) -> LuaResult<usize> {
    g_iofile(st, IO_OUTPUT, b"w")
}

// ---- lines ------------------------------------------------------------------------------------

/// `aux_lines`: pushes the iteration closure over `file` + the formats in args 2...
fn aux_lines(st: &mut State, file: Value, toclose: bool) -> LuaResult<()> {
    let n = st.nargs().saturating_sub(1);
    st.arg_check(n <= MAXARGLINE, MAXARGLINE + 2, b"too many arguments")?;
    let cl = st.try_alloc(|h| h.new_cclosure(io_readline, 3 + n))?;
    st.push(Value::cclosure(cl));
    cl.set_upvalue(st.heap(), 0, file);
    cl.set_upvalue(st.heap(), 1, Value::int(n as i64));
    cl.set_upvalue(st.heap(), 2, Value::boolean(toclose));
    for i in 0..n {
        let f = st.arg(1 + i);
        cl.set_upvalue(st.heap(), 3 + i, f);
    }
    st.check_gc();
    Ok(())
}

fn f_lines(st: &mut State) -> LuaResult<usize> {
    tofile(st)?;
    let f = st.arg(0);
    aux_lines(st, f, false)?;
    Ok(1)
}

fn io_lines(st: &mut State) -> LuaResult<usize> {
    let toclose;
    let file;
    if st.arg_is_none_or_nil(1) {
        file = st.registry_get(IO_INPUT)?;
        // tofile on the default input
        match test_file(st, &file) {
            Some(u) if read_stream(u).closef.is_some() => {}
            _ => return Err(st.l_error(b"attempt to use a closed file")),
        }
        toclose = false;
    } else {
        let filename = st.check_str(1)?.get().as_bytes();
        file = opencheck(st, filename, b"r")?;
        // lua_replace(L, 1): the handle takes the place of the file name
        let base = st.ci().r().func + 1;
        st.set(base, file);
        st.pop();
        toclose = true;
    }
    aux_lines(st, file, toclose)?;
    if toclose {
        st.push(Value::NIL);
        st.push(Value::NIL);
        st.push(file);
        Ok(4)
    } else {
        Ok(1)
    }
}

// ---- reading ----------------------------------------------------------------------------------

struct Rn {
    f: CFile,
    c: i32,
    buff: Vec<u8>,
    overflow: bool,
}

impl Rn {
    fn nextc(&mut self) -> bool {
        if self.buff.len() >= L_MAXLENNUM {
            self.buff.clear();
            self.overflow = true;
            false
        } else {
            self.buff.push(self.c as u8);
            self.c = self.f.getc_unlocked();
            true
        }
    }
    fn test2(&mut self, set: &[u8; 2]) -> bool {
        if self.c == set[0] as i32 || self.c == set[1] as i32 {
            self.nextc()
        } else {
            false
        }
    }
    fn readdigits(&mut self, hex: bool) -> usize {
        let mut count = 0;
        while self.c >= 0
            && (if hex {
                (self.c as u8).is_ascii_hexdigit()
            } else {
                (self.c as u8).is_ascii_digit()
            })
            && self.nextc()
        {
            count += 1;
        }
        count
    }
}

fn read_number(st: &mut State, f: CFile) -> LuaResult<bool> {
    let mut rn = Rn {
        f,
        c: 0,
        buff: Vec::new(),
        overflow: false,
    };
    let mut count = 0;
    let mut hex = false;
    f.lock();
    loop {
        rn.c = f.getc_unlocked();
        if !(rn.c >= 0 && number::is_space(rn.c as u8)) {
            break;
        }
    }
    rn.test2(b"-+");
    if rn.test2(b"00") {
        if rn.test2(b"xX") {
            hex = true;
        } else {
            count = 1;
        }
    }
    count += rn.readdigits(hex);
    if rn.test2(b"..") {
        count += rn.readdigits(hex);
    }
    if count > 0 && rn.test2(if hex { b"pP" } else { b"eE" }) {
        rn.test2(b"-+");
        rn.readdigits(false);
    }
    f.ungetc(rn.c);
    f.unlock();
    let _ = rn.overflow;
    match number::str2num(&rn.buff) {
        Some(n) => {
            st.push(Value::number(n));
            Ok(true)
        }
        None => {
            st.push(Value::NIL);
            Ok(false)
        }
    }
}

fn test_eof(st: &mut State, f: CFile) -> LuaResult<bool> {
    let c = f.getc();
    f.ungetc(c);
    st.push_str("")?;
    Ok(c != -1)
}

fn read_line(st: &mut State, f: CFile, chop: bool) -> LuaResult<bool> {
    let mut buf: Vec<u8> = Vec::new();
    f.lock();
    let mut c;
    loop {
        c = f.getc_unlocked();
        if c == -1 || c == b'\n' as i32 {
            break;
        }
        buf.push(c as u8);
    }
    f.unlock();
    if !chop && c == b'\n' as i32 {
        buf.push(b'\n');
    }
    let nonempty = c == b'\n' as i32 || !buf.is_empty();
    st.push_vec(buf)?;
    Ok(nonempty)
}

fn read_all(st: &mut State, f: CFile) -> LuaResult<()> {
    let mut out: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 8192];
    loop {
        let nr = f.read(&mut chunk);
        out.extend_from_slice(&chunk[..nr]);
        if nr < chunk.len() {
            break;
        }
    }
    st.push_vec(out)?;
    Ok(())
}

fn read_chars(st: &mut State, f: CFile, n: usize) -> LuaResult<bool> {
    if n >= MAX_SIZE {
        #[cfg(not(rustynail_lua54))]
        return Err(st.l_error(b"resulting string too large"));
        #[cfg(rustynail_lua54)]
        return Err(st.l_error(b"buffer too large"));
    }
    // `luaL_prepbuffsize(&b, n)` allocates the whole request up front, so absurd counts fail with
    // "not enough memory" instead of returning a short read.
    let mut out: Vec<u8> = Vec::new();
    if out.try_reserve_exact(n).is_err() {
        return Err(Status::ErrMem);
    }
    let mut chunk = vec![0u8; n.min(65536)];
    let mut remaining = n;
    while remaining > 0 {
        let want = remaining.min(chunk.len());
        let nr = f.read(&mut chunk[..want]);
        out.extend_from_slice(&chunk[..nr]);
        remaining -= nr;
        if nr < want {
            break;
        }
    }
    let nonempty = !out.is_empty();
    st.push_vec(out)?;
    Ok(nonempty)
}

/// `g_read`: formats are the `nargs` arguments starting at argument number `first`.
fn g_read(st: &mut State, f: CFile, first: usize, nargs: usize) -> LuaResult<usize> {
    f.clearerr();
    set_errno(0);
    let mut success;
    let mut n = first;
    if nargs == 0 {
        success = read_line(st, f, true)?;
        n = first + 1;
    } else {
        st.check_stack_msg(nargs + LUA_MINSTACK, "too many arguments")?;
        success = true;
        let mut left = nargs;
        while left > 0 && success {
            left -= 1;
            let a = st.arg(n - 1);
            if a.type_of() == LuaType::Number {
                let l = st.check_integer(n)? as usize;
                success = if l == 0 {
                    test_eof(st, f)?
                } else {
                    read_chars(st, f, l)?
                };
            } else {
                let s = st.check_str(n)?.get().as_bytes();
                let mut p = s;
                if p.first() == Some(&b'*') {
                    p = &p[1..];
                }
                match p.first() {
                    Some(b'n') => success = read_number(st, f)?,
                    Some(b'l') => success = read_line(st, f, true)?,
                    Some(b'L') => success = read_line(st, f, false)?,
                    Some(b'a') => {
                        read_all(st, f)?;
                        success = true;
                    }
                    _ => return Err(st.arg_error(n, b"invalid format")),
                }
            }
            n += 1;
        }
    }
    if f.error() {
        return file_result(st, false, None);
    }
    if !success {
        st.pop();
        st.push(Value::NIL);
    }
    Ok(n - first)
}

fn io_read(st: &mut State) -> LuaResult<usize> {
    let nargs = st.nargs();
    let (_, f) = getiofile(st, IO_INPUT)?;
    g_read(st, f, 1, nargs)
}

fn f_read(st: &mut State) -> LuaResult<usize> {
    let f = tofile(st)?;
    let nargs = st.nargs() - 1;
    g_read(st, f, 2, nargs)
}

fn io_readline(st: &mut State) -> LuaResult<usize> {
    let func = st.ci().r().func;
    let Some(cl) = st.get(func).as_cclosure() else {
        return Err(st.l_error(b"file is already closed"));
    };
    let fv = cl.upvalue(0);
    let n = cl.upvalue(1).as_int().unwrap_or(0) as usize;
    let s = match fv.as_userdata() {
        Some(u) => read_stream(u),
        None => return Err(st.l_error(b"file is already closed")),
    };
    if s.closef.is_none() {
        return Err(st.l_error(b"file is already closed"));
    }
    // lua_settop(L, 1)
    let base = func + 1;
    if st.top() < base + 1 {
        st.set(base, Value::NIL);
    }
    st.set_top(base + 1);
    st.check_stack_msg(n, "too many arguments")?;
    for i in 0..n {
        st.push(cl.upvalue(3 + i));
    }
    let nres = g_read(st, CFile(s.f), 2, n)?;
    let first = st.get(st.top() - nres);
    if first.is_truthy() {
        return Ok(nres);
    }
    if nres > 1 {
        let m = st.get(st.top() - nres + 1);
        let msg = m
            .as_string()
            .map(|s| s.as_bytes().to_vec())
            .unwrap_or_default();
        return Err(st.l_error(&msg));
    }
    if cl.upvalue(2).is_truthy() {
        st.set_top(base);
        st.push(fv);
        aux_close(st)?;
    }
    Ok(0)
}

// ---- writing ----------------------------------------------------------------------------------

fn g_write(st: &mut State, f: CFile, arg: usize, nargs: usize, ret: Value) -> LuaResult<usize> {
    let mut total: usize = 0;
    #[allow(unused_mut)]
    let mut status = true;
    set_errno(0);
    let mut a = arg;
    for _ in 0..nargs {
        let v = st.arg(a - 1);
        let owned: Vec<u8>;
        let bytes: &[u8] = if let Some(num) = v.as_number() {
            #[cfg(rustynail_lua54)]
            {
                owned = match num {
                    number::Number::Int(i) => number::tostring_int(i).as_bytes().to_vec(),
                    number::Number::Float(x) => number::fmt_g(x, 14).as_bytes().to_vec(),
                };
            }
            #[cfg(not(rustynail_lua54))]
            {
                owned = number::tostring(num).as_bytes().to_vec();
            }
            &owned
        } else {
            st.check_str(a)?.get().as_bytes()
        };
        if status {
            let nb = f.write(bytes);
            total += nb;
            if nb < bytes.len() {
                #[cfg(not(rustynail_lua54))]
                {
                    let n = file_result(st, false, None)?;
                    st.push(Value::int(total as i64));
                    return Ok(n + 1);
                }
                #[cfg(rustynail_lua54)]
                {
                    status = false;
                }
            }
        }
        a += 1;
    }
    let _ = total;
    if status {
        st.push(ret);
        Ok(1)
    } else {
        file_result(st, false, None)
    }
}

fn io_write(st: &mut State) -> LuaResult<usize> {
    let nargs = st.nargs();
    let (v, f) = getiofile(st, IO_OUTPUT)?;
    g_write(st, f, 1, nargs, v)
}

fn f_write(st: &mut State) -> LuaResult<usize> {
    let f = tofile(st)?;
    let v = st.arg(0);
    let nargs = st.nargs() - 1;
    g_write(st, f, 2, nargs, v)
}

fn f_seek(st: &mut State) -> LuaResult<usize> {
    let f = tofile(st)?;
    let op = st.check_option(2, Some("cur"), &["set", "cur", "end"])?;
    let p3 = st.opt_integer(3, 0)?;
    set_errno(0);
    let whence = [libc::SEEK_SET, libc::SEEK_CUR, libc::SEEK_END][op];
    if !f.seek(p3, whence) {
        return file_result(st, false, None);
    }
    st.push(Value::int(f.tell()));
    Ok(1)
}

fn f_setvbuf(st: &mut State) -> LuaResult<usize> {
    let f = tofile(st)?;
    let op = st.check_option(2, None, &["no", "full", "line"])?;
    let sz = st.opt_integer(3, LUAL_BUFFERSIZE as i64)?;
    set_errno(0);
    let mode = [libc::_IONBF, libc::_IOFBF, libc::_IOLBF][op];
    let ok = f.setvbuf(mode, sz as usize);
    file_result(st, ok, None)
}

fn aux_flush(st: &mut State, f: CFile) -> LuaResult<usize> {
    set_errno(0);
    let ok = f.flush();
    file_result(st, ok, None)
}

fn f_flush(st: &mut State) -> LuaResult<usize> {
    let f = tofile(st)?;
    aux_flush(st, f)
}

fn io_flush(st: &mut State) -> LuaResult<usize> {
    let (_, f) = getiofile(st, IO_OUTPUT)?;
    aux_flush(st, f)
}

// ---- library setup ----------------------------------------------------------------------------

const IOLIB: &[(&str, Option<NativeFn>)] = &[
    ("close", Some(io_close)),
    ("flush", Some(io_flush)),
    ("input", Some(io_input)),
    ("lines", Some(io_lines)),
    ("open", Some(io_open)),
    ("output", Some(io_output)),
    ("popen", Some(io_popen)),
    ("read", Some(io_read)),
    ("tmpfile", Some(io_tmpfile)),
    ("type", Some(io_type)),
    ("write", Some(io_write)),
];

const METH: &[(&str, Option<NativeFn>)] = &[
    ("read", Some(f_read)),
    ("write", Some(f_write)),
    ("lines", Some(f_lines)),
    ("flush", Some(f_flush)),
    ("seek", Some(f_seek)),
    ("close", Some(f_close)),
    ("setvbuf", Some(f_setvbuf)),
];

const METAMETH: &[(&str, Option<NativeFn>)] = &[
    ("__index", None),
    ("__gc", Some(f_gc)),
    ("__close", Some(f_gc)),
    ("__tostring", Some(f_tostring)),
];

fn create_meta(st: &mut State) -> LuaResult<()> {
    let mt = st.new_table_sized(0, 5)?;
    st.push(Value::table(mt));
    let name = st.new_str_value(LUA_FILEHANDLE)?;
    st.push(name);
    st.raw_set_str(mt, b"__name", name)?;
    st.registry_set(LUA_FILEHANDLE, Value::table(mt))?;
    st.set_funcs(mt, METAMETH)?;
    let meth = st.new_table_sized(0, METH.len())?;
    st.push(Value::table(meth));
    st.set_funcs(meth, METH)?;
    st.raw_set_str(mt, b"__index", Value::table(meth))?;
    st.pop();
    st.pop();
    st.pop();
    Ok(())
}

fn create_std_file(
    st: &mut State,
    lib: TableRef,
    f: CFile,
    key: Option<&[u8]>,
    fname: &str,
) -> LuaResult<()> {
    let u = new_prefile(st)?;
    write_stream(
        u,
        LStream {
            f: f.0,
            closef: Some(io_noclose),
        },
    );
    if let Some(k) = key {
        st.registry_set(k, Value::userdata(u))?;
    }
    st.raw_set_str(lib, fname.as_bytes(), Value::userdata(u))?;
    st.pop();
    Ok(())
}

/// `luaopen_io`.
pub fn open(st: &mut State) -> LuaResult<usize> {
    let lib = st.new_table_sized(0, IOLIB.len())?;
    st.push(Value::table(lib));
    st.set_funcs(lib, IOLIB)?;
    create_meta(st)?;
    create_std_file(st, lib, io_ffi::std_in(), Some(IO_INPUT), "stdin")?;
    create_std_file(st, lib, io_ffi::std_out(), Some(IO_OUTPUT), "stdout")?;
    create_std_file(st, lib, io_ffi::std_err(), None, "stderr")?;
    Ok(1)
}
