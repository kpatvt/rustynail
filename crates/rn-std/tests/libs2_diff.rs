//! io / os / package library cases run through our interpreter and the reference binary; the
//! (string) results must be identical. `@D@` in a snippet is replaced by a private temp directory.
use std::process::{Command, Stdio};

fn ref_bin() -> Option<std::path::PathBuf> {
    #[cfg(not(rustynail_lua54))]
    let p = "ref/55/build/lua";
    #[cfg(rustynail_lua54)]
    let p = "ref/54/build/lua";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let b = root.join(p);
    b.exists().then_some(b)
}

fn run_ours(src: &str) -> String {
    let mut st = rn_std::new_state_with_libs().expect("state");
    let status = st.load_chunk(src.as_bytes(), b"=(command line)", None);
    if status != rn_core::status::Status::Ok {
        let m = st.get(st.top() - 1);
        return format!(
            "LOADERR:{}",
            String::from_utf8_lossy(m.as_string().unwrap().as_bytes())
        );
    }
    let f = st.top() - 1;
    let s = st.pcall(f, 1, 0);
    let v = st.get(st.top() - 1);
    let text = v
        .as_string()
        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
        .unwrap_or_else(|| format!("{v:?}"));
    if s == rn_core::status::Status::Ok {
        text
    } else {
        format!("ERR:{text}")
    }
}

fn run_ref(src: &str) -> Option<String> {
    let b = ref_bin()?;
    let wrapped = format!(
        "local ok, r = pcall(function() {src}\n end) if ok then io.write(tostring(r)) else io.write('ERR:', tostring(r)) end"
    );
    let out = Command::new(b)
        .arg("-e")
        .arg(wrapped)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Hide addresses (`0x55...`).
fn norm(s: String) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'0'
            && b.get(i + 1) == Some(&b'x')
            && b.get(i + 2).is_some_and(|c| c.is_ascii_hexdigit())
        {
            i += 2;
            while i < b.len() && b[i].is_ascii_hexdigit() {
                i += 1;
            }
            out.push_str("PTR");
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

fn tmpdir(tag: &str) -> String {
    // Tests run on parallel threads of one process: make every dir unique.
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("rn_libs2_{}_{}_{}", std::process::id(), seq, tag));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().into_owned()
}

/// Compare all cases; returns the list of mismatches.
fn compare(cases: &[&str]) -> Vec<String> {
    let mut bad = Vec::new();
    for (i, c) in cases.iter().enumerate() {
        let d_ours = tmpdir(&format!("o{i}"));
        let d_ref = tmpdir(&format!("r{i}"));
        let ours = norm(run_ours(&c.replace("@D@", &d_ours)).replace(&d_ours, "D"));
        if ours.contains("interpreter is not implemented yet") {
            continue; // 5.4 executor not available yet
        }
        if let Some(r) = run_ref(&c.replace("@D@", &d_ref)) {
            let r = norm(r.replace(&d_ref, "D"));
            if r != ours {
                bad.push(format!("MISMATCH for {c}\n  ours: {ours:?}\n  ref:  {r:?}"));
            }
        }
        let _ = std::fs::remove_dir_all(&d_ours);
        let _ = std::fs::remove_dir_all(&d_ref);
    }
    bad
}

const IO_CASES: &[&str] = &[
    // write / read back
    "local f = assert(io.open('@D@/a.txt','w')) f:write('hello\\n', 12, ' ', 1.5, '\\nworld') f:close() \
     local g = io.open('@D@/a.txt') local a,b,c = g:read('l','n','a') g:close() return tostring(a)..'|'..tostring(b)..'|'..tostring(c)",
    "local f = io.open('@D@/a.txt','w') f:write('l1\\nl2\\n\\nl4') f:close() local t={} for l in io.lines('@D@/a.txt') do t[#t+1]=l end return #t..':'..table.concat(t,',')",
    "local f = io.open('@D@/a.txt','w') f:write('l1\\nl2\\n') f:close() local t={} for l in io.lines('@D@/a.txt','L') do t[#t+1]=l end return table.concat(t,'#')",
    "local f = io.open('@D@/a.txt','w') f:write('0x1F 12e2 -7 .5 abc') f:close() f=io.open('@D@/a.txt') return table.concat({tostring(f:read('n')),tostring(f:read('n')),tostring(f:read('n')),tostring(f:read('n')),tostring(f:read('n'))},',')",
    "local f = io.open('@D@/a.txt','w') f:write('abcdefghij') f:close() f=io.open('@D@/a.txt') return f:read(3)..'|'..f:read(0)..'|'..f:read(100)..'|'..tostring(f:read(1))..'|'..tostring(f:read(0))",
    "local f = io.open('@D@/a.txt','w') f:write('abcdefghij') f:close() f=io.open('@D@/a.txt') local a=f:seek('set',2) local b=f:read(2) local c=f:seek('cur') local d=f:seek('end') local e=f:seek('cur',-3) return table.concat({a,b,c,d,e,f:read('a')},',')",
    "local f = io.open('@D@/nonexistent/x.txt') return tostring(f)",
    "local a,b,c = io.open('@D@/nonexistent/x.txt') return tostring(a)..'|'..b..'|'..c",
    "local f = io.open('@D@/a.txt','w') f:close() return tostring(pcall(f.write, f, 'x'))..select(2,pcall(f.write,f,'x'))",
    "local f = io.open('@D@/a.txt','w') return io.type(f)..tostring(io.type(1))..tostring(f)..'|'..(function() f:close() return io.type(f)..tostring(f) end)()",
    "return select(2, pcall(io.open, 'x', 'rw'))",
    "return select(2, pcall(io.open, 'x', 'r+b'))..tostring(io.open('@D@/no','r+b'))",
    "return select(2, pcall(io.open))",
    "return select(2, pcall(io.type))",
    "local f = io.open('@D@/a.txt','w') return select(2, pcall(f.read, f, 'x'))",
    "local f = io.open('@D@/a.txt','w') f:close() return select(2, pcall(f.read, f))",
    "return select(2, pcall(io.lines, '@D@/zzz'))",
    "local f = io.open('@D@/a.txt','w') f:write('a\\nb\\n') f:close() local it = io.lines('@D@/a.txt') local x = it() local y = it() local z = it() return x..y..tostring(z)..select(2, pcall(it))",
    "local f = io.open('@D@/a.txt','w') f:write('a\\nb\\n') f:close() local n=0 for l in io.lines('@D@/a.txt') do n=n+1 end return n",
    "local f = io.open('@D@/a.txt','w') f:write('x y z') f:close() local t={} for a,b in io.lines('@D@/a.txt', 1, 1) do t[#t+1]=a..b end return table.concat(t,',')",
    "return select(2, pcall(io.lines, '@D@/a.txt', table.unpack({}, 1, 300)))",
    "local f = io.open('@D@/a.txt','w') local a = f:setvbuf('no') local b = f:setvbuf('full', 100) local c = f:setvbuf('line') return tostring(a)..tostring(b)..tostring(c)..select(2,pcall(f.setvbuf,f,'bad'))",
    "local f = io.open('@D@/a.txt','w') return select(2, pcall(f.seek, f, 'bad'))..select(2, pcall(f.seek, f, 'set', 1.5))",
    "local f = io.open('@D@/a.txt','w') return tostring(f:flush()==f)..tostring(io.flush())..tostring(f:write('a')==f)",
    "local f = io.tmpfile() f:write('abc') f:seek('set') return f:read('a')",
    "io.output('@D@/o.txt') io.write('via default') io.close() io.output(io.stdout) local f = io.open('@D@/o.txt') local s = f:read('a') f:close() return s",
    "local f = io.open('@D@/i.txt','w') f:write('line1\\nline2') f:close() io.input('@D@/i.txt') local a = io.read() local b = io.read('L') io.close(io.input()) return a..'|'..tostring(b)..select(2, pcall(io.read))",
    "return io.stdout:close()..'' ",
    "local a,b = io.stdout:close() return tostring(a)..'|'..tostring(b)",
    "return tostring(io.stdin)=='file ('..tostring(io.stdin):match('0x%x+')..')' and 'ok'",
    "local f = io.popen('echo hi', 'r') local s = f:read('a') local a,b,c = f:close() return s..tostring(a)..tostring(b)..tostring(c)",
    "local f = io.popen('exit 3', 'r') local a,b,c = f:close() return tostring(a)..tostring(b)..tostring(c)",
    "return select(2, pcall(io.popen, 'ls', 'rw'))",
    "local f = io.open('@D@/a.txt','w') f:write(string.rep('x', 5000), '\\n', 'tail') f:close() local l = io.open('@D@/a.txt'):read('l') return #l",
    "local f = io.open('@D@/a.txt','w') f:write('1234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890') f:close() return tostring(io.open('@D@/a.txt'):read('n'))",
    "local f = io.open('@D@/a.txt','w') f:write('  -  12') f:close() return tostring(io.open('@D@/a.txt'):read('n'))",
    "local f = io.open('@D@/a.txt','w') f:write('0x') f:close() return tostring(io.open('@D@/a.txt'):read('n'))",
    "local f = io.open('@D@/a.txt','w') f:write('1e+') f:close() return tostring(io.open('@D@/a.txt'):read('n'))",
    "local f = io.open('@D@/a.txt','w') f:write('abc') f:close() local g = io.open('@D@/a.txt') return tostring(g:read('*a'))..tostring(g:read('*a'))..tostring(g:read('*l'))..tostring(g:read(0))",
    "do local f <close> = io.open('@D@/a.txt','w') f:write('x') end return io.open('@D@/a.txt'):read('a')",
    "local f = io.open('@D@/a.txt','w') return select(2, pcall(io.write, f))",
    "local f = io.open('@D@/a.txt','w') return select(2, pcall(f.write, f, {}))",
    "local f = io.open('@D@/a.txt','w') return select(2, pcall(f.write, f, nil))",
    "return select(2, pcall(io.close, 12))",
    "return select(2, pcall(io.output, {}))",
    "local f = io.open('@D@/a.txt','w') f:write('a') f:close() return select(2, pcall(io.lines))..''",
    "local t = {} for k in pairs(io) do t[#t+1]=k end table.sort(t) return table.concat(t,',')",
    "local t = {} for k in pairs(getmetatable(io.stdout)) do t[#t+1]=k end table.sort(t) return table.concat(t,',')",
    "local t = {} for k in pairs(getmetatable(io.stdout).__index) do t[#t+1]=k end table.sort(t) return table.concat(t,',')",
    "return getmetatable(io.stdout).__name",
];

const IO_CASES2: &[&str] = &[
    "local f = io.open('/dev/full','w') f:setvbuf('no') local a,b,c = f:write('x') return tostring(a)..'|'..tostring(b)..'|'..tostring(c)",
    "local t = {} for i = 1, 300 do local f = io.open('@D@/g'..i, 'w') f:write(i) t[i] = f end t = nil collectgarbage() collectgarbage() local n = 0 for i = 1, 300 do local f = io.open('@D@/g'..i) n = n + tonumber(f:read('a')) f:close() end return n",
    "local f = io.open('@D@/a.txt','w') f:write('1\\n2\\n3\\n') f:close() local s = 0 for a in io.lines('@D@/a.txt', 'n') do s = s + a end return s",
    "local f = io.open('@D@/a.txt','w') f:write('1 2 3') f:close() local g = io.open('@D@/a.txt') local a,b,c,d = g:read('n','n','n','n') return tostring(a)..tostring(b)..tostring(c)..tostring(d)",
    "local f = io.open('@D@/a.txt','w') f:write('abc\\ndef') f:close() local g = io.open('@D@/a.txt') local t = {} for l in g:lines() do t[#t+1] = l end return table.concat(t, ',')..tostring(io.type(g))",
    "local f = io.open('@D@/a.txt','w') f:write('abc') f:close() local g = io.open('@D@/a.txt') local it = g:lines() g:close() return select(2, pcall(it))",
    "return select(2, pcall(io.read, '*x'))",
    "io.input('/dev/null') return tostring(io.read(0))",
    "local f <close> = io.open('@D@/a.txt','w') return select(2, pcall(f.seek, f, 'set', 2^70))",
    "local f = io.open('@D@/a.txt','w') f:write('0x1p4 0x.8 1e2 5.') f:close() local g = io.open('@D@/a.txt') return table.concat({tostring(g:read('n')),tostring(g:read('n')),tostring(g:read('n')),tostring(g:read('n'))}, ',')",
    "local f = io.open('@D@/a.txt','w') f:write('a') f:close() local g = io.open('@D@/a.txt') return select(2, pcall(g.read, g, 1.5))",
    "local f = io.open('@D@/a.txt','w') f:close() return tostring(io.open('@D@/a.txt'):read('l'))..tostring(io.open('@D@/a.txt'):read('a'))..tostring(io.open('@D@/a.txt'):read(5))",
];

const OS_CASES: &[&str] = &[
    "return os.date('!%Y-%m-%d %H:%M:%S', 86400*365)",
    "return os.date('!%c', 0)",
    "local t = os.date('!*t', 3600) return t.year..'-'..t.month..'-'..t.day..' '..t.hour..' '..t.wday..' '..t.yday..' '..tostring(t.isdst)",
    "return os.time{year=2020, month=1, day=1, hour=0}",
    "return os.time{year=2020, month=14, day=40}",
    "local t = {year=2020, month=14, day=40, hour=12} os.time(t) return t.year..'/'..t.month..'/'..t.day",
    "return select(2, pcall(os.time, {year=2020}))",
    "return select(2, pcall(os.time, {year=2020, month='x', day=1}))",
    "return select(2, pcall(os.time, {year=1e100, month=1, day=1}))",
    "return select(2, pcall(os.time, {year=2^40, month=1, day=1}))",
    "return select(2, pcall(os.date, '%Q'))",
    "return select(2, pcall(os.date, '%'))",
    "return select(2, pcall(os.time, 1))",
    "return os.difftime(10, 4)",
    "return select(2, pcall(os.difftime))",
    "return tostring(os.getenv('HOME') ~= nil)..tostring(os.getenv('NOPE_NOT_SET_XYZ'))",
    "return type(os.clock())",
    "return type(os.tmpname())",
    "local n = os.tmpname() local a = os.remove(n) return tostring(a)",
    "local a,b,c = os.remove('@D@/nonexist') return tostring(a)..'|'..b..'|'..c",
    "local a,b,c = os.rename('@D@/nonexist', '@D@/x') return tostring(a)..'|'..b..'|'..c",
    "local f = io.open('@D@/a','w') f:close() local a = os.rename('@D@/a','@D@/b') return tostring(a)..tostring(io.open('@D@/b')~=nil)..tostring(os.remove('@D@/b'))",
    "return tostring(os.execute())",
    "local a,b,c = os.execute('exit 2') return tostring(a)..tostring(b)..tostring(c)",
    "local a,b,c = os.execute('true') return tostring(a)..tostring(b)..tostring(c)",
    "local a,b,c = os.execute('kill -9 $$') return tostring(a)..tostring(b)..tostring(c)",
    "return os.setlocale()..'|'..tostring(os.setlocale('C'))..'|'..tostring(os.setlocale('xx_YY'))..'|'..tostring(os.setlocale(nil, 'numeric'))",
    "return select(2, pcall(os.setlocale, 'C', 'bad'))",
    "local t = {} for k in pairs(os) do t[#t+1]=k end table.sort(t) return table.concat(t,',')",
    "return select(2, pcall(os.getenv))",
    "return select(2, pcall(os.exit, {}))..''",
];

const PKG_CASES: &[&str] = &[
    "package.path = '@D@/?.lua' local f = io.open('@D@/mymod.lua','w') f:write('local M = {} M.x = ... return M') f:close() local m, where = require('mymod') return m.x..'|'..where..'|'..tostring(package.loaded.mymod == m)",
    "package.path = '@D@/?.lua' local f = io.open('@D@/m2.lua','w') f:write('x = 1') f:close() local m, w = require('m2') return tostring(m)..'|'..w",
    "package.path = '@D@/?.lua;@D@/?/init.lua' os.execute('mkdir -p @D@/sub') local f = io.open('@D@/sub/init.lua','w') f:write('return {ok=true}') f:close() return tostring(require('sub').ok)",
    "package.path = '@D@/?.lua' os.execute('mkdir -p @D@/a') local f = io.open('@D@/a/b.lua','w') f:write('return 42') f:close() return require('a.b')",
    "package.path = '@D@/?.lua' package.cpath = '@D@/?.so' return select(2, pcall(require, 'nomod'))",
    "package.path = '' package.cpath = '' return select(2, pcall(require, 'nomod.sub'))",
    "package.preload.foo = function(...) return {n=select('#',...), a=..., b=select(2,...)} end local m, w = require('foo') return m.n..tostring(m.a)..tostring(m.b)..tostring(w)",
    "package.path = '@D@/?.lua' local f = io.open('@D@/bad.lua','w') f:write('x = = 1') f:close() return select(2, pcall(require, 'bad'))",
    "package.path = '@D@/?.lua' local f = io.open('@D@/err.lua','w') f:write('error(\"boom\")') f:close() return select(2, pcall(require, 'err'))",
    "package.path = 12 return select(2, pcall(require, 'zzz'))",
    "package.path = {} return select(2, pcall(require, 'zzz'))",
    "package.searchers = nil return select(2, pcall(require, 'zzz'))",
    "package.searchers = {function(n) return nil end} return select(2, pcall(require, 'zzz'))",
    "package.searchers = {function(n) return 'custom msg' end, function() end} return select(2, pcall(require, 'zzz'))",
    "package.searchers = {function(n) return function(a,b) return a..b end, 'data' end} local m, d = require('zzz') return m..'|'..d",
    "return package.config",
    "local a,b = package.searchpath('foo.bar', '@D@/?.x;@D@/?/y.z') return tostring(a)..'|'..b",
    "local f = io.open('@D@/fb.x','w') f:close() return package.searchpath('foo.bar', '@D@/f?.x', '.', '')..''",
    "local f = io.open('@D@/foo_bar.x','w') f:close() return package.searchpath('foo.bar', '@D@/?.x', '.', '_')",
    "return select(2, pcall(package.searchpath))",
    "return tostring(package.loaded._G == _G)..tostring(package.loaded.io == io)..tostring(package.loaded.package == package)..tostring(package.loaded.string == string)",
    "return tostring(package.preload ~= nil)..#package.searchers..type(package.path)..type(package.cpath)",
    "return require('string') == string and 'ok'",
    "package.path = '@D@/?.lua' local f = io.open('@D@/cyc.lua','w') f:write('return nil') f:close() local a = require('cyc') local b = require('cyc') return tostring(a)..tostring(b)",
    "local t = {} for k in pairs(package) do t[#t+1]=k end table.sort(t) return table.concat(t,',')",
    "return select(2, pcall(require))",
];

#[test]
fn io_cases() {
    let bad = compare(IO_CASES);
    assert!(
        bad.is_empty(),
        "{} mismatches:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn io_cases2() {
    let bad = compare(IO_CASES2);
    assert!(
        bad.is_empty(),
        "{} mismatches:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn os_cases() {
    let bad = compare(OS_CASES);
    assert!(
        bad.is_empty(),
        "{} mismatches:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn package_cases() {
    let bad = compare(PKG_CASES);
    assert!(
        bad.is_empty(),
        "{} mismatches:\n{}",
        bad.len(),
        bad.join("\n")
    );
}
