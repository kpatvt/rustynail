//! Lexical analyzer (port of `llex.c` / `llex.h`, Lua 5.5 with the 5.4 reserved-word set under
//! `rustynail_lua54`). Errors are returned as [`CompileError`] with the reference message text.
use crate::CompileError;
use rn_core::host::CompilerHost;
use rn_core::number::{str2num, Number};

/// `LUA_IDSIZE`
pub const LUA_IDSIZE: usize = 60;

/// In 5.5 `global` is only a contextual keyword (`LUA_COMPAT_GLOBAL` defaults to 1 in `luaconf.h`):
/// the lexer returns it as an ordinary name and the parser checks it against `LexState::glbn`.
#[cfg(not(rustynail_lua54))]
pub const COMPAT_GLOBAL: bool = true;
#[cfg(rustynail_lua54)]
pub const COMPAT_GLOBAL: bool = false;

/// A token with its semantic value (`Token` + `SemInfo`). Single-byte tokens are `Char`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Token<S> {
    Char(u8),
    // reserved words
    And,
    Break,
    Do,
    Else,
    Elseif,
    End,
    False,
    For,
    Function,
    /// Only produced when `COMPAT_GLOBAL` is false (never in 5.4).
    Global,
    Goto,
    If,
    In,
    Local,
    Nil,
    Not,
    Or,
    Repeat,
    Return,
    Then,
    True,
    Until,
    While,
    // other terminal symbols
    IDiv,
    Concat,
    Dots,
    Eq,
    Ge,
    Le,
    Ne,
    Shl,
    Shr,
    DbColon,
    Eos,
    Flt(f64),
    Int(i64),
    Name(S),
    Str(S),
}

/// Reserved words in `RESERVED` order (the `Token` variants `And..=While` minus `Global` on 5.4).
const RESERVED: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "global", "goto",
    "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

impl<S> Token<S> {
    #[cfg_attr(rustynail_lua54, allow(dead_code))]
    fn reserved(word: &[u8]) -> Option<Token<S>> {
        Some(match word {
            b"and" => Token::And,
            b"break" => Token::Break,
            b"do" => Token::Do,
            b"else" => Token::Else,
            b"elseif" => Token::Elseif,
            b"end" => Token::End,
            b"false" => Token::False,
            b"for" => Token::For,
            b"function" => Token::Function,
            b"global" if !COMPAT_GLOBAL && cfg!(not(rustynail_lua54)) => Token::Global,
            b"goto" => Token::Goto,
            b"if" => Token::If,
            b"in" => Token::In,
            b"local" => Token::Local,
            b"nil" => Token::Nil,
            b"not" => Token::Not,
            b"or" => Token::Or,
            b"repeat" => Token::Repeat,
            b"return" => Token::Return,
            b"then" => Token::Then,
            b"true" => Token::True,
            b"until" => Token::Until,
            b"while" => Token::While,
            _ => return None,
        })
    }

    /// [`Token::reserved`] without byte-string compares: every reserved word has at most 8
    /// bytes, so the word is packed into a little-endian `u64` and matched as an integer.
    #[cfg(rustynail_lua54)]
    #[inline]
    fn reserved_packed(word: &[u8]) -> Option<Token<S>> {
        const fn pack(w: &[u8]) -> u64 {
            let mut v = 0u64;
            let mut i = 0;
            while i < w.len() {
                v |= (w[i] as u64) << (8 * i);
                i += 1;
            }
            v
        }
        if word.len() < 2 || word.len() > 8 {
            return None;
        }
        // (byte loads: the word was just stored byte by byte, so a wider load would stall on
        // store forwarding)
        let mut v = 0u64;
        for (i, &b) in word.iter().enumerate() {
            v |= u64::from(b) << (8 * i);
        }
        const AND: u64 = pack(b"and");
        const BREAK: u64 = pack(b"break");
        const DO: u64 = pack(b"do");
        const ELSE: u64 = pack(b"else");
        const ELSEIF: u64 = pack(b"elseif");
        const END: u64 = pack(b"end");
        const FALSE: u64 = pack(b"false");
        const FOR: u64 = pack(b"for");
        const FUNCTION: u64 = pack(b"function");
        const GOTO: u64 = pack(b"goto");
        const IF: u64 = pack(b"if");
        const IN: u64 = pack(b"in");
        const LOCAL: u64 = pack(b"local");
        const NIL: u64 = pack(b"nil");
        const NOT: u64 = pack(b"not");
        const OR: u64 = pack(b"or");
        const REPEAT: u64 = pack(b"repeat");
        const RETURN: u64 = pack(b"return");
        const THEN: u64 = pack(b"then");
        const TRUE: u64 = pack(b"true");
        const UNTIL: u64 = pack(b"until");
        const WHILE: u64 = pack(b"while");
        Some(match v {
            AND => Token::And,
            BREAK => Token::Break,
            DO => Token::Do,
            ELSE => Token::Else,
            ELSEIF => Token::Elseif,
            END => Token::End,
            FALSE => Token::False,
            FOR => Token::For,
            FUNCTION => Token::Function,
            GOTO => Token::Goto,
            IF => Token::If,
            IN => Token::In,
            LOCAL => Token::Local,
            NIL => Token::Nil,
            NOT => Token::Not,
            OR => Token::Or,
            REPEAT => Token::Repeat,
            RETURN => Token::Return,
            THEN => Token::Then,
            TRUE => Token::True,
            UNTIL => Token::Until,
            WHILE => Token::While,
            _ => return None,
        })
    }

    /// Text of fixed-format tokens (symbols and reserved words); `None` for `Char` and value tokens.
    fn fixed_text(&self) -> Option<&'static str> {
        Some(match self {
            Token::And => RESERVED[0],
            Token::Break => RESERVED[1],
            Token::Do => RESERVED[2],
            Token::Else => RESERVED[3],
            Token::Elseif => RESERVED[4],
            Token::End => RESERVED[5],
            Token::False => RESERVED[6],
            Token::For => RESERVED[7],
            Token::Function => RESERVED[8],
            Token::Global => RESERVED[9],
            Token::Goto => RESERVED[10],
            Token::If => RESERVED[11],
            Token::In => RESERVED[12],
            Token::Local => RESERVED[13],
            Token::Nil => RESERVED[14],
            Token::Not => RESERVED[15],
            Token::Or => RESERVED[16],
            Token::Repeat => RESERVED[17],
            Token::Return => RESERVED[18],
            Token::Then => RESERVED[19],
            Token::True => RESERVED[20],
            Token::Until => RESERVED[21],
            Token::While => RESERVED[22],
            Token::IDiv => "//",
            Token::Concat => "..",
            Token::Dots => "...",
            Token::Eq => "==",
            Token::Ge => ">=",
            Token::Le => "<=",
            Token::Ne => "~=",
            Token::Shl => "<<",
            Token::Shr => ">>",
            Token::DbColon => "::",
            _ => return None,
        })
    }

    /// `luaX_token2str`.
    pub fn to_text(&self) -> Vec<u8> {
        match self {
            Token::Char(c) => {
                if (0x20..0x7f).contains(c) {
                    vec![b'\'', *c, b'\'']
                } else {
                    format!("'<\\{c}>'").into_bytes()
                }
            }
            Token::Eos => b"<eof>".to_vec(),
            Token::Flt(_) => b"<number>".to_vec(),
            Token::Int(_) => b"<integer>".to_vec(),
            Token::Name(_) => b"<name>".to_vec(),
            Token::Str(_) => b"<string>".to_vec(),
            t => format!("'{}'", t.fixed_text().unwrap_or("?")).into_bytes(),
        }
    }
}

const EOZ: i32 = -1;
const UTF8BUFFSZ: usize = 8;

fn is_alpha(c: i32) -> bool {
    (0..256).contains(&c) && ((c as u8).is_ascii_alphabetic() || c == b'_' as i32)
}
fn is_alnum(c: i32) -> bool {
    is_alpha(c) || is_digit(c)
}
fn is_digit(c: i32) -> bool {
    (b'0' as i32..=b'9' as i32).contains(&c)
}
fn is_xdigit(c: i32) -> bool {
    (0..256).contains(&c) && (c as u8).is_ascii_hexdigit()
}
fn is_space(c: i32) -> bool {
    c == 32 || (9..=13).contains(&c)
}
fn hexavalue(c: i32) -> i32 {
    if is_digit(c) {
        c - b'0' as i32
    } else {
        (c | 0x20) - b'a' as i32 + 10
    }
}

fn cstr(s: &[u8]) -> &[u8] {
    match s.iter().position(|&c| c == 0) {
        Some(i) => &s[..i],
        None => s,
    }
}

/// `luaO_chunkid`: printable chunk name from a `source` string (`@file`, `=name`, or literal source).
pub fn chunk_id(source: &[u8]) -> Vec<u8> {
    rn_core::debuginfo::chunk_id(source)
}

fn utf8esc_bytes(mut x: u32) -> Vec<u8> {
    let mut buff = [0u8; UTF8BUFFSZ];
    let mut n = 1;
    if x < 0x80 {
        buff[UTF8BUFFSZ - 1] = x as u8;
    } else {
        let mut mfb: u32 = 0x3f;
        loop {
            buff[UTF8BUFFSZ - n] = (0x80 | (x & 0x3f)) as u8;
            n += 1;
            x >>= 6;
            mfb >>= 1;
            if x <= mfb {
                break;
            }
        }
        buff[UTF8BUFFSZ - n] = ((!mfb << 1) | x) as u8;
    }
    buff[UTF8BUFFSZ - n..].to_vec()
}

type R<T> = Result<T, CompileError>;

/// Scanner state (`LexState`). The parser (task A2) owns one of these and drives it with
/// [`LexState::next_token`] / [`LexState::lookahead`].
pub struct LexState<'a, H: CompilerHost> {
    pub host: &'a mut H,
    input: &'a [u8],
    pos: usize,
    /// current character (`EOZ` = -1 at end of input)
    current: i32,
    /// input line counter
    pub line_number: i32,
    /// line of last token consumed
    pub last_line: i32,
    /// current token
    pub t: Token<H::Str>,
    /// look-ahead token (`Token::Eos` = none)
    pub ahead: Token<H::Str>,
    buff: Vec<u8>,
    /// current source name (as given to `load`, e.g. `@file`, `=stdin`, or source text)
    pub source: Vec<u8>,
    pub envn: H::Str,
    pub brkn: H::Str,
    pub glbn: H::Str,
    long_strs: crate::khash::KMap<Vec<u8>, H::Str>,
}

impl<'a, H: CompilerHost> LexState<'a, H> {
    /// `luaX_setinput`. `input` is the chunk text *after* any shebang line the caller skipped.
    pub fn new(host: &'a mut H, input: &'a [u8], source: &[u8]) -> R<Self> {
        let mem = |_| CompileError {
            message: b"not enough memory".to_vec(),
        };
        let envn = host.intern(b"_ENV").map_err(mem)?;
        let brkn = host.intern(b"break").map_err(mem)?;
        let glbn = host.intern(b"global").map_err(mem)?;
        let mut ls = LexState {
            host,
            input,
            pos: 0,
            current: EOZ,
            line_number: 1,
            last_line: 1,
            t: Token::Char(0),
            ahead: Token::Eos,
            buff: Vec::with_capacity(32),
            source: source.to_vec(),
            envn,
            brkn,
            glbn,
            long_strs: Default::default(),
        };
        ls.advance();
        Ok(ls)
    }

    #[inline]
    fn advance(&mut self) {
        self.current = match self.input.get(self.pos) {
            Some(&c) => {
                self.pos += 1;
                i32::from(c)
            }
            None => EOZ,
        };
    }

    #[inline]
    fn save(&mut self, c: i32) {
        self.buff.push(c as u8);
    }

    #[inline]
    fn save_and_next(&mut self) {
        self.save(self.current);
        self.advance();
    }

    fn curr_is_newline(&self) -> bool {
        self.current == b'\n' as i32 || self.current == b'\r' as i32
    }

    fn new_string(&mut self, bytes: &[u8]) -> R<H::Str> {
        // `luaX_newstring` anchors every string in the scanner table `ls->h`, so equal long
        // strings in one chunk share a single object (short ones are interned by the host).
        const MAXSHORTLEN: usize = 40;
        if bytes.len() > MAXSHORTLEN {
            if let Some(&s) = self.long_strs.get(bytes) {
                return Ok(s);
            }
        }
        let s = self.host.intern(bytes).map_err(|_| CompileError {
            message: b"not enough memory".to_vec(),
        })?;
        if bytes.len() > MAXSHORTLEN {
            self.long_strs.insert(bytes.to_vec(), s);
        }
        Ok(s)
    }

    /// `txtToken`
    fn txt_token(&self, tok: &Token<H::Str>) -> Vec<u8> {
        match tok {
            Token::Name(_) | Token::Str(_) | Token::Flt(_) | Token::Int(_) => {
                let mut v = vec![b'\''];
                v.extend_from_slice(cstr(&self.buff));
                v.push(b'\'');
                v
            }
            t => t.to_text(),
        }
    }

    /// `lexerror`: `chunk:LINE: msg [near tok]`.
    pub fn lex_error(&self, msg: &[u8], token: Option<&Token<H::Str>>) -> CompileError {
        let mut m = chunk_id(&self.source);
        m.push(b':');
        m.extend_from_slice(self.line_number.to_string().as_bytes());
        m.extend_from_slice(b": ");
        m.extend_from_slice(msg);
        // `if (token)`: token 0 (a NUL byte in the source, or the `luaK_semerror` reset) adds no "near"
        if let Some(t) = token.filter(|t| !matches!(t, Token::Char(0))) {
            m.extend_from_slice(b" near ");
            m.extend_from_slice(&self.txt_token(t));
        }
        CompileError { message: m }
    }

    /// `luaK_semerror`: semantic error without a "near `<token>`" suffix. In 5.5 it is reported at the
    /// line of the last consumed token; 5.4 uses the current line.
    pub fn sem_error(&mut self, msg: &[u8]) -> CompileError {
        self.t = Token::Char(0);
        #[cfg(not(rustynail_lua54))]
        {
            self.line_number = self.last_line;
        }
        self.lex_error(msg, None)
    }

    /// `luaX_syntaxerror`: error attributed to the current token.
    pub fn syntax_error(&self, msg: &str) -> CompileError {
        self.lex_error(msg.as_bytes(), Some(&self.t))
    }

    /// `luaX_token2str`
    pub fn token2str(&self, tok: &Token<H::Str>) -> Vec<u8> {
        tok.to_text()
    }

    /// `inclinenumber`
    fn inc_line_number(&mut self) -> R<()> {
        let old = self.current;
        self.advance();
        if self.curr_is_newline() && self.current != old {
            self.advance();
        }
        self.line_number += 1;
        if self.line_number == i32::MAX {
            return Err(self.lex_error(b"chunk has too many lines", None));
        }
        Ok(())
    }

    fn check_next1(&mut self, c: u8) -> bool {
        if self.current == i32::from(c) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check_next2(&mut self, set: &[u8; 2]) -> bool {
        if self.current == i32::from(set[0]) || self.current == i32::from(set[1]) {
            self.save_and_next();
            true
        } else {
            false
        }
    }

    fn read_numeral(&mut self) -> R<Token<H::Str>> {
        let mut expo = b"Ee";
        let first = self.current;
        self.save_and_next();
        if first == b'0' as i32 && self.check_next2(b"xX") {
            expo = b"Pp";
        }
        loop {
            if self.check_next2(expo) {
                self.check_next2(b"-+");
            } else if is_xdigit(self.current) || self.current == b'.' as i32 {
                self.save_and_next();
            } else {
                break;
            }
        }
        if is_alpha(self.current) {
            self.save_and_next();
        }
        match str2num(&self.buff) {
            None => Err(self.lex_error(b"malformed number", Some(&Token::Flt(0.0)))),
            Some(Number::Int(i)) => Ok(Token::Int(i)),
            Some(Number::Float(f)) => Ok(Token::Flt(f)),
        }
    }

    /// `skip_sep`
    fn skip_sep(&mut self) -> usize {
        let mut count = 0usize;
        let s = self.current;
        self.save_and_next();
        while self.current == b'=' as i32 {
            self.save_and_next();
            count += 1;
        }
        if self.current == s {
            count + 2
        } else if count == 0 {
            1
        } else {
            0
        }
    }

    fn read_long_string(&mut self, want: bool, sep: usize) -> R<Option<H::Str>> {
        let line = self.line_number;
        self.save_and_next();
        if self.curr_is_newline() {
            self.inc_line_number()?;
        }
        loop {
            match self.current {
                EOZ => {
                    let what = if want { "string" } else { "comment" };
                    let msg = format!("unfinished long {what} (starting at line {line})");
                    return Err(self.lex_error(msg.as_bytes(), Some(&Token::Eos)));
                }
                c if c == b']' as i32 => {
                    if self.skip_sep() == sep {
                        self.save_and_next();
                        break;
                    }
                }
                c if c == b'\n' as i32 || c == b'\r' as i32 => {
                    self.save(b'\n' as i32);
                    self.inc_line_number()?;
                    if !want {
                        self.buff.clear();
                    }
                }
                _ => {
                    if want {
                        self.save_and_next();
                    } else {
                        self.advance();
                    }
                }
            }
        }
        if want {
            let bytes = self.buff[sep..self.buff.len() - sep].to_vec();
            Ok(Some(self.new_string(&bytes)?))
        } else {
            Ok(None)
        }
    }

    fn esc_check(&mut self, ok: bool, msg: &str) -> R<()> {
        if !ok {
            if self.current != EOZ {
                self.save_and_next();
            }
            return Err(self.lex_error(msg.as_bytes(), Some(&Token::Str(self.envn))));
        }
        Ok(())
    }

    fn get_hexa(&mut self) -> R<i32> {
        self.save_and_next();
        self.esc_check(is_xdigit(self.current), "hexadecimal digit expected")?;
        Ok(hexavalue(self.current))
    }

    fn buff_remove(&mut self, n: usize) {
        let l = self.buff.len().saturating_sub(n);
        self.buff.truncate(l);
    }

    fn read_hexa_esc(&mut self) -> R<i32> {
        let mut r = self.get_hexa()?;
        r = (r << 4) + self.get_hexa()?;
        self.buff_remove(2);
        Ok(r)
    }

    fn read_utf8_esc(&mut self) -> R<u32> {
        let mut i = 4usize;
        self.save_and_next();
        self.esc_check(self.current == b'{' as i32, "missing '{'")?;
        let mut r = self.get_hexa()? as u32;
        loop {
            self.save_and_next();
            if !is_xdigit(self.current) {
                break;
            }
            i += 1;
            self.esc_check(r <= (0x7FFF_FFFFu32 >> 4), "UTF-8 value too large")?;
            r = (r << 4) + hexavalue(self.current) as u32;
        }
        self.esc_check(self.current == b'}' as i32, "missing '}'")?;
        self.advance();
        self.buff_remove(i);
        Ok(r)
    }

    fn read_dec_esc(&mut self) -> R<i32> {
        let mut r = 0i32;
        let mut i = 0usize;
        while i < 3 && is_digit(self.current) {
            r = 10 * r + self.current - b'0' as i32;
            self.save_and_next();
            i += 1;
        }
        self.esc_check(r <= 255, "decimal escape too large")?;
        self.buff_remove(i);
        Ok(r)
    }

    fn read_string(&mut self, del: i32) -> R<Token<H::Str>> {
        self.save_and_next();
        while self.current != del {
            match self.current {
                EOZ => return Err(self.lex_error(b"unfinished string", Some(&Token::Eos))),
                c if c == b'\n' as i32 || c == b'\r' as i32 => {
                    return Err(self.lex_error(b"unfinished string", Some(&Token::Str(self.envn))));
                }
                c if c == b'\\' as i32 => {
                    self.save_and_next();
                    // Some(c) = remove '\\' and save c (after `next` if `adv`); None = nothing to save
                    enum Act {
                        Esc(i32),
                        Raw(i32),
                        Skip,
                    }
                    let act = match self.current {
                        c if c == b'a' as i32 => Act::Esc(7),
                        c if c == b'b' as i32 => Act::Esc(8),
                        c if c == b'f' as i32 => Act::Esc(12),
                        c if c == b'n' as i32 => Act::Esc(10),
                        c if c == b'r' as i32 => Act::Esc(13),
                        c if c == b't' as i32 => Act::Esc(9),
                        c if c == b'v' as i32 => Act::Esc(11),
                        c if c == b'x' as i32 => Act::Esc(self.read_hexa_esc()?),
                        c if c == b'u' as i32 => {
                            let v = self.read_utf8_esc()?;
                            for b in utf8esc_bytes(v) {
                                self.save(i32::from(b));
                            }
                            Act::Skip
                        }
                        c if c == b'\n' as i32 || c == b'\r' as i32 => {
                            self.inc_line_number()?;
                            Act::Raw(b'\n' as i32)
                        }
                        c if c == b'\\' as i32 || c == b'"' as i32 || c == b'\'' as i32 => {
                            Act::Esc(c)
                        }
                        EOZ => Act::Skip,
                        c if c == b'z' as i32 => {
                            self.buff_remove(1);
                            self.advance();
                            while is_space(self.current) {
                                if self.curr_is_newline() {
                                    self.inc_line_number()?;
                                } else {
                                    self.advance();
                                }
                            }
                            Act::Skip
                        }
                        _ => {
                            self.esc_check(is_digit(self.current), "invalid escape sequence")?;
                            Act::Raw(self.read_dec_esc()?)
                        }
                    };
                    match act {
                        Act::Esc(c) => {
                            self.advance();
                            self.buff_remove(1);
                            self.save(c);
                        }
                        Act::Raw(c) => {
                            self.buff_remove(1);
                            self.save(c);
                        }
                        Act::Skip => {}
                    }
                }
                _ => self.save_and_next(),
            }
        }
        self.save_and_next();
        #[cfg(not(rustynail_lua54))]
        {
            let bytes = self.buff[1..self.buff.len() - 1].to_vec();
            Ok(Token::Str(self.new_string(&bytes)?))
        }
        #[cfg(rustynail_lua54)]
        {
            let word = std::mem::take(&mut self.buff);
            let s = self.new_string(&word[1..word.len() - 1]);
            self.buff = word;
            Ok(Token::Str(s?))
        }
    }

    /// `llex`
    fn llex(&mut self) -> R<Token<H::Str>> {
        self.buff.clear();
        loop {
            let c = self.current;
            if c == EOZ {
                return Ok(Token::Eos);
            }
            let ch = c as u8;
            match ch {
                b'\n' | b'\r' => self.inc_line_number()?,
                b' ' | 0x0c | b'\t' | 0x0b => self.advance(),
                b'-' => {
                    self.advance();
                    if self.current != b'-' as i32 {
                        return Ok(Token::Char(b'-'));
                    }
                    self.advance();
                    if self.current == b'[' as i32 {
                        let sep = self.skip_sep();
                        self.buff.clear();
                        if sep >= 2 {
                            self.read_long_string(false, sep)?;
                            self.buff.clear();
                            continue;
                        }
                    }
                    while !self.curr_is_newline() && self.current != EOZ {
                        self.advance();
                    }
                }
                b'[' => {
                    let sep = self.skip_sep();
                    if sep >= 2 {
                        let s = self.read_long_string(true, sep)?;
                        return match s {
                            Some(s) => Ok(Token::Str(s)),
                            None => Ok(Token::Eos),
                        };
                    } else if sep == 0 {
                        return Err(self.lex_error(
                            b"invalid long string delimiter",
                            Some(&Token::Str(self.envn)),
                        ));
                    }
                    return Ok(Token::Char(b'['));
                }
                b'=' => {
                    self.advance();
                    return Ok(if self.check_next1(b'=') {
                        Token::Eq
                    } else {
                        Token::Char(b'=')
                    });
                }
                b'<' => {
                    self.advance();
                    return Ok(if self.check_next1(b'=') {
                        Token::Le
                    } else if self.check_next1(b'<') {
                        Token::Shl
                    } else {
                        Token::Char(b'<')
                    });
                }
                b'>' => {
                    self.advance();
                    return Ok(if self.check_next1(b'=') {
                        Token::Ge
                    } else if self.check_next1(b'>') {
                        Token::Shr
                    } else {
                        Token::Char(b'>')
                    });
                }
                b'/' => {
                    self.advance();
                    return Ok(if self.check_next1(b'/') {
                        Token::IDiv
                    } else {
                        Token::Char(b'/')
                    });
                }
                b'~' => {
                    self.advance();
                    return Ok(if self.check_next1(b'=') {
                        Token::Ne
                    } else {
                        Token::Char(b'~')
                    });
                }
                b':' => {
                    self.advance();
                    return Ok(if self.check_next1(b':') {
                        Token::DbColon
                    } else {
                        Token::Char(b':')
                    });
                }
                b'"' | b'\'' => return self.read_string(c),
                b'.' => {
                    self.save_and_next();
                    if self.check_next1(b'.') {
                        return Ok(if self.check_next1(b'.') {
                            Token::Dots
                        } else {
                            Token::Concat
                        });
                    } else if !is_digit(self.current) {
                        return Ok(Token::Char(b'.'));
                    } else {
                        return self.read_numeral();
                    }
                }
                b'0'..=b'9' => return self.read_numeral(),
                _ => {
                    if is_alpha(c) {
                        loop {
                            self.save_and_next();
                            if !is_alnum(self.current) {
                                break;
                            }
                        }
                        #[cfg(not(rustynail_lua54))]
                        {
                            if let Some(t) = Token::reserved(&self.buff) {
                                return Ok(t);
                            }
                            let word = self.buff.clone();
                            return Ok(Token::Name(self.new_string(&word)?));
                        }
                        #[cfg(rustynail_lua54)]
                        {
                            if let Some(t) = Token::reserved_packed(&self.buff) {
                                return Ok(t);
                            }
                            // (the buffer is lent to `new_string`, not copied)
                            let word = std::mem::take(&mut self.buff);
                            let s = self.new_string(&word);
                            self.buff = word;
                            return Ok(Token::Name(s?));
                        }
                    }
                    self.advance();
                    return Ok(Token::Char(ch));
                }
            }
        }
    }

    /// `luaX_next`: advance to the next token.
    pub fn next_token(&mut self) -> R<()> {
        self.last_line = self.line_number;
        #[cfg(rustynail_lua54)]
        let has_ahead = !matches!(self.ahead, Token::Eos);
        #[cfg(not(rustynail_lua54))]
        let has_ahead = self.ahead != Token::Eos;
        if has_ahead {
            self.t = self.ahead;
            self.ahead = Token::Eos;
        } else {
            self.t = self.llex()?;
        }
        Ok(())
    }

    /// `luaX_lookahead`: scan (and cache) the token after the current one.
    pub fn lookahead(&mut self) -> R<Token<H::Str>> {
        debug_assert!(self.ahead == Token::Eos);
        self.ahead = self.llex()?;
        Ok(self.ahead)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testhost::TestHost;

    fn toks(src: &[u8]) -> Vec<String> {
        let mut host = TestHost::default();
        let mut ls = LexState::new(&mut host, src, b"=t").unwrap();
        let mut out = Vec::new();
        loop {
            ls.next_token().unwrap();
            let t = ls.t;
            let s = match t {
                Token::Str(h) | Token::Name(h) => {
                    let b = ls.host.bytes(h).to_vec();
                    format!(
                        "{}:{}",
                        if matches!(t, Token::Str(_)) { "S" } else { "N" },
                        String::from_utf8_lossy(&b)
                    )
                }
                Token::Int(i) => format!("I:{i}"),
                Token::Flt(f) => format!("F:{f}"),
                t => String::from_utf8_lossy(&t.to_text()).into_owned(),
            };
            if t == Token::Eos {
                return out;
            }
            out.push(s);
        }
    }

    #[test]
    fn symbols_and_words() {
        assert_eq!(
            toks(b"local x <const> = a // b .. c ... == ~= <= >= << >> :: global goto"),
            vec![
                "'local'", "N:x", "'<'", "N:const", "'>'", "'='", "N:a", "'//'", "N:b", "'..'",
                "N:c", "'...'", "'=='", "'~='", "'<='", "'>='", "'<<'", "'>>'", "'::'", "N:global",
                "'goto'"
            ]
        );
    }

    #[test]
    fn numerals() {
        assert_eq!(
            toks(b"3 3.0 0x10 0xA.8p0 1e2 .5 3-4 0xe+1 9223372036854775808"),
            vec![
                "I:3",
                "F:3",
                "I:16",
                "F:10.5",
                "F:100",
                "F:0.5",
                "I:3",
                "'-'",
                "I:4",
                "I:14",
                "'+'",
                "I:1",
                "F:9223372036854776000"
            ]
        );
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(
            toks(b"'a\\n\\65\\x41\\u{20AC}\\z   \n  b' [[\nx]] [==[a]]b]==] -- c\n--[[ m\nl ]] z"),
            vec!["S:a\nAA\u{20AC}b", "S:x", "S:a]]b", "N:z"]
        );
    }

    #[test]
    fn line_counting_and_lookahead() {
        let mut host = TestHost::default();
        let mut ls = LexState::new(&mut host, b"a\r\nb\n\rc\n\nd", b"@f.lua").unwrap();
        ls.next_token().unwrap();
        assert_eq!(ls.line_number, 1);
        let la = ls.lookahead().unwrap();
        assert!(matches!(la, Token::Name(_)));
        assert_eq!(ls.line_number, 2);
        ls.next_token().unwrap(); // consumes lookahead
        assert_eq!(ls.last_line, 2);
        assert!(matches!(ls.t, Token::Name(_)));
        ls.next_token().unwrap();
        ls.next_token().unwrap();
        assert_eq!(ls.line_number, 5);
        ls.next_token().unwrap();
        assert_eq!(ls.t, Token::Eos);
        let e = ls.syntax_error("boom");
        assert_eq!(e.message, b"f.lua:5: boom near <eof>");
    }

    #[test]
    fn high_bytes_are_char_tokens() {
        let mut host = TestHost::default();
        let mut ls = LexState::new(&mut host, b"\xC3", b"=t").unwrap();
        ls.next_token().unwrap();
        assert_eq!(ls.syntax_error("m").message, b"t:1: m near '<\\195>'");
    }
}
