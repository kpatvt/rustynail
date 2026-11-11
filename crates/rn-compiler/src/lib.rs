//! rn-compiler — lexer, parser, code generator, dump/undump. See docs/ARCH.md.
#![forbid(unsafe_code)]
use rn_core::host::CompilerHost;
use rn_core::protodata::ProtoData;

pub mod dump;
pub(crate) mod khash;
pub mod lexer;
pub mod listing;
pub mod testhost;
pub mod undump;

#[cfg(rustynail_lua54)]
pub mod v54;
#[cfg(not(rustynail_lua54))]
pub mod v55;

pub use dump::dump;
pub use undump::undump;

/// Compile error. `message` is formatted exactly like the reference (`chunk:line: msg near 'tok'`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub message: Vec<u8>,
}

/// Load mode restriction (`"t"`, `"b"`, `"bt"`).
///
/// This only records *which* kinds are allowed. The reference prints the mode string exactly as the
/// caller wrote it (`mode is 'tb'`, `mode is 'x'`); callers that have the raw string should use
/// [`load_with_mode_str`], which reproduces those messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadMode {
    pub text: bool,
    pub binary: bool,
}

impl LoadMode {
    /// Both kinds allowed (`mode == NULL` / `"bt"`).
    pub const BT: LoadMode = LoadMode {
        text: true,
        binary: true,
    };

    /// Interpret a mode string like `luaD_protectedparser`/`checkmode` do (`strchr(mode, 'b')`, `strchr(mode, 't')`).
    pub fn from_mode_str(mode: &[u8]) -> LoadMode {
        LoadMode {
            text: mode.contains(&b't'),
            binary: mode.contains(&b'b'),
        }
    }
}

/// `checkmode` (ldo.c): error text for loading a `kind` (`"binary"`/`"text"`) chunk with mode string `mode`.
pub fn mode_error(kind: &str, mode: &[u8]) -> CompileError {
    // "attempt to load a %s chunk (mode is '%s')"; `%s` of the mode stops at the first NUL
    let mode = match mode.iter().position(|&c| c == 0) {
        Some(i) => &mode[..i],
        None => mode,
    };
    let mut m = b"attempt to load a ".to_vec();
    m.extend_from_slice(kind.as_bytes());
    m.extend_from_slice(b" chunk (mode is '");
    m.extend_from_slice(mode);
    m.extend_from_slice(b"')");
    CompileError { message: m }
}

/// Like [`load`] but takes the raw mode string (`None` = no restriction), so that mode errors print
/// the string exactly as the caller wrote it, as the reference does.
pub fn load_with_mode_str<H: CompilerHost>(
    host: &mut H,
    chunk: &[u8],
    chunk_name: &[u8],
    mode: Option<&[u8]>,
) -> Result<ProtoData<H::Str>, CompileError> {
    let binary = chunk.first() == Some(&undump::LUA_SIGNATURE[0]);
    let (flag, kind) = if binary {
        (b'b', "binary")
    } else {
        (b't', "text")
    };
    if let Some(mode) = mode {
        // `strchr(mode, x[0])` also matches the terminating NUL only for x[0] == 0 (never here)
        let cmode = match mode.iter().position(|&c| c == 0) {
            Some(i) => &mode[..i],
            None => mode,
        };
        // 5.5: a 'B' in the mode (fixed buffer; C API only) lets a binary chunk skip the check
        let fixed = !cfg!(rustynail_lua54) && binary && cmode.contains(&b'B');
        if !fixed && !cmode.contains(&flag) {
            return Err(mode_error(kind, mode));
        }
    }
    if binary {
        undump(host, chunk, chunk_name)
    } else {
        compile_text(host, chunk, chunk_name)
    }
}

/// Compile a text chunk (or undump a binary one) into a prototype.
///
/// A chunk starting with ESC (`LUA_SIGNATURE[0]`) is binary; otherwise it is text.
/// Mode violations produce the reference `attempt to load a {text,binary} chunk (mode is '..')`
/// errors; with only a [`LoadMode`] the mode string is reconstructed as `b`/`t`/`bt`
/// (use [`load_with_mode_str`] to print the caller's original string).
pub fn load<H: CompilerHost>(
    host: &mut H,
    chunk: &[u8],
    chunk_name: &[u8],
    mode: LoadMode,
) -> Result<ProtoData<H::Str>, CompileError> {
    let mut s = Vec::new();
    if mode.binary {
        s.push(b'b');
    }
    if mode.text {
        s.push(b't');
    }
    load_with_mode_str(host, chunk, chunk_name, Some(&s))
}

#[cfg(not(rustynail_lua54))]
fn compile_text<H: CompilerHost>(
    host: &mut H,
    chunk: &[u8],
    chunk_name: &[u8],
) -> Result<ProtoData<H::Str>, CompileError> {
    v55::compile(host, chunk, chunk_name, 1)
}

#[cfg(rustynail_lua54)]
fn compile_text<H: CompilerHost>(
    host: &mut H,
    chunk: &[u8],
    chunk_name: &[u8],
) -> Result<ProtoData<H::Str>, CompileError> {
    v54::compile(host, chunk, chunk_name, 1)
}

/// What `luaL_loadfilex` does before handing a file to the loader: skip an UTF-8 BOM and, if the
/// first line starts with `#`, skip that line (keeping its newline so line numbers stay right).
pub fn skip_file_prefix(src: &[u8]) -> &[u8] {
    let src = src.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(src);
    if src.first() == Some(&b'#') {
        let nl = src.iter().position(|&c| c == b'\n').unwrap_or(src.len());
        &src[nl..]
    } else {
        src
    }
}
