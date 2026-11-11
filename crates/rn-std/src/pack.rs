//! `string.pack` / `string.unpack` / `string.packsize` engine (port of the pack section of `lstrlib.c`).
//!
//! Native sizes are those of the reference build on 64-bit Linux (`short`=2, `int`=4, `long`=8,
//! `size_t`=8, `lua_Integer`=8, `lua_Number`=8, max alignment 8 (5.4) / 16 (5.5), little-endian host).
//! Version differences: 5.4 keeps sizes in `int` (limit `INT_MAX`), 5.5 in `size_t` (limit `MAX_SIZE`);
//! see [`MAX_SIZE`]. Arguments are fetched lazily through [`PackSource`] so the VM keeps its own
//! argument-conversion errors.
#![forbid(unsafe_code)]

const MAXINTSIZE: u64 = 16;
const SZINT: usize = 8;
/// Largest size accepted in a format (5.4: `INT_MAX`, 5.5: `MAX_SIZE` = `lua_Integer` max).
pub const MAX_SIZE: u64 = if cfg!(rustynail_lua54) {
    i32::MAX as u64
} else {
    i64::MAX as u64
};
/// Padding larger than this fails with `not enough memory` (the reference would try to allocate it).
const MAX_PAD: u64 = 1 << 31;
/// `offsetof(struct cD, u)`: 5.4's `LUAI_MAXALIGN` union tops out at 8 bytes, 5.5's includes `long double` (16).
const NATIVE_MAXALIGN: u64 = if cfg!(rustynail_lua54) { 8 } else { 16 };
const NATIVE_LITTLE: bool = cfg!(target_endian = "little");

/// Errors produced by the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackError<E> {
    /// `luaL_error` message (no position).
    Msg(Vec<u8>),
    /// `luaL_argerror`/`luaL_argcheck`: argument index (1-based) and message.
    Arg(usize, &'static str),
    /// Error from the argument source (e.g. wrong argument type).
    Src(E),
}

/// Where `pack` gets its arguments (`idx` is the Lua argument number: first value is argument 2).
pub trait PackSource {
    type Error;
    fn int(&mut self, idx: usize) -> Result<i64, Self::Error>;
    fn num(&mut self, idx: usize) -> Result<f64, Self::Error>;
    fn str(&mut self, idx: usize) -> Result<Vec<u8>, Self::Error>;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum K {
    Int,
    Uint,
    Float,
    Number,
    Double,
    Char,
    Str,
    Zstr,
    Padding,
    Paddalign,
    Nop,
}

struct Header<'a> {
    fmt: &'a [u8],
    pos: usize,
    islittle: bool,
    maxalign: u64,
}

type R<T, E = std::convert::Infallible> = Result<T, PackError<E>>;

fn msg<E>(s: String) -> PackError<E> {
    PackError::Msg(s.into_bytes())
}

fn digit(c: u8) -> bool {
    c.is_ascii_digit()
}

impl<'a> Header<'a> {
    fn new(fmt: &'a [u8]) -> Self {
        Header {
            fmt,
            pos: 0,
            islittle: NATIVE_LITTLE,
            maxalign: 1,
        }
    }
    fn peek(&self) -> u8 {
        self.fmt.get(self.pos).copied().unwrap_or(0)
    }
    fn more(&self) -> bool {
        self.peek() != 0
    }

    fn getnum(&mut self, df: u64) -> u64 {
        if !digit(self.peek()) {
            return df;
        }
        let mut a: u64 = 0;
        loop {
            a = a * 10 + (self.peek() - b'0') as u64;
            self.pos += 1;
            if !(digit(self.peek()) && a <= (MAX_SIZE - 9) / 10) {
                break;
            }
        }
        a
    }

    fn getnumlimit<E>(&mut self, df: u64) -> R<u64, E> {
        let sz = self.getnum(df);
        if sz.wrapping_sub(1) >= MAXINTSIZE {
            // C passes a size_t to "%d": only the low 32 bits are printed
            return Err(msg(format!(
                "integral size ({}) out of limits [1,{}]",
                sz as u32 as i32, MAXINTSIZE
            )));
        }
        Ok(sz)
    }

    fn getoption<E>(&mut self, size: &mut u64) -> R<K, E> {
        let opt = self.peek();
        self.pos += 1;
        *size = 0;
        Ok(match opt {
            b'b' => {
                *size = 1;
                K::Int
            }
            b'B' => {
                *size = 1;
                K::Uint
            }
            b'h' => {
                *size = 2;
                K::Int
            }
            b'H' => {
                *size = 2;
                K::Uint
            }
            b'l' | b'j' => {
                *size = 8;
                K::Int
            }
            b'L' | b'J' | b'T' => {
                *size = 8;
                K::Uint
            }
            b'f' => {
                *size = 4;
                K::Float
            }
            b'n' => {
                *size = 8;
                K::Number
            }
            b'd' => {
                *size = 8;
                K::Double
            }
            b'i' => {
                *size = self.getnumlimit(4)?;
                K::Int
            }
            b'I' => {
                *size = self.getnumlimit(4)?;
                K::Uint
            }
            b's' => {
                *size = self.getnumlimit(8)?;
                K::Str
            }
            b'c' => {
                *size = self.getnum(u64::MAX);
                if *size == u64::MAX {
                    return Err(msg("missing size for format option 'c'".to_string()));
                }
                K::Char
            }
            b'z' => K::Zstr,
            b'x' => {
                *size = 1;
                K::Padding
            }
            b'X' => K::Paddalign,
            b' ' => K::Nop,
            b'<' => {
                self.islittle = true;
                K::Nop
            }
            b'>' => {
                self.islittle = false;
                K::Nop
            }
            b'=' => {
                self.islittle = NATIVE_LITTLE;
                K::Nop
            }
            b'!' => {
                self.maxalign = self.getnumlimit(NATIVE_MAXALIGN)?;
                K::Nop
            }
            c => {
                let mut m = b"invalid format option '".to_vec();
                m.push(c);
                m.push(b'\'');
                return Err(PackError::Msg(m));
            }
        })
    }

    fn getdetails<E>(&mut self, totalsize: u64, psize: &mut u64, ntoalign: &mut u64) -> R<K, E> {
        let opt = self.getoption(psize)?;
        let mut align = *psize;
        if opt == K::Paddalign
            && (!self.more() || self.getoption::<E>(&mut align)? == K::Char || align == 0)
        {
            return Err(PackError::Arg(1, "invalid next option for option 'X'"));
        }
        if align <= 1 || opt == K::Char {
            *ntoalign = 0;
        } else {
            if align > self.maxalign {
                align = self.maxalign;
            }
            if align & (align - 1) != 0 {
                *ntoalign = 0;
                return Err(PackError::Arg(
                    1,
                    "format asks for alignment not power of 2",
                ));
            }
            let szmoda = totalsize & (align - 1);
            *ntoalign = (align - szmoda) & (align - 1);
        }
        Ok(opt)
    }
}

fn packint(b: &mut Vec<u8>, n: u64, islittle: bool, size: usize, neg: bool) {
    let mut buff = vec![0u8; size];
    let mut n = n;
    let idx = |i: usize| if islittle { i } else { size - 1 - i };
    buff[idx(0)] = (n & 0xff) as u8;
    for i in 1..size {
        n >>= 8;
        buff[idx(i)] = (n & 0xff) as u8;
    }
    if neg && size > SZINT {
        for i in SZINT..size {
            buff[idx(i)] = 0xff;
        }
    }
    b.extend_from_slice(&buff);
}

fn with_endian(mut bytes: Vec<u8>, islittle: bool) -> Vec<u8> {
    // `bytes` holds the native little-endian representation
    if islittle != NATIVE_LITTLE {
        bytes.reverse();
    }
    bytes
}

/// `string.pack(fmt, ...)`.
pub fn pack<S: PackSource>(fmt: &[u8], src: &mut S) -> Result<Vec<u8>, PackError<S::Error>> {
    // The format string ends at the first NUL (C string semantics).
    let fmt = cstr(fmt);
    let mut h = Header::new(fmt);
    let mut b = Vec::new();
    let mut arg = 1usize;
    let mut totalsize: u64 = 0;
    while h.more() {
        let mut size = 0;
        let mut ntoalign = 0;
        let opt = h.getdetails::<S::Error>(totalsize, &mut size, &mut ntoalign)?;
        if size.wrapping_add(ntoalign) > MAX_SIZE - totalsize
            || size.checked_add(ntoalign).is_none()
        {
            return Err(PackError::Arg(arg, "result too long"));
        }
        totalsize += ntoalign + size;
        b.extend(std::iter::repeat_n(0u8, ntoalign as usize));
        arg += 1;
        match opt {
            K::Int => {
                let n = src.int(arg).map_err(PackError::Src)?;
                if (size as usize) < SZINT {
                    let lim = 1i64 << (size * 8 - 1);
                    if !(-lim <= n && n < lim) {
                        return Err(PackError::Arg(arg, "integer overflow"));
                    }
                }
                packint(&mut b, n as u64, h.islittle, size as usize, n < 0);
            }
            K::Uint => {
                let n = src.int(arg).map_err(PackError::Src)?;
                if (size as usize) < SZINT && (n as u64) >= (1u64 << (size * 8)) {
                    return Err(PackError::Arg(arg, "unsigned overflow"));
                }
                packint(&mut b, n as u64, h.islittle, size as usize, false);
            }
            K::Float => {
                let f = src.num(arg).map_err(PackError::Src)? as f32;
                b.extend(with_endian(f.to_le_bytes().to_vec(), h.islittle));
            }
            K::Number | K::Double => {
                let f = src.num(arg).map_err(PackError::Src)?;
                b.extend(with_endian(f.to_le_bytes().to_vec(), h.islittle));
            }
            K::Char => {
                let s = src.str(arg).map_err(PackError::Src)?;
                if s.len() as u64 > size {
                    return Err(PackError::Arg(arg, "string longer than given size"));
                }
                b.extend_from_slice(&s);
                if size > MAX_PAD {
                    return Err(msg("not enough memory".to_string()));
                }
                b.extend(std::iter::repeat_n(0u8, (size as usize) - s.len()));
            }
            K::Str => {
                let s = src.str(arg).map_err(PackError::Src)?;
                if !(size >= 8 || (s.len() as u64) < (1u64 << (size * 8))) {
                    return Err(PackError::Arg(
                        arg,
                        "string length does not fit in given size",
                    ));
                }
                packint(&mut b, s.len() as u64, h.islittle, size as usize, false);
                b.extend_from_slice(&s);
                totalsize += s.len() as u64;
            }
            K::Zstr => {
                let s = src.str(arg).map_err(PackError::Src)?;
                if s.contains(&0) {
                    return Err(PackError::Arg(arg, "string contains zeros"));
                }
                b.extend_from_slice(&s);
                b.push(0);
                totalsize += s.len() as u64 + 1;
            }
            K::Padding => {
                b.push(0);
                arg -= 1;
            }
            K::Paddalign | K::Nop => arg -= 1,
        }
    }
    Ok(b)
}

fn cstr(fmt: &[u8]) -> &[u8] {
    match fmt.iter().position(|&c| c == 0) {
        Some(i) => &fmt[..i],
        None => fmt,
    }
}

/// `string.packsize(fmt)`.
pub fn packsize(fmt: &[u8]) -> R<u64> {
    let mut h = Header::new(cstr(fmt));
    let mut totalsize: u64 = 0;
    while h.more() {
        let mut size = 0;
        let mut ntoalign = 0;
        let opt = h.getdetails(totalsize, &mut size, &mut ntoalign)?;
        if opt == K::Str || opt == K::Zstr {
            return Err(PackError::Arg(1, "variable-length format"));
        }
        size = size.wrapping_add(ntoalign);
        if size > MAX_SIZE || totalsize > MAX_SIZE - size {
            return Err(PackError::Arg(1, "format result too large"));
        }
        totalsize += size;
    }
    Ok(totalsize)
}

/// A value returned by `string.unpack`.
#[derive(Clone, Debug, PartialEq)]
pub enum Unpacked {
    Int(i64),
    Num(f64),
    Str(Vec<u8>),
}

fn unpackint(data: &[u8], islittle: bool, size: usize, signed: bool) -> R<i64> {
    let idx = |i: usize| if islittle { i } else { size - 1 - i };
    let limit = size.min(SZINT);
    let mut res: u64 = 0;
    for i in (0..limit).rev() {
        res <<= 8;
        res |= data[idx(i)] as u64;
    }
    if size < SZINT {
        if signed {
            let mask = 1u64 << (size * 8 - 1);
            res = (res ^ mask).wrapping_sub(mask);
        }
    } else if size > SZINT {
        let mask = if !signed || (res as i64) >= 0 {
            0
        } else {
            0xff
        };
        for i in limit..size {
            if data[idx(i)] != mask {
                return Err(msg(format!(
                    "{size}-byte integer does not fit into Lua Integer"
                )));
            }
        }
    }
    Ok(res as i64)
}

/// `string.unpack(fmt, data [, init])`; returns the values and the next position.
/// `init` is the raw Lua argument (default 1).
pub fn unpack(fmt: &[u8], data: &[u8], init: i64) -> R<(Vec<Unpacked>, i64)> {
    let ld = data.len() as u64;
    let mut pos = (crate::pattern::posrelat_i(init, data.len()) - 1) as u64;
    if pos > ld {
        return Err(PackError::Arg(3, "initial position out of string"));
    }
    let mut h = Header::new(cstr(fmt));
    let mut out = Vec::new();
    while h.more() {
        let mut size = 0;
        let mut ntoalign = 0;
        let opt = h.getdetails(pos, &mut size, &mut ntoalign)?;
        if size.checked_add(ntoalign).is_none_or(|t| t > ld - pos) {
            return Err(PackError::Arg(2, "data string too short"));
        }
        pos += ntoalign;
        let p = pos as usize;
        let sz = size as usize;
        match opt {
            K::Int | K::Uint => {
                out.push(Unpacked::Int(unpackint(
                    &data[p..],
                    h.islittle,
                    sz,
                    opt == K::Int,
                )?));
            }
            K::Float => {
                let mut a = [0u8; 4];
                a.copy_from_slice(&data[p..p + 4]);
                let v = with_endian(a.to_vec(), h.islittle);
                a.copy_from_slice(&v);
                out.push(Unpacked::Num(f32::from_le_bytes(a) as f64));
            }
            K::Number | K::Double => {
                let mut a = [0u8; 8];
                a.copy_from_slice(&data[p..p + 8]);
                let v = with_endian(a.to_vec(), h.islittle);
                a.copy_from_slice(&v);
                out.push(Unpacked::Num(f64::from_le_bytes(a)));
            }
            K::Char => out.push(Unpacked::Str(data[p..p + sz].to_vec())),
            K::Str => {
                let len = unpackint(&data[p..], h.islittle, sz, false)? as u64;
                if len > ld - pos - size {
                    return Err(PackError::Arg(2, "data string too short"));
                }
                out.push(Unpacked::Str(data[p + sz..p + sz + len as usize].to_vec()));
                pos += len;
            }
            K::Zstr => {
                let len = data[p..]
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(data.len() - p);
                if pos + len as u64 >= ld {
                    return Err(PackError::Arg(2, "unfinished string for format 'z'"));
                }
                out.push(Unpacked::Str(data[p..p + len].to_vec()));
                pos += len as u64 + 1;
            }
            K::Paddalign | K::Padding | K::Nop => {}
        }
        pos += size;
    }
    Ok((out, pos as i64 + 1))
}
