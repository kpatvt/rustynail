//! `string` library: every function x argument shapes (error messages), plus semantic cases,
//! compared against the reference interpreter.
mod libs1_support;
use libs1_support::*;

const ARGS: &[&str] = &[
    "", "nil", "{}", "1", "-1", "1.5", "'a'", "'abc'", "true", "print", "nil,nil", "'a',nil", "'a','b'", "'a',1",
    "'a',1,2", "'a',{}", "{},1", "{},1,2", "1,2", "1,2,3", "'abc',2,1", "'abc','b','c'", "'abc',-1,-5", "'x',2^63",
    "'x',0/0", "'x',3.5", "'x',-2^63", "'%d',1.5", "'%d','x'", "'%s',nil", "'%q',{}", "'%c',256", "'%10.3s','abcdef'",
    "'abc','%',''", "'abc','(',''", "'abc','[a',''", "'abc','%1',''", "'abc','b','%2'", "'abc','b',{}", "'abc','b',true",
    "'abc','b','x',-1", "'abc','b','x',1.5", "'x',1,'y',3", "'abc',function() end", "'>I4',2^40", "'i17',1", "'z','a\\0b'",
    "'s1','abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789'",
    "'<i4','\\1\\2'", "'i4','\\1\\2\\3\\4',10", "'!4 i1 i4',1,2", "'x'", "'<d',1", "'j',2^53", "'Xi3'",
];

const NAMES: &[&str] = &[
    "byte", "char", "dump", "find", "format", "gmatch", "gsub", "len", "lower", "match", "rep",
    "reverse", "sub", "upper", "pack", "packsize", "unpack",
];

#[test]
fn error_matrix() {
    assert_batch(&call_matrix("string", NAMES, ARGS));
}

#[test]
fn method_calls() {
    let mut cases = Vec::new();
    for n in NAMES {
        for self_ in ["nil", "{}", "1", "'abc'"] {
            for a in ["", "1", "'a'", "{}", "1,2"] {
                let sep = if a.is_empty() { "" } else { "," };
                cases.push(format!("local s = {self_}; return select(2, pcall(function() return string.{n}({self_}{sep}{a}) end))"));
                cases.push(format!(
                    "return select(2, pcall(function() local o = setmetatable({{}}, {{__index = string}}); return o:{n}({a}) end))"
                ));
                cases.push(format!(
                    "return select(2, pcall(function() return ('abc'):{n}({a}) end))"
                ));
            }
        }
    }
    assert_batch(&cases);
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn basic_semantics() {
    let cases = s(&[
        "return J(string.byte('abc'), string.byte('abc', 2), string.byte('abc', 1, -1), string.byte('abc', -2, -1), string.byte('', 1), string.byte('abc', 10))",
        "return J(string.char(), string.char(65,66,67), #string.char(0,255))",
        "return J(('abc'):upper(), ('ABC'):lower(), ('abc'):reverse(), ('abc'):len(), #('a\\0b'))",
        "return J(('abc'):sub(2), ('abc'):sub(-2), ('abc'):sub(2,2), ('abc'):sub(0), ('abc'):sub(4), ('abc'):sub(-100,100), ('abc'):sub(3,2), ('abc'):sub(math.mininteger, math.maxinteger))",
        "return J(('ab'):rep(3), ('ab'):rep(3, ','), ('ab'):rep(0), ('ab'):rep(-1), (''):rep(5), (''):rep(5, 'x'), ('x'):rep(1, 'sep'))",
        "return J(string.rep('', 1e8), #string.rep('ab', 1000, 'cd'))",
        "return J(string.find('hello world', 'wor'), string.find('hello', 'l+'), string.find('hello', '(l)(l)'), string.find('hello', 'xyz'), string.find('hello', ''), string.find('hello', '', 10), string.find('hello', '', 6))",
        "return J(string.find('a.b', '.', 1, true), string.find('a.b', '%.'), string.find('abc', 'b', -1), string.find('abc', 'b', -2), string.find('', ''), string.find('abc', '^b'), string.find('abc', '^a'), string.find('abc', 'c$'))",
        "return J(string.match('key=value', '(%w+)=(%w+)'), string.match('abc', '()b()'), string.match('abc', '.-'), string.match('  x  ', '^%s*(.-)%s*$'), string.match('2024-01-02', '(%d+)-(%d+)-(%d+)'))",
        "local t = {} for w in string.gmatch('one two three', '%a+') do t[#t+1] = w end return table.concat(t, '|')",
        "local t = {} for k, v in string.gmatch('a=1, b=2', '(%w+)=(%w+)') do t[#t+1] = k..v end return table.concat(t, '|')",
        "local t = {} for p in string.gmatch('abc', '()') do t[#t+1] = p end return table.concat(t, '|')",
        "local t = {} for w in string.gmatch('abc', '', 2) do t[#t+1] = '<'..w..'>' end return table.concat(t, '|')",
        "local t = {} for w in string.gmatch('abc def', '%a+', -3) do t[#t+1] = w end return table.concat(t, '|')",
        "local t = {} for w in ('x y z'):gmatch('%a', 100) do t[#t+1] = w end return #t",
        "return J(string.gsub('hello world', 'o', '0'), string.gsub('hello', '', '-'), string.gsub('abc', '%w', '%0%0'), string.gsub('abc', '(a)(b)', '%2%1'), string.gsub('abc', 'b', '%%'), string.gsub('hello', 'l', 'L', 1))",
        "return J(string.gsub('abc', '%w', {a='1', b=true, c=false}), string.gsub('abc', '%w', function(c) if c == 'b' then return 'B' end end), string.gsub('abc def', '(%w+)', '<%1>'))",
        "return J(string.gsub('abc', '^a', 'X'), string.gsub('aaa', '^a', 'X'), string.gsub('abc', 'x*', '-'), string.gsub('', '', 'x'), string.gsub('abc', '.', {a=1, b=2.5}))",
        "return J(string.gsub('abc', '(b)', function(...) return select('#', ...) end), string.gsub('abc', 'b()', function(p) return p end))",
        "return J(pcall(string.gsub, 'abc', 'b', function() return {} end))",
        "return J(pcall(string.gsub, 'abc', 'b', function() return print end))",
        "return J(pcall(string.gsub, 'abc', 'b', '%2'))",
        "return J(pcall(string.gsub, 'abc', '(b)', '%9'))",
        "return J(pcall(string.gsub, 'abc', 'b', '%x'))",
        "return J(pcall(string.gsub, 'abc', '(', 'x'))",
        "return J(pcall(string.find, 'abc', '%'))",
        "return J(pcall(string.find, 'abc', '[a'))",
        "return J(pcall(string.match, 'abc', '(()'))",
        "return J(pcall(string.match, 'abc', '%f'))",
        "return J(pcall(string.match, 'abc', '%b'))",
        "return J(string.match('THE (quick) fox', '%((%a+)%)'), string.match('x(a(b)c)y', '%b()'), string.find('THE (quick) fox', '%f[%a]%a+%f[%A]'))",
        "return J(string.rep('a', 3, nil), string.len(123), string.upper(12.5), string.sub(12345, 2, 3), (12):rep(2))",
        "return J(('%d'):format(3), ('%5.2f'):format(3.14159), ('%s|%s'):format(1, 'x'), ('%q'):format('a\\nb\\0c\"'), ('%x'):format(255), ('%X'):format(255), ('%o'):format(8), ('%e'):format(12345.678), ('%g'):format(1e20), ('%a'):format(1))",
        "return J(('%q'):format(1/3), ('%q'):format(math.mininteger), ('%q'):format(1/0), ('%q'):format(-1/0), ('%q'):format(0/0), ('%q'):format(true), ('%q'):format(nil))",
        "return J(('%i'):format(42), ('%u'):format and 'x', ('%c%c'):format(72, 105), ('%-5d|'):format(42), ('%+d'):format(42), ('% d'):format(42), ('%05d'):format(-42), ('%.3d'):format(5), ('%#x'):format(255), ('%.0f'):format(0.5), ('%5s|'):format('ab'), ('%-5s|'):format('ab'), ('%.2s'):format('abcdef'))",
        "return J(pcall(string.format, '%d', 3.5), pcall(string.format, '%d', '10'), pcall(string.format, '%d', '10.0'), pcall(string.format, '%z', 1), pcall(string.format, '%', 1), pcall(string.format, '%10', 1))",
        "return J(pcall(string.format, '%s'), pcall(string.format, '%s %s', 1), pcall(string.format, '%q', print), pcall(string.format, '%123d', 1), pcall(string.format, '%.123d', 1))",
        "local t = setmetatable({}, {__tostring = function() return 'TT' end}) return J(string.format('%s', t), string.format('%10s', t), tostring(t))",
        "local t = setmetatable({}, {__tostring = function() return 1 end}) return J(pcall(string.format, '%s', t))",
        "local t = setmetatable({}, {__tostring = function() return {} end}) return J(pcall(string.format, '%s', t))",
        "local t = setmetatable({}, {__name = 'MyType'}) return (string.format('%s', t):gsub('0x%x+', 'ADDR'))",
        "return J(string.format('%s', nil), string.format('%s', true), string.format('%s', 12), string.format('%s', 1.5), string.format('%d', 3.0))",
        "return J(string.format('%5.1s|', 'abc'), string.format('%s', 'a\\0b') == 'a\\0b', #string.format('%s', 'a\\0b'), #string.format('%-10s', 'a\\0b'))",
        "return J(string.pack('i4', 100):byte(1, -1))",
        "return J(string.pack('>i4', 100):byte(1, -1))",
        "return J(string.unpack('<i2', '\\1\\2'), string.unpack('>I2', '\\1\\2'), string.unpack('z', 'abc\\0def'), string.unpack('s1', '\\3abcdef'), string.unpack('<d', string.pack('<d', 1.5)))",
        "return J(string.packsize('i4i8'), string.packsize('!8i4i8'), string.packsize('<i1i2i4'), pcall(string.packsize, 's'), pcall(string.packsize, 'z'))",
        "return J(string.unpack('i4', string.pack('i4', -2)), string.unpack('b', '\\255'), string.unpack('B', '\\255'), string.unpack('h', string.pack('h', -3)), string.unpack('<j', string.pack('<j', math.mininteger)))",
        "return J(pcall(string.unpack, 'i4', 'abc'), pcall(string.unpack, 'i4', 'abcd', 2), pcall(string.unpack, 'i4', 'abcd', 10), pcall(string.unpack, 'z', 'abc'), pcall(string.unpack, 's4', '\\255\\255\\255\\255'))",
        "return J(pcall(string.pack, 'i4'), pcall(string.pack, 'i4', 'x'), pcall(string.pack, 'i1', 200), pcall(string.pack, 'I1', -1), pcall(string.pack, 'z', 'a\\0b'), pcall(string.pack, 's1', string.rep('x', 300)))",
        "return J(string.pack('i4i4', 1), 1)",
        "return J(pcall(string.pack, 'i4i4', 1))",
        "return J(string.unpack('i4', string.pack('i4', 7), 1), string.unpack('c3', 'abcdef', 2), string.unpack('<i3', string.pack('<i3', -5)), string.unpack('<I16', string.pack('<I16', 1)))",
        "return J(string.unpack('=I4 !4 I1 Xi4 I4', string.pack('=I4 !4 I1 Xi4 I4', 1, 2, 3)))",
        "return J(#string.pack('s', 'hello'), #string.pack('s2', 'hello'), string.pack('c5', 'ab'):byte(1, -1))",
        "return J(pcall(string.pack, 'c2', 'abc'), pcall(string.pack, 'c', 'abc'))",
        "return J(string.pack('f', 1.5):byte(1, -1))",
        "return J(string.dump(function() end) ~= nil, pcall(string.dump, print), pcall(string.dump))",
        "local f = load(string.dump(function(a, b) return a + b end)) return f(3, 4)",
        "local f = load(string.dump(function(a, b) local x = a * b return x + 1 end, true)) return f(3, 4)",
        "local up = 10 local f = function() return up end local g = load(string.dump(f)) return J(pcall(g))",
        "local f = load(string.dump(function(...) return select('#', ...), ... end)) return J(f(1, nil, 3))",
        "local f = load(string.dump(function() return 'abc', 1.5, 2^63, 'x'..'y', nil, true end)) return J(f())",
        "local f = load(string.dump(function() local function inner(n) if n == 0 then return 0 end return n + inner(n-1) end return inner(10) end)) return f()",
        "return J('10' + 5, '3' * '4', 10 .. '', '2' ^ 2, '10' // '3', '10' % '3', -'2', '0x10' + 0, ' 5 ' + 1, '1e1' + 0, 10 / '4')",
        "return J(pcall(function() return 'a' + 1 end))",
        "return J(pcall(function() return {} + 'x' end))",
        "return J(pcall(function() return 'x' + {} end))",
        "return J(pcall(function() return 'x' .. {} end))",
        "return J(pcall(function() return -'x' end))",
        "return J(pcall(function() return '10' % 0 end))",
        "return J(pcall(function() return '10' // 0 end))",
        "return J('10' % 0.0, '10' // 0.0, '7' // '2', '7.5' // 2, 2^63 .. '')",
        "return J(math.type('10' + 0), math.type('10.0' + 0), math.type('0x10' * 1), '9223372036854775807' + 1, '9223372036854775808' + 0)",
        "local mt = getmetatable('') return J(type(mt), mt.__index == string, type(mt.__add), type(mt.__unm), mt.__mod ~= nil, mt.__concat)",
        "local t = setmetatable({}, {__add = function(a, b) return 'meta' end}) return J('x' + t, t + 'x', '10' + t)",
        "local t = setmetatable({}, {__unm = function(a, b) return 'um' end}) return J(pcall(string.__unm, t))",
        "return J(pcall(string.rep, 'x', 1e100))",
        "return J(pcall(string.rep, 'x', math.maxinteger // 2), pcall(string.rep, 'x', 1 << 40), pcall(string.rep, 'xyz', 1 << 62), pcall(string.rep, '', 1 << 62), #select(2, pcall(string.rep, 'x', 1 << 24, 'y')))",
        "return J(pcall(string.rep, 'xx', math.maxinteger))",
        "return J(pcall(string.rep, 'x', math.maxinteger, 'y'))",
        "return J(#string.rep('x', 100000), #string.rep('xy', 50000, 'z'))",
        "return J(pcall(string.byte, 'abc', 1, math.maxinteger))",
        "return J(string.byte('abc', math.mininteger, math.maxinteger))",
        "return J(pcall(string.char, 256), pcall(string.char, -1), pcall(string.char, 'x'), pcall(string.char, 65, 'x'), pcall(string.char, 65.5))",
        "return J(string.format('%.14g', 0.1), string.format('%.17g', 0.1), string.format('%10.4f|', math.pi), string.format('%-10.4f|', -math.pi), string.format('%+.3e', 12345.6789))",
        "return J(tostring(1e15), tostring(-0.0), tostring(1e100), tostring(2^53), tostring(0/0) == tostring(0/0), 1e15 .. '', 2^63 .. '', math.mininteger .. '')",
        "return J(('x'):rep(3):len(), #('x'):rep(3, ''), ('abc'):find('b', 1, true), ('a+b'):find('+', 1, true), ('a+b'):find('a+', 1, true))",
        "return J(string.find('abc', 'b', nil, true), string.find('abc', 'b', nil, false), string.find('a\\0b', '\\0'), string.find('a\\0b', '%z'), string.find('a\\0b', 'b', 2))",
        "return J(string.match('abc', '^(a)(b)(c)$'), string.match('abc', '^(a)(x)?'), string.match('  abc', '^%s+'), string.match('abc123', '%a+(%d+)'), string.match('foo.bar', '^(.-)%.(.*)$'))",
        "return J(string.gsub('hello world', '%w+', string.upper), string.gsub('abc', '.', string.byte), string.gsub('a b', ' ', '%%20'))",
        "return J(string.gsub('abc', '', '-', 2), string.gsub('abc', 'b*', 'X'), string.gsub('abc', 'b-', 'X'), string.gsub('', 'x*', 'Y'))",
        "return J(pcall(string.gsub, 'abc', 'b', 'x', 'q'), pcall(string.gsub, 'abc', 'b', 'x', -1), pcall(string.gsub, 'abc', 'b', 'x', 0))",
        "local s = string.rep('a', 300000) return J(pcall(string.find, s, string.rep('a?', 300000) .. string.rep('a', 300000)))",
        "return J(pcall(string.match, string.rep('a', 1000), string.rep('(a)', 33)))",
        "return J(pcall(string.rep, 'ab', 3, 5), pcall(string.rep, 'ab', '3'), pcall(string.rep, 5, 2))",
        "local t = {} for i = 1, 100 do t[i] = string.char(i) end local s = table.concat(t) return J(#s, s:byte(100), s:sub(98, 100):byte(1, -1))",
        "return J(string.format('%q', 'tab\\there\\r\\n\\200\\0001'), string.format('%q', '\\\\'))",
        "return J(string.format('%-3c|', 65), string.format('%3c|', 65), pcall(string.format, '%#c', 65))",
        "return J(string.format('%.3q', 'x'))",
        "return J(pcall(string.format, '%.3q', 'x'), pcall(string.format, '%10q', 'x'))",
        "return J(string.format('%p', 1), string.format('%p', nil), string.format('%p', true), #string.format('%p', {}) > 2, string.format('%10p|', 1))",
        "return J(string.format('%i', -0), string.format('%d', -0.0), string.format('%x', -1), string.format('%5.3d', 7), string.format('%o', -1))",
        "return J(string.format('%.20f', 0.1), string.format('%.99f', 1/3):len(), string.format('%99.99f', 1/3):len())",
        "return J(#string.format('%099d', 7), pcall(string.format, '%0100d', 7))",
        "return J(string.format('%a', 0.5), string.format('%A', 255.5), string.format('%.2a', 1/3))",
        "return J(string.format('%g', 100000), string.format('%g', 1e-5), string.format('%G', 1e-10), string.format('%g', 0/0) , string.format('%f', 1/0), string.format('%f', -1/0), string.format('%5.1f|', 0/0))",
        "return J(string.format('%s %s %s', 1, 2, 3), string.format('%%'), string.format('%5%'))",
        "return J(string.format('%s', 'abc', 'extra'), string.format('abc'), string.format(''))",
    ]);
    // 5.4: `string.rep('', n)` really loops `n` times in the reference (5.5 returns at once):
    // leave out the 2^62-iteration case
    let cases: Vec<String> = cases
        .into_iter()
        .map(|c| {
            if cfg!(rustynail_lua54) {
                c.replace("pcall(string.rep, '', 1 << 62), ", "")
            } else {
                c
            }
        })
        .collect();
    assert_batch(&cases);
}

#[test]
fn string_indexing_and_misc() {
    let cases = s(&[
        "return J(('abc').len, ('abc').x, #'abc', ('abc'):len())",
        "return J(pcall(function() return ('abc').x.y end))",
        "return J(pcall(function() local s = 'abc'; s.x = 1 end))",
        "return J(pcall(function() return #5 end))",
        "return J(rawequal('a', 'a'), 'a' < 'b', 'a' < 'B', 'a\\0b' < 'a\\0c', '' < 'a', 'abc' <= 'abd', 'Z' < 'a')",
        "return J(pcall(function() return 'a' < 1 end))",
        "return J(tostring('x'), tostring(12), tostring(-0.0), tonumber('  10  '), tonumber('0x1p4'), tonumber('1e'), tonumber(''), tonumber('1 2'))",
        "return J(string.format('%s', string.rep('x', 10)), ('x'):rep(3) == 'xxx')",
        "local a = {} for i = 1, 200 do a[i] = ('x'):rep(i) end local n = 0 for i = 1, 200 do n = n + #a[i] end return n",
        "collectgarbage() local t = {} for i = 1, 2000 do t[i] = ('k'..i):rep(3) end collectgarbage() return #t[2000]",
        "local r = {} for w in ('a,b,,c'):gmatch('([^,]*)') do r[#r+1] = '['..w..']' end return table.concat(r)",
        "local it = string.gmatch('abc', '.') return J(it(), it(), it(), it(), it())",
        "local it = string.gmatch('abc', '(.)(.)') return J(it(), it())",
        "return J(pcall(string.gmatch('abc', '(')))",
        "return J(pcall(string.gmatch, 'abc', '('))",
        "return string.gsub('abc', '%f[%w]', '|')",
        "return (string.gsub('one two', '(%w+) (%w+)', '%2 %1'))",
        "local r = {} string.gsub('abc', '.', function(c) r[#r+1] = c; return nil end) return table.concat(r)",
        "return (string.gsub('abc', '.', function(c) error('boom') end))",
        "return (string.gsub('abc', '.', setmetatable({}, {__index = function(t, k) return k:upper() end})))",
        "return (string.gsub('abc', '.', setmetatable({}, {__index = function(t, k) error('idx') end})))",
        "local co = coroutine and 1 return 1",
    ]);
    assert_batch(&cases);
}
