//! `package` library and `require` (port of `loadlib.c`). C libraries are loaded with `dlopen`
//! (see [`crate::dl_ffi`]); loaded handles live in the registry table `_CLIBS`, whose `__gc`
//! unloads them.
#![forbid(unsafe_code)]

use rn_core::auxlib::{LUA_LOADED_TABLE, LUA_PRELOAD_TABLE};
use rn_core::state::{NativeFn, State};
use rn_core::status::{LuaResult, Status};
use rn_core::value::Value;

const CLIBS: &[u8] = b"_CLIBS";
const LUA_PATH_SEP: u8 = b';';
const LUA_PATH_MARK: u8 = b'?';
const LUA_DIRSEP: &[u8] = b"/";
const LUA_IGMARK: u8 = b'-';
const LIB_FAIL: &str = "open";

#[cfg(not(rustynail_lua54))]
const LUA_VERSUFFIX: &str = "_5_5";
#[cfg(rustynail_lua54)]
const LUA_VERSUFFIX: &str = "_5_4";

#[cfg(not(rustynail_lua54))]
const LUA_PATH_DEFAULT: &[u8] = b"/usr/local/share/lua/5.5/?.lua;/usr/local/share/lua/5.5/?/init.lua;/usr/local/lib/lua/5.5/?.lua;/usr/local/lib/lua/5.5/?/init.lua;./?.lua;./?/init.lua";
#[cfg(not(rustynail_lua54))]
const LUA_CPATH_DEFAULT: &[u8] =
    b"/usr/local/lib/lua/5.5/?.so;/usr/local/lib/lua/5.5/loadall.so;./?.so";
#[cfg(rustynail_lua54)]
const LUA_PATH_DEFAULT: &[u8] = b"/usr/local/share/lua/5.4/?.lua;/usr/local/share/lua/5.4/?/init.lua;/usr/local/lib/lua/5.4/?.lua;/usr/local/lib/lua/5.4/?/init.lua;./?.lua;./?/init.lua";
#[cfg(rustynail_lua54)]
const LUA_CPATH_DEFAULT: &[u8] =
    b"/usr/local/lib/lua/5.4/?.so;/usr/local/lib/lua/5.4/loadall.so;./?.so";

/// First upvalue of the running native closure (the `package` table).
fn upvalue0(st: &State) -> Value {
    let f = st.get(st.ci().r().func);
    f.as_cclosure().map_or(Value::NIL, |c| c.upvalue(0))
}

/// Create a native closure with upvalues and push it.
fn push_closure(st: &mut State, f: NativeFn, ups: &[Value]) -> LuaResult<Value> {
    let cl = st.try_alloc(|h| h.new_cclosure(f, ups.len()))?;
    let v = Value::cclosure(cl);
    st.push(v);
    for (i, u) in ups.iter().enumerate() {
        cl.set_upvalue(st.heap(), i, *u);
    }
    st.check_gc();
    Ok(v)
}

/// `luaL_gsub` on byte strings (literal pattern).
fn gsub(s: &[u8], pat: &[u8], rep: &[u8]) -> Vec<u8> {
    if pat.is_empty() {
        return s.to_vec();
    }
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with(pat) {
            out.extend_from_slice(rep);
            i += pat.len();
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

fn readable(filename: &[u8]) -> bool {
    std::fs::File::open(rn_core::auxlib::bytes_to_path(filename)).is_ok()
}

/// `searchpath`: the first readable file, or the "no file ..." message.
fn searchpath(name: &[u8], path: &[u8], sep: &[u8], dirsep: &[u8]) -> Result<Vec<u8>, Vec<u8>> {
    let name: Vec<u8> = if !sep.is_empty() && name.contains(&sep[0]) {
        gsub(name, sep, dirsep)
    } else {
        name.to_vec()
    };
    let pathname = gsub(path, &[LUA_PATH_MARK], &name);
    if !pathname.is_empty() {
        for filename in pathname.split(|&c| c == LUA_PATH_SEP) {
            if readable(filename) {
                return Ok(filename.to_vec());
            }
        }
    }
    let mut msg = b"no file '".to_vec();
    msg.extend_from_slice(&gsub(&pathname, &[LUA_PATH_SEP], b"'\n\tno file '"));
    msg.push(b'\'');
    Err(msg)
}

fn ll_searchpath(st: &mut State) -> LuaResult<usize> {
    // the reference evaluates the C call arguments right to left
    let dirsep: &[u8] = match st.opt_str(4)? {
        Some(s) => s.get().as_bytes(),
        None => LUA_DIRSEP,
    };
    let sep: &[u8] = match st.opt_str(3)? {
        Some(s) => s.get().as_bytes(),
        None => b".",
    };
    let path = st.check_str(2)?.get().as_bytes();
    let name = st.check_str(1)?.get().as_bytes();
    match searchpath(name, path, sep, dirsep) {
        Ok(f) => {
            st.push_bytes(&f)?;
            Ok(1)
        }
        Err(m) => {
            st.push(Value::NIL);
            st.push_bytes(&m)?;
            Ok(2)
        }
    }
}

/// `findfile`: `Ok(Some(name))` or `Err(message)`; raises if the path is not a string.
fn findfile(
    st: &mut State,
    name: &[u8],
    pname: &str,
    dirsep: &[u8],
) -> LuaResult<Result<Vec<u8>, Vec<u8>>> {
    let pkg = upvalue0(st);
    let v = st.index_str(pkg, pname.as_bytes())?;
    let path: Vec<u8> = if let Some(s) = v.as_string() {
        s.as_bytes().to_vec()
    } else if let Some(n) = v.as_number() {
        rn_core::number::tostring(n).as_bytes().to_vec()
    } else {
        return Err(st.l_error(format!("'package.{pname}' must be a string").as_bytes()));
    };
    Ok(searchpath(name, &path, b".", dirsep))
}

/// `checkload`: on success pushes the file name and returns 2 (loader below it).
fn checkload(st: &mut State, ok: bool, filename: &[u8]) -> LuaResult<usize> {
    if ok {
        st.push_bytes(filename)?;
        return Ok(2);
    }
    let name = st.arg(0);
    let name = name
        .as_string()
        .map(|s| s.as_bytes().to_vec())
        .unwrap_or_default();
    let e = st.get(st.top() - 1);
    let e = e
        .as_string()
        .map(|s| s.as_bytes().to_vec())
        .unwrap_or_default();
    let mut m = b"error loading module '".to_vec();
    m.extend_from_slice(&name);
    m.extend_from_slice(b"' from file '");
    m.extend_from_slice(filename);
    m.extend_from_slice(b"':\n\t");
    m.extend_from_slice(&e);
    Err(st.l_error(&m))
}

fn searcher_lua(st: &mut State) -> LuaResult<usize> {
    let name = st.check_str(1)?.get().as_bytes();
    let filename = match findfile(st, name, "path", LUA_DIRSEP)? {
        Ok(f) => f,
        Err(m) => {
            st.push_bytes(&m)?;
            return Ok(1);
        }
    };
    let status = st.load_filex(Some(&filename), Some(b"bt"));
    checkload(st, status == Status::Ok, &filename)
}

const ERRLIB: i32 = 1;
const ERRFUNC: i32 = 2;

/// `lookforfunc`: push the C function `sym` of library `path` (loading the library if needed).
/// `sym == "*"` only loads the library (globally) and pushes `true`. Returns 0, `ERRLIB` or
/// `ERRFUNC` (message pushed on error).
fn lookforfunc(st: &mut State, path: &[u8], sym: &[u8]) -> LuaResult<i32> {
    let clibs = st
        .registry_get(CLIBS)?
        .as_table()
        .expect("_CLIBS is a table");
    let mut lib = st
        .raw_get_str(clibs, path)?
        .as_light_userdata()
        .map(crate::dl_ffi::DlHandle);
    if lib.is_none() {
        // must load the library
        match crate::dl_ffi::open(path, sym.first() == Some(&b'*')) {
            Err(msg) => {
                st.push_bytes(&msg)?;
                return Ok(ERRLIB);
            }
            Ok(h) => {
                let v = Value::light_userdata(h.0);
                st.raw_set_str(clibs, path, v)?;
                let n = clibs.len(st.heap()) as i64;
                st.raw_set_int(clibs, n + 1, &v)?;
                lib = Some(h);
            }
        }
    }
    if sym.first() == Some(&b'*') {
        st.push(Value::TRUE); // loading only the library
        return Ok(0);
    }
    match crate::dl_ffi::sym(lib.expect("handle"), sym) {
        Err(msg) => {
            st.push_bytes(&msg)?;
            Ok(ERRFUNC)
        }
        Ok(f) => {
            st.push(Value::light_cfunction(f));
            Ok(0)
        }
    }
}

/// `gctm` of the `_CLIBS` table: unload every library, in reverse order.
fn clibs_gc(st: &mut State) -> LuaResult<usize> {
    let t = st.check_table(1)?;
    let mut n = t.len(st.heap()) as i64;
    while n >= 1 {
        if let Some(h) = t.get_int(n).as_light_userdata() {
            crate::dl_ffi::close(crate::dl_ffi::DlHandle(h));
        }
        n -= 1;
    }
    Ok(0)
}

/// `loadfunc`: returns 0 (function pushed) or an error code (message pushed).
fn loadfunc(st: &mut State, filename: &[u8], modname: &[u8]) -> LuaResult<i32> {
    let modname = gsub(modname, b".", b"_");
    let mut modname = modname.as_slice();
    if let Some(mark) = modname.iter().position(|&c| c == LUA_IGMARK) {
        let mut of = b"luaopen_".to_vec();
        of.extend_from_slice(&modname[..mark]);
        let stat = lookforfunc(st, filename, &of)?;
        if stat != ERRFUNC {
            return Ok(stat);
        }
        modname = &modname[mark + 1..];
    }
    let mut of = b"luaopen_".to_vec();
    of.extend_from_slice(modname);
    lookforfunc(st, filename, &of)
}

fn searcher_c(st: &mut State) -> LuaResult<usize> {
    let name = st.check_str(1)?.get().as_bytes();
    let filename = match findfile(st, name, "cpath", LUA_DIRSEP)? {
        Ok(f) => f,
        Err(m) => {
            st.push_bytes(&m)?;
            return Ok(1);
        }
    };
    let stat = loadfunc(st, &filename, name)?;
    checkload(st, stat == 0, &filename)
}

fn searcher_croot(st: &mut State) -> LuaResult<usize> {
    let name = st.check_str(1)?.get().as_bytes();
    let Some(p) = name.iter().position(|&c| c == b'.') else {
        return Ok(0);
    };
    let root = &name[..p];
    let filename = match findfile(st, root, "cpath", LUA_DIRSEP)? {
        Ok(f) => f,
        Err(m) => {
            st.push_bytes(&m)?;
            return Ok(1);
        }
    };
    let stat = loadfunc(st, &filename, name)?;
    if stat != 0 {
        if stat != ERRFUNC {
            return checkload(st, false, &filename);
        }
        let mut m = b"no module '".to_vec();
        m.extend_from_slice(name);
        m.extend_from_slice(b"' in file '");
        m.extend_from_slice(&filename);
        m.push(b'\'');
        st.push_bytes(&m)?;
        return Ok(1);
    }
    st.push_bytes(&filename)?;
    Ok(2)
}

fn searcher_preload(st: &mut State) -> LuaResult<usize> {
    let name = st.check_str(1)?.get().as_bytes();
    let pre = st.registry_get(LUA_PRELOAD_TABLE)?;
    st.push(pre);
    let v = st.index_str(pre, name)?;
    if v.is_nil() {
        let mut m = b"no field package.preload['".to_vec();
        m.extend_from_slice(name);
        m.extend_from_slice(b"']");
        st.push_bytes(&m)?;
        Ok(1)
    } else {
        st.push(v);
        st.push_str(":preload:")?;
        Ok(2)
    }
}

/// `findloader`: leaves `loader, loader-data` on the stack.
fn findloader(st: &mut State, name: &[u8]) -> LuaResult<()> {
    let pkg = upvalue0(st);
    let searchers = st.index_str(pkg, b"searchers")?;
    let Some(searchers) = searchers.as_table() else {
        return Err(st.l_error(b"'package.searchers' must be a table"));
    };
    st.push(Value::table(searchers));
    let namev = st.arg(0);
    #[cfg(not(rustynail_lua54))]
    let mut msg: Vec<u8> = b"\n\t".to_vec();
    #[cfg(rustynail_lua54)]
    let mut msg: Vec<u8> = Vec::new();
    let mut i = 1i64;
    loop {
        let s = searchers.get_int(i);
        #[cfg(rustynail_lua54)]
        msg.extend_from_slice(b"\n\t");
        if s.is_nil() {
            msg.truncate(msg.len().saturating_sub(2));
            let mut m = b"module '".to_vec();
            m.extend_from_slice(name);
            m.extend_from_slice(b"' not found:");
            m.extend_from_slice(&msg);
            return Err(st.l_error(&m));
        }
        st.call_value(s, &[namev], 2)?;
        let top = st.top();
        let r1 = st.get(top - 2);
        if r1.is_function() {
            // drop the searchers table from under the results
            let r2 = st.get(top - 1);
            st.set(top - 3, r1);
            st.set(top - 2, r2);
            st.set_top(top - 1);
            return Ok(());
        } else if let Some(sb) = r1.as_string() {
            msg.extend_from_slice(sb.as_bytes());
            #[cfg(not(rustynail_lua54))]
            msg.extend_from_slice(b"\n\t");
        } else if let Some(n) = r1.as_number() {
            msg.extend_from_slice(rn_core::number::tostring(n).as_bytes());
            #[cfg(not(rustynail_lua54))]
            msg.extend_from_slice(b"\n\t");
        } else {
            // no error message: remove prefix (5.4)
            #[cfg(rustynail_lua54)]
            msg.truncate(msg.len() - 2);
        }
        st.set_top(top - 2);
        i += 1;
    }
}

fn ll_require(st: &mut State) -> LuaResult<usize> {
    let name = st.check_str(1)?.get().as_bytes();
    let base = st.ci().r().func + 1;
    st.set_top(base + 1);
    let loaded = st.registry_get(LUA_LOADED_TABLE)?;
    st.push(loaded);
    let v = st.index_str(loaded, name)?;
    if v.is_truthy() {
        st.push(v);
        return Ok(1);
    }
    findloader(st, name)?;
    // stack: name, loaded, loader, data
    let top = st.top();
    let loader = st.get(top - 2);
    let data = st.get(top - 1);
    let namev = st.arg(0);
    st.call_value(loader, &[namev, data], 1)?;
    let r = st.pop();
    if !r.is_nil() {
        st.push(r);
        st.set_index_str(loaded, name, r)?;
        st.pop();
    }
    let mut m = st.index_str(loaded, name)?;
    if m.is_nil() {
        m = Value::TRUE;
        st.set_index_str(loaded, name, m)?;
    }
    st.push(m);
    st.push(data);
    Ok(2)
}

fn ll_loadlib(st: &mut State) -> LuaResult<usize> {
    let path = st.check_str(1)?.get().as_bytes();
    let init = st.check_str(2)?.get().as_bytes();
    let stat = lookforfunc(st, path, init)?;
    if stat == 0 {
        return Ok(1);
    }
    // error message is on top: fail, message, where
    let msg = st.pop();
    st.push(Value::NIL);
    st.push(msg);
    st.push_str(if stat == ERRLIB { LIB_FAIL } else { "init" })?;
    Ok(3)
}

fn setpath(
    st: &mut State,
    pkg: rn_core::table::TableRef,
    fieldname: &str,
    envname: &str,
    dft: &[u8],
) -> LuaResult<()> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let nver = format!("{envname}{LUA_VERSUFFIX}");
    let get = |n: &str| std::env::var_os(std::ffi::OsStr::from_bytes(n.as_bytes()));
    let path = get(&nver).or_else(|| get(envname)).map(|p| p.into_vec());
    let noenv = st.registry_get(b"LUA_NOENV")?.is_truthy();
    let result: Vec<u8> = match path {
        Some(path) if !noenv => match path.windows(2).position(|w| w == b";;") {
            None => path,
            Some(i) => {
                let mut b = Vec::new();
                if i > 0 {
                    b.extend_from_slice(&path[..i]);
                    b.push(LUA_PATH_SEP);
                }
                b.extend_from_slice(dft);
                if i + 2 < path.len() {
                    b.push(LUA_PATH_SEP);
                    b.extend_from_slice(&path[i + 2..]);
                }
                b
            }
        },
        _ => dft.to_vec(),
    };
    let s = st.push_bytes(&result)?;
    st.raw_set_str(pkg, fieldname.as_bytes(), s)?;
    st.pop();
    Ok(())
}

const PK_FUNCS: &[(&str, Option<NativeFn>)] = &[
    ("loadlib", Some(ll_loadlib)),
    ("searchpath", Some(ll_searchpath)),
    ("preload", None),
    ("cpath", None),
    ("path", None),
    ("searchers", None),
    ("loaded", None),
];

/// `luaopen_package`.
pub fn open(st: &mut State) -> LuaResult<usize> {
    let reg = st.registry();
    let (clibs, existed) = st.get_sub_table(reg, CLIBS)?;
    if !existed {
        // the table that owns the loaded libraries closes them when collected
        let mt = st.new_table_sized(0, 1)?;
        st.push(Value::table(mt));
        st.raw_set_str(mt, b"__gc", Value::native(clibs_gc))?;
        st.set_metatable(&Value::table(clibs), Some(mt));
        st.pop();
    }
    let pkg = st.new_table_sized(0, PK_FUNCS.len())?;
    let pkgv = Value::table(pkg);
    st.push(pkgv);
    st.set_funcs(pkg, PK_FUNCS)?;
    // searchers
    let searchers = st.new_table_sized(4, 0)?;
    st.push(Value::table(searchers));
    let fns: [NativeFn; 4] = [searcher_preload, searcher_lua, searcher_c, searcher_croot];
    for (i, f) in fns.iter().enumerate() {
        let c = push_closure(st, *f, &[pkgv])?;
        st.raw_set_int(searchers, i as i64 + 1, &c)?;
        st.pop();
    }
    st.raw_set_str(pkg, b"searchers", Value::table(searchers))?;
    st.pop();
    setpath(st, pkg, "path", "LUA_PATH", LUA_PATH_DEFAULT)?;
    setpath(st, pkg, "cpath", "LUA_CPATH", LUA_CPATH_DEFAULT)?;
    let cfg = st.push_bytes(b"/\n;\n?\n!\n-\n")?;
    st.raw_set_str(pkg, b"config", cfg)?;
    st.pop();
    let (loaded, _) = st.get_sub_table(reg, LUA_LOADED_TABLE)?;
    st.raw_set_str(pkg, b"loaded", Value::table(loaded))?;
    let (pre, _) = st.get_sub_table(reg, LUA_PRELOAD_TABLE)?;
    st.raw_set_str(pkg, b"preload", Value::table(pre))?;
    // require, with `package` as upvalue, in the globals table
    let req = push_closure(st, ll_require, &[pkgv])?;
    let g = st.globals();
    st.raw_set_str(g, b"require", req)?;
    st.pop();
    Ok(1)
}
