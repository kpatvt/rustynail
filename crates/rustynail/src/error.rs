//! The typed error of the embedding API.
use std::fmt;

/// What kind of failure an [`Error`] reports.
///
/// The first four variants correspond to the C API status codes (`LUA_ERRRUN`, `LUA_ERRSYNTAX`,
/// `LUA_ERRMEM`, `LUA_ERRERR`); the others are conditions of this API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A runtime error (`error(...)`, a failed operation, an error raised by a Rust function).
    Runtime,
    /// A compilation error from `load` (also: a text chunk loaded in binary-only mode and vice
    /// versa).
    Syntax,
    /// An allocation failed, or the memory limit ([`Lua::set_memory_limit`]) was reached.
    ///
    /// [`Lua::set_memory_limit`]: crate::Lua::set_memory_limit
    Memory,
    /// The message handler of a [`pcall`](trait.Stack.html#method.pcall) itself raised an error.
    MessageHandler,
    /// A file could not be opened or read ([`load_file`](trait.Stack.html#method.load_file)).
    File,
    /// The instruction budget ([`Lua::set_instruction_limit`]) ran out.
    ///
    /// [`Lua::set_instruction_limit`]: crate::Lua::set_instruction_limit
    InstructionLimit,
    /// The API was used incorrectly: an invalid stack index, a value of the wrong type where the
    /// operation needs a specific one (e.g. `raw_get` on a non-table), not enough values on the
    /// stack, a stack that cannot grow any more, a userdata of another Rust type.
    Api,
    /// Not an error: the request to yield made by [`Stack::yield_`](trait.Stack.html#method.yield_). A Rust
    /// function must return it unchanged.
    Yield,
}

/// The error type of every fallible operation.
///
/// It carries the [`ErrorKind`] and a copy of the error message (the Lua error object converted
/// with the rules of the standalone interpreter: strings and numbers as themselves, other values
/// as `(error object is a <type> value)`). Messages are byte strings, as Lua strings are; the
/// [`Display`](fmt::Display) implementation converts them lossily to UTF-8.
///
/// When a Lua error is caught by this API, the original error *object* is also kept inside the
/// state: returning the [`Error`] from a Rust function (for example with `?`) raises that same
/// object again, so tables or userdata used as error values survive a round trip through Rust.
#[derive(Clone)]
pub struct Error {
    kind: ErrorKind,
    message: Vec<u8>,
    /// Identifies the error object stored in the state (0: none, raise `message` instead).
    pub(crate) seq: u64,
    /// For [`ErrorKind::Yield`]: number of values to yield.
    pub(crate) nyield: usize,
}

/// `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// A new error of the given kind. When returned from a Rust function called by Lua, it is
    /// raised as a Lua error whose object is the message string (as `luaL_error` does; a
    /// [`ErrorKind::Memory`] error is raised as a memory error).
    pub fn new(kind: ErrorKind, message: impl Into<Vec<u8>>) -> Error {
        Error {
            kind,
            message: message.into(),
            seq: 0,
            nyield: 0,
        }
    }

    /// A runtime error with the given message (the usual way for a Rust function to fail).
    pub fn runtime(message: impl Into<Vec<u8>>) -> Error {
        Error::new(ErrorKind::Runtime, message)
    }

    pub(crate) fn api(message: impl Into<Vec<u8>>) -> Error {
        Error::new(ErrorKind::Api, message)
    }

    pub(crate) fn yield_marker(n: usize) -> Error {
        Error {
            kind: ErrorKind::Yield,
            message: b"yield".to_vec(),
            seq: 0,
            nyield: n,
        }
    }

    /// The kind of failure.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// The error message as raw bytes.
    pub fn message(&self) -> &[u8] {
        &self.message
    }

    /// The error message, converted lossily to UTF-8.
    pub fn message_lossy(&self) -> String {
        String::from_utf8_lossy(&self.message).into_owned()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.message))
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("kind", &self.kind)
            .field("message", &String::from_utf8_lossy(&self.message))
            .finish()
    }
}

impl std::error::Error for Error {}
