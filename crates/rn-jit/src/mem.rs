//! Executable memory manager: W^X arenas, runtime probe, per-state code cap, perf map.
//!
//! # Design
//!
//! Memory is never mapped writable and executable at the same time. A [`CodeArena`] reserves address
//! space in *chunks* (anonymous private `mmap`, 1 MiB by default). Inside a chunk, pages are in one of two
//! states, split at a page-aligned watermark `sealed_end`:
//!
//! ```text
//!  0                 sealed_end         bump                 len
//!  | RX (committed)  | RW, written ...  | RW, unused         |
//! ```
//!
//! * [`CodeArena::alloc`] bump-allocates above `sealed_end` (padding and new blocks are filled with `int3`).
//! * [`CodeArena::writer`] gives write access to a block only while it lies in the RW region.
//! * [`CodeArena::commit`] flips every page written since the last commit from RW to RX with one `mprotect`
//!   per chunk (batching: a compile unit does several allocs and one commit), fills the rest of the last
//!   page with `int3`, and advances `sealed_end` to the next page boundary. A page is therefore never both
//!   writable and executable, and never flips back while any block in its chunk is live.
//! * A chunk whose blocks have all been freed is parked in a small pool (still RX: the flip back to RW is
//!   deferred until the chunk is reused, so a free costs no syscall), or unmapped. Pooled chunks beyond
//!   1 MiB of total size are additionally trimmed with `madvise(MADV_DONTNEED)`.
//!
//! # Limits
//!
//! The *cap* ([`CodeArena::cap`]) bounds **mapped** address space ([`CodeArena::mapped`]): every chunk is
//! charged its full length when mapped and refunded when unmapped; pooled and quarantined chunks stay
//! charged. [`CodeArena::used`] (sum of live block sizes) is only a statistic. [`CodeArena::new`] uses
//! [`DEFAULT_CAP`] (256 MiB); [`CodeArena::with_cap`] may raise it. A single block is limited to
//! [`MAX_BLOCK_SIZE`] (64 MiB, larger requests are `BadRequest`). When the cap is tight a chunk smaller
//! than the standard size (down to the page-rounded request) is mapped instead of failing.
//!
//! # Thread-safety contract
//!
//! `CodeArena` is `Send` and `!Sync`: its *bookkeeping* (and every `mprotect`) happens through `&mut self`
//! on one thread at a time. That does **not** stop another thread from *executing* code whose entry
//! pointer it copied out (function pointers are plain `Copy` values). The soundness guarantee is therefore
//! a caller contract, spelled out on [`CodeArena::entry`], [`CodeArena::free`] and the `Drop` impl: no
//! thread may be executing in, or later call, a block that is freed or whose arena is dropped.
//!
//! # Drop
//!
//! Dropping a [`CodeArena`] unmaps every chunk, **including live code**. Any entry pointer obtained from it
//! dangles afterwards (calling one is a use-after-free of code memory). Free or stop running the code first.
//!
//! # Quarantine (debugging)
//!
//! [`CodeArena::set_quarantine`] (also enabled by `RUSTYNAIL_JIT=verify`) makes a fully freed chunk
//! `PROT_NONE` and never reuses or unmaps it until the arena drops, so calling a stale entry pointer
//! faults deterministically (`SIGSEGV`) instead of running recycled code. Blocks sharing a chunk with a
//! live block cannot be protected individually and stay executable until the whole chunk is freed.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

const INT3: u8 = 0xCC;
/// Default chunk size (1 MiB).
pub const DEFAULT_CHUNK_SIZE: usize = 1 << 20;
/// Largest accepted chunk size (1 GiB); [`CodeArena::with_config`] clamps to `[page size, this]`.
pub const MAX_CHUNK_SIZE: usize = 1 << 30;
/// Default cap on mapped bytes of [`CodeArena::new`] (256 MiB).
pub const DEFAULT_CAP: usize = 256 << 20;
/// Largest single block ([`CodeArena::alloc`] returns `BadRequest` above this): 64 MiB.
pub const MAX_BLOCK_SIZE: usize = 64 << 20;
/// Maximum number of empty standard-size chunks kept for reuse.
const MAX_POOL: usize = 2;
/// Pooled chunks beyond this many bytes in total are returned to the OS (`MADV_DONTNEED`).
const POOL_RESIDENT: usize = 1 << 20;

/// The OS page size (cached).
pub fn page_size() -> usize {
    static PAGE: OnceLock<usize> = OnceLock::new();
    *PAGE.get_or_init(|| {
        // SAFETY: sysconf with a valid name has no memory-safety preconditions.
        let p = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if p > 0 {
            p as usize
        } else {
            4096
        }
    })
}

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Rounds `v` up to a multiple of the power of two `a`; `None` on overflow.
fn align_up(v: usize, a: usize) -> Option<usize> {
    debug_assert!(a.is_power_of_two());
    v.checked_add(a - 1).map(|x| x & !(a - 1))
}

// ---------------------------------------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------------------------------------

/// Why executable memory is not available in this process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// Anonymous `mmap` failed (errno).
    Map(i32),
    /// `mprotect(PROT_READ | PROT_EXEC)` on a page that had been writable failed (errno). Typical for
    /// `MemoryDenyWriteExecute`, `PR_SET_MDWE` and SELinux `execmem` denials (`EACCES`/`EPERM`).
    Protect(i32),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeError::Map(e) => write!(
                f,
                "cannot map memory for JIT: {}",
                std::io::Error::from_raw_os_error(*e)
            ),
            ProbeError::Protect(e) => write!(
                f,
                "host forbids RW->RX transitions (W^X policy?): {}",
                std::io::Error::from_raw_os_error(*e)
            ),
        }
    }
}

impl std::error::Error for ProbeError {}

/// Checks (once per process, result cached) that "map RW, write code, `mprotect` RX, execute" works.
///
/// Maps one page `PROT_READ|PROT_WRITE`, writes a `ret`, flips it to `PROT_READ|PROT_EXEC`, calls it and unmaps
/// it. Never maps RWX. Returns `Err` on hosts that deny this (MDWE, SELinux `execmem`, ...).
pub fn probe_exec() -> Result<(), ProbeError> {
    static RESULT: OnceLock<Result<(), ProbeError>> = OnceLock::new();
    RESULT.get_or_init(probe_uncached).clone()
}

fn probe_uncached() -> Result<(), ProbeError> {
    let len = page_size();
    // SAFETY: anonymous private mapping, no address hint; the result is checked before use.
    let p = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if p == libc::MAP_FAILED {
        return Err(ProbeError::Map(errno()));
    }
    // SAFETY: `p` is a fresh writable mapping of `len >= 1` bytes.
    unsafe { (p as *mut u8).write(0xC3) };
    // SAFETY: `p`/`len` describe exactly the mapping created above.
    let rc = unsafe { libc::mprotect(p, len, libc::PROT_READ | libc::PROT_EXEC) };
    if rc != 0 {
        let e = errno();
        // SAFETY: unmapping the mapping created above; nothing references it.
        unsafe { libc::munmap(p, len) };
        return Err(ProbeError::Protect(e));
    }
    // SAFETY: the page now holds a single `ret` instruction and is executable; `extern "C" fn()` matches it.
    unsafe {
        let f: extern "C" fn() = std::mem::transmute::<*mut libc::c_void, extern "C" fn()>(p);
        f();
        libc::munmap(p, len);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------
// perf map
// ---------------------------------------------------------------------------------------------------------

/// Writer for `/tmp/perf-<pid>.map` (the format `perf` uses to symbolise JIT code).
///
/// Hardening: the file is opened with `O_NOFOLLOW` (a symlink planted in `/tmp` is refused), mode `0600`,
/// and must be a regular file owned by the current effective uid, otherwise opening fails (and
/// [`PerfMap::from_env`] silently disables the map). A map created with [`PerfMap::open_current`] /
/// [`PerfMap::from_env`] follows `fork`: when `getpid()` no longer equals the pid the file was opened for,
/// the next [`PerfMap::write_symbol`] reopens `/tmp/perf-<new pid>.map`. Entries are never unregistered
/// (freed code keeps its name in the map; `perf` only needs addresses that were valid when sampled).
#[derive(Debug)]
pub struct PerfMap {
    file: File,
    pid: u32,
    follow_pid: bool,
}

fn open_perf_file(pid: u32) -> std::io::Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(format!("/tmp/perf-{pid}.map"))?;
    let md = file.metadata()?;
    // SAFETY: geteuid has no preconditions.
    let euid = unsafe { libc::geteuid() };
    if !md.is_file() || md.uid() != euid {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "perf map is not a regular file owned by the current user",
        ));
    }
    Ok(file)
}

impl PerfMap {
    /// Opens (creating, appending) `/tmp/perf-<pid>.map` for the given pid (fixed: no fork tracking).
    pub fn open(pid: u32) -> std::io::Result<PerfMap> {
        Ok(PerfMap {
            file: open_perf_file(pid)?,
            pid,
            follow_pid: false,
        })
    }

    /// Opens the map of the current process; after a `fork` the child writes to its own pid's file.
    pub fn open_current() -> std::io::Result<PerfMap> {
        let pid = std::process::id();
        Ok(PerfMap {
            file: open_perf_file(pid)?,
            pid,
            follow_pid: true,
        })
    }

    /// Opens the map for this process iff the environment variable `RUSTYNAIL_JIT_PERFMAP` is `1`.
    /// Any failure (symlink, foreign owner, I/O error) yields `None`.
    pub fn from_env() -> Option<PerfMap> {
        if std::env::var("RUSTYNAIL_JIT_PERFMAP").as_deref() == Ok("1") {
            PerfMap::open_current().ok()
        } else {
            None
        }
    }

    /// Appends `"<start hex> <size hex> <name>\n"` (one unbuffered write, so it is flushed). Newlines in
    /// `name` are replaced by spaces.
    pub fn write_symbol(&mut self, start: usize, size: usize, name: &str) -> std::io::Result<()> {
        if self.follow_pid {
            let now = std::process::id();
            if now != self.pid {
                self.file = open_perf_file(now)?;
                self.pid = now;
            }
        }
        let name = name.replace(['\n', '\r'], " ");
        let line = format!("{start:x} {size:x} {name}\n");
        self.file.write_all(line.as_bytes())
    }
}

// ---------------------------------------------------------------------------------------------------------
// Arena
// ---------------------------------------------------------------------------------------------------------

/// Errors from [`CodeArena`] operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArenaError {
    /// Mapping the memory would push [`CodeArena::mapped`] above [`CodeArena::cap`].
    CapExceeded,
    /// `mmap`/`mprotect` failed (errno).
    Os(i32),
    /// Zero size, size above [`MAX_BLOCK_SIZE`], alignment not a power of two in `1..=page size`, or a
    /// write outside a block.
    BadRequest,
    /// The block belongs to a page range that was already committed (RX, not writable).
    NotWritable,
    /// The block's pages have not been committed yet, so it must not be executed.
    NotCommitted,
    /// The block is not live in this arena: double free, or a block of another arena.
    InvalidBlock,
}

impl fmt::Display for ArenaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArenaError::CapExceeded => f.write_str("code cap exceeded"),
            ArenaError::Os(e) => write!(f, "OS error: {}", std::io::Error::from_raw_os_error(*e)),
            ArenaError::BadRequest => f.write_str(
                "bad request (size 0 or too large, bad alignment, or write out of range)",
            ),
            ArenaError::NotWritable => {
                f.write_str("block is already committed (read-only/executable)")
            }
            ArenaError::NotCommitted => f.write_str("block is not committed yet"),
            ArenaError::InvalidBlock => {
                f.write_str("invalid code block (double free or foreign arena)")
            }
        }
    }
}

impl std::error::Error for ArenaError {}

static NEXT_ARENA_ID: AtomicU64 = AtomicU64::new(1);

struct Chunk {
    base: *mut u8,
    len: usize,
    /// Page-aligned; `[0, sealed_end)` is RX, `[sealed_end, len)` is RW.
    sealed_end: usize,
    /// Next free offset (`>= sealed_end`).
    bump: usize,
    /// End of the last block allocated before the most recent commit (the padding after it up to
    /// `sealed_end` is `int3`): the part of the last sealed page that [`CodeArena::reopen_tail`] can reuse.
    used_end: usize,
    /// Offsets of live blocks.
    live: BTreeSet<usize>,
    gen: u64,
}

impl Chunk {
    fn unmap(self) {
        // SAFETY: `base`/`len` are exactly the mapping created in `map_chunk`, owned by this Chunk; blocks
        // into it are dead (freed, or the arena is being dropped).
        unsafe { libc::munmap(self.base as *mut libc::c_void, self.len) };
    }
}

/// A move-only handle to an allocated code region. It carries no ownership of the memory: give it back with
/// [`CodeArena::free`]. Dropping it without freeing leaks the space until the arena is dropped.
#[derive(Debug)]
pub struct CodeBlock {
    arena: u64,
    chunk: usize,
    gen: u64,
    offset: usize,
    size: usize,
    ptr: *mut u8,
}

impl CodeBlock {
    /// Address of the first byte (meaningful while the block is live).
    pub fn addr(&self) -> usize {
        self.ptr as usize
    }
    /// Size in bytes as requested.
    pub fn size(&self) -> usize {
        self.size
    }
}

/// Write access to a block's bytes, pre-filled with `int3`.
#[derive(Debug)]
pub struct CodeWriter<'a> {
    bytes: &'a mut [u8],
}

impl Deref for CodeWriter<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.bytes
    }
}
impl DerefMut for CodeWriter<'_> {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.bytes
    }
}
impl CodeWriter<'_> {
    /// Copies `code` to offset `at` in the block, or `BadRequest` if `at + code.len()` exceeds the block
    /// (overflow-safe).
    pub fn try_write_at(&mut self, at: usize, code: &[u8]) -> Result<(), ArenaError> {
        let end = at.checked_add(code.len()).ok_or(ArenaError::BadRequest)?;
        self.bytes
            .get_mut(at..end)
            .ok_or(ArenaError::BadRequest)?
            .copy_from_slice(code);
        Ok(())
    }

    /// Copies `code` to offset `at` in the block.
    ///
    /// # Panics
    /// If the range `at..at + code.len()` does not lie inside the block (use [`CodeWriter::try_write_at`]
    /// to get an error instead).
    pub fn write_at(&mut self, at: usize, code: &[u8]) {
        let size = self.bytes.len();
        if self.try_write_at(at, code).is_err() {
            panic!(
                "CodeWriter::write_at out of range: offset {at} + length {} exceeds block size {size}",
                code.len()
            );
        }
    }
}

/// Compile-time check that a type is pointer sized (used by [`CodeArena::entry`]).
struct AssertPtrSized<F>(PhantomData<F>);
impl<F> AssertPtrSized<F> {
    const OK: () = assert!(
        std::mem::size_of::<F>() == std::mem::size_of::<usize>(),
        "CodeArena::entry: F must be a pointer-sized function pointer type"
    );
}

/// Per-state executable memory arena (see the module docs for the W^X state machine, the limits and the
/// thread-safety contract). `Send`, `!Sync`.
pub struct CodeArena {
    id: u64,
    chunk_size: usize,
    cap: usize,
    used: usize,
    mapped: usize,
    quarantine: bool,
    slots: Vec<Option<Chunk>>,
    free_slots: Vec<usize>,
    pool: Vec<Chunk>,
    quarantined: Vec<Chunk>,
    current: Option<usize>,
    pending: Vec<usize>,
    next_gen: u64,
    mprotects: u64,
    perf: Option<PerfMap>,
    _not_sync: PhantomData<Cell<()>>,
}

// SAFETY: the arena exclusively owns its mappings (raw pointers are never shared outside it except through
// borrow-checked `CodeWriter`s and caller-contract `entry` pointers), so moving it to another thread is fine.
unsafe impl Send for CodeArena {}

impl fmt::Debug for CodeArena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodeArena")
            .field("used", &self.used)
            .field("mapped", &self.mapped)
            .field("cap", &self.cap)
            .field("chunks", &self.live_chunks())
            .finish()
    }
}

impl Default for CodeArena {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeArena {
    /// Arena with 1 MiB chunks and a [`DEFAULT_CAP`] (256 MiB) cap on mapped bytes. Maps nothing until the
    /// first allocation; call [`probe_exec`] first. Opens the perf map if `RUSTYNAIL_JIT_PERFMAP=1` and
    /// enables quarantine if `RUSTYNAIL_JIT=verify`.
    pub fn new() -> CodeArena {
        Self::with_config(DEFAULT_CHUNK_SIZE, DEFAULT_CAP)
    }

    /// Arena with the given cap on [`mapped`](Self::mapped) bytes (may exceed [`DEFAULT_CAP`]; the cap is
    /// the only memory bound, `usize::MAX` means unlimited).
    pub fn with_cap(cap: usize) -> CodeArena {
        Self::with_config(DEFAULT_CHUNK_SIZE, cap)
    }

    /// Arena with a chunk size and a cap. The chunk size is clamped to `[page size, MAX_CHUNK_SIZE]` and
    /// rounded up to the page size ([`chunk_size`](Self::chunk_size) reports the value in effect).
    pub fn with_config(chunk_size: usize, cap: usize) -> CodeArena {
        let chunk_size = chunk_size.clamp(page_size(), MAX_CHUNK_SIZE);
        let chunk_size = align_up(chunk_size, page_size()).unwrap_or(MAX_CHUNK_SIZE);
        CodeArena {
            id: NEXT_ARENA_ID.fetch_add(1, Ordering::Relaxed),
            chunk_size,
            cap,
            used: 0,
            mapped: 0,
            quarantine: std::env::var("RUSTYNAIL_JIT").as_deref() == Ok("verify"),
            slots: Vec::new(),
            free_slots: Vec::new(),
            pool: Vec::new(),
            quarantined: Vec::new(),
            current: None,
            pending: Vec::new(),
            next_gen: 1,
            mprotects: 0,
            perf: PerfMap::from_env(),
            _not_sync: PhantomData,
        }
    }

    /// Bytes currently allocated (sum of live block sizes); a statistic, not what the cap bounds.
    pub fn used(&self) -> usize {
        self.used
    }
    /// Bytes of address space currently mapped by this arena (live, pooled and quarantined chunks).
    pub fn mapped(&self) -> usize {
        self.mapped
    }
    /// The cap on [`mapped`](Self::mapped) bytes.
    pub fn cap(&self) -> usize {
        self.cap
    }
    /// Changes the cap on [`mapped`](Self::mapped) bytes. Existing mappings are kept (a cap below `mapped`
    /// only makes new chunks fail until enough code was freed), so this is always sound.
    pub fn set_cap(&mut self, cap: usize) {
        self.cap = cap;
    }
    /// The standard chunk size in effect (after clamping).
    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }
    /// Number of `mprotect` calls this arena has issued (for tests of batching).
    pub fn mprotect_calls(&self) -> u64 {
        self.mprotects
    }
    /// Number of chunks currently mapped, including pooled empty ones (and quarantined ones).
    pub fn live_chunks(&self) -> usize {
        self.slots.iter().flatten().count() + self.pool.len() + self.quarantined.len()
    }

    /// Enables or disables quarantine mode for chunks freed from now on (see the module docs).
    pub fn set_quarantine(&mut self, on: bool) {
        self.quarantine = on;
    }
    /// Whether quarantine mode is on.
    pub fn quarantine_enabled(&self) -> bool {
        self.quarantine
    }

    fn map_chunk(&mut self, len: usize) -> Result<Chunk, ArenaError> {
        // SAFETY: anonymous private mapping, no hint; the result is checked before use.
        let p = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if p == libc::MAP_FAILED {
            return Err(ArenaError::Os(errno()));
        }
        self.mapped += len;
        let gen = self.next_gen;
        self.next_gen += 1;
        Ok(Chunk {
            base: p as *mut u8,
            len,
            sealed_end: 0,
            bump: 0,
            used_end: 0,
            live: BTreeSet::new(),
            gen,
        })
    }

    fn unmap_chunk(&mut self, c: Chunk) {
        self.mapped -= c.len;
        c.unmap();
    }

    fn install(&mut self, c: Chunk) -> usize {
        if let Some(i) = self.free_slots.pop() {
            self.slots[i] = Some(c);
            i
        } else {
            self.slots.push(Some(c));
            self.slots.len() - 1
        }
    }

    fn chunk(&self, id: usize) -> &Chunk {
        self.slots[id].as_ref().expect("code chunk slot is live")
    }

    /// Unmaps pooled chunks until `mapped + len` fits under the cap (or the pool is empty).
    fn reclaim_pool_for(&mut self, len: usize) {
        while self.mapped.saturating_add(len) > self.cap {
            match self.pool.pop() {
                Some(c) => self.unmap_chunk(c),
                None => break,
            }
        }
    }

    /// Makes a pooled chunk writable again (the deferred RX -> RW flip) and empties it.
    fn revive(&mut self, c: &mut Chunk) -> Result<(), ArenaError> {
        if c.sealed_end > 0 {
            self.protect(c.base, c.sealed_end, libc::PROT_READ | libc::PROT_WRITE)?;
        }
        c.sealed_end = 0;
        c.bump = 0;
        c.used_end = 0;
        Ok(())
    }

    /// Finds or maps a fresh chunk able to hold `size` bytes and makes it current.
    fn new_current(&mut self, size: usize) -> Result<usize, ArenaError> {
        let need = align_up(size, page_size()).ok_or(ArenaError::BadRequest)?;
        let mut chunk = None;
        if need <= self.chunk_size {
            if let Some(mut c) = self.pool.pop() {
                if let Err(e) = self.revive(&mut c) {
                    self.pool.push(c);
                    return Err(e);
                }
                chunk = Some(c);
            }
        }
        let chunk = match chunk {
            Some(c) => c,
            None => {
                let mut len = need.max(self.chunk_size);
                self.reclaim_pool_for(len);
                if self.mapped.saturating_add(len) > self.cap {
                    // Cap is tight: map just what the request needs.
                    len = need;
                    self.reclaim_pool_for(len);
                }
                if self.mapped.saturating_add(len) > self.cap {
                    return Err(ArenaError::CapExceeded);
                }
                self.map_chunk(len)?
            }
        };
        let id = self.install(chunk);
        self.current = Some(id);
        Ok(id)
    }

    /// Allocates `size` bytes (`1..=MAX_BLOCK_SIZE`) aligned to `align` (a power of two up to the page
    /// size, normally 16 or 32), filled with `int3`. The block is writable until the next
    /// [`commit`](Self::commit). Fails with `CapExceeded` if a new chunk would push
    /// [`mapped`](Self::mapped) over the cap.
    pub fn alloc(&mut self, size: usize, align: usize) -> Result<CodeBlock, ArenaError> {
        if size == 0 || size > MAX_BLOCK_SIZE || !align.is_power_of_two() || align > page_size() {
            return Err(ArenaError::BadRequest);
        }
        let fits = |c: &Chunk| {
            align_up(c.bump, align)
                .and_then(|o| o.checked_add(size))
                .is_some_and(|e| e <= c.len)
        };
        let id = match self.current {
            Some(id) if fits(self.chunk(id)) => id,
            _ => self.new_current(size)?,
        };
        let c = self.slots[id].as_mut().expect("code chunk slot is live");
        let off = align_up(c.bump, align).expect("checked by `fits` / fresh chunk");
        // SAFETY: `[bump, off+size)` lies inside the chunk (checked by `fits`, or a fresh chunk with
        // size <= len) and above `sealed_end`, so it is RW; no CodeWriter can exist (they borrow the arena
        // shared and we hold `&mut self`).
        unsafe { std::ptr::write_bytes(c.base.add(c.bump), INT3, off + size - c.bump) };
        c.bump = off + size;
        c.live.insert(off);
        let (gen, ptr) = (
            c.gen,
            // SAFETY: `off` is within the mapping.
            unsafe { c.base.add(off) },
        );
        if !self.pending.contains(&id) {
            self.pending.push(id);
        }
        self.used += size;
        Ok(CodeBlock {
            arena: self.id,
            chunk: id,
            gen,
            offset: off,
            size,
            ptr,
        })
    }

    fn validate(&self, b: &CodeBlock) -> Result<&Chunk, ArenaError> {
        self.slots
            .get(b.chunk)
            .and_then(|s| s.as_ref())
            .filter(|c| b.arena == self.id && c.gen == b.gen && c.live.contains(&b.offset))
            .ok_or(ArenaError::InvalidBlock)
    }

    /// Write access to `block`, which must not have been committed yet.
    pub fn writer<'a>(&'a self, block: &'a mut CodeBlock) -> Result<CodeWriter<'a>, ArenaError> {
        let c = self.validate(block)?;
        if block.offset < c.sealed_end {
            return Err(ArenaError::NotWritable);
        }
        // SAFETY: the block is live, inside the chunk, in the RW region; `&mut CodeBlock` guarantees unique
        // access to these bytes (blocks never overlap) and the shared arena borrow prevents commit/free.
        let bytes = unsafe { std::slice::from_raw_parts_mut(block.ptr, block.size) };
        Ok(CodeWriter { bytes })
    }

    fn protect(&mut self, base: *mut u8, len: usize, prot: i32) -> Result<(), ArenaError> {
        self.mprotects += 1;
        // SAFETY: callers pass a page-aligned range inside a chunk mapping owned by this arena.
        let rc = unsafe { libc::mprotect(base as *mut libc::c_void, len, prot) };
        if rc != 0 {
            Err(ArenaError::Os(errno()))
        } else {
            Ok(())
        }
    }

    /// Flips all pages written since the last commit from RW to RX: one `mprotect` per chunk touched. After
    /// success every block allocated so far may be executed. Remaining pages of a chunk stay RW and are used
    /// by later allocations.
    ///
    /// # Partial failure
    /// Chunks are committed one at a time. If an `mprotect` fails (for example `EACCES` under a W^X policy)
    /// the error is returned immediately; chunks committed before the failure stay committed (and their
    /// blocks executable), the failing chunk and the not yet processed ones stay pending and fully
    /// consistent (their blocks remain writable and `entry` reports `NotCommitted`). The call can simply be
    /// retried, and freeing/dropping is always safe.
    pub fn commit(&mut self) -> Result<(), ArenaError> {
        while let Some(&id) = self.pending.last() {
            let c = self.chunk(id);
            let (base, start, bump) = (c.base, c.sealed_end, c.bump);
            let end = align_up(bump, page_size()).map_or(c.len, |e| e.min(c.len));
            // SAFETY: `[bump, end)` is inside the chunk and still RW (above sealed_end).
            unsafe { std::ptr::write_bytes(base.add(bump), INT3, end - bump) };
            // SAFETY: `start` is page aligned and `<= end <= len`.
            let p = unsafe { base.add(start) };
            self.protect(p, end - start, libc::PROT_READ | libc::PROT_EXEC)?;
            let c = self.slots[id].as_mut().expect("code chunk slot is live");
            c.sealed_end = end;
            c.used_end = bump;
            c.bump = end;
            self.pending.pop();
        }
        Ok(())
    }

    /// Packs the next allocations into the partially used last page of the current chunk: flips that page
    /// back from RX to RW (so `[page_start, sealed_end)` becomes writable again, with `bump` at the end of
    /// the last block) and marks the chunk pending, so the following [`commit`](Self::commit) seals it again.
    /// Returns false (nothing changed) if there is nothing to reuse, a commit is pending, or the
    /// `mprotect` failed.
    ///
    /// The caller must call `commit` before any code of that page runs again, and no thread may execute
    /// code in that page while it is writable. A compile is synchronous and never runs generated code, so
    /// the single thread that owns the state can reopen and reseal inside one compile (suspended compiled
    /// frames only resume after the compile returned). If the reseal fails the page stays RW: the caller
    /// must treat that as fatal for the blocks on it (`rt` aborts, see docs/UNSAFE.md).
    pub fn reopen_tail(&mut self) -> bool {
        if !self.pending.is_empty() {
            return false;
        }
        let Some(id) = self.current else {
            return false;
        };
        let pg = page_size();
        let c = self.chunk(id);
        if c.bump != c.sealed_end || c.used_end == 0 {
            return false;
        }
        let start = c.used_end & !(pg - 1);
        if c.used_end == start || start >= c.sealed_end {
            return false; // the last page is full (or empty): nothing to pack into
        }
        // SAFETY: `[start, sealed_end)` is page aligned, inside the chunk, and currently RX.
        let (p, len) = (unsafe { c.base.add(start) }, c.sealed_end - start);
        if self
            .protect(p, len, libc::PROT_READ | libc::PROT_WRITE)
            .is_err()
        {
            return false;
        }
        let c = self.slots[id].as_mut().expect("code chunk slot is live");
        c.bump = c.used_end;
        c.sealed_end = start;
        self.pending.push(id);
        true
    }

    /// Total bytes of sealed (committed) pages over all live chunks.
    pub fn sealed_bytes(&self) -> usize {
        self.slots.iter().flatten().map(|c| c.sealed_end).sum()
    }

    /// Returns the block's address as a function pointer type `F` (a `fn`/`extern "C" fn` pointer).
    ///
    /// # Safety
    /// `F` must be a pointer-sized `Copy` function pointer type (enforced at compile time) whose ABI and
    /// signature match the machine code in the block. The returned pointer is an ordinary value that the
    /// borrow checker no longer tracks, and the arena being `!Sync` does not prevent other threads from
    /// calling it: the caller must guarantee that the block stays live (not freed, arena not dropped) for as
    /// long as the pointer is called, and that no thread is executing inside it when it is freed. Fails with
    /// `NotCommitted` if the block's pages are still writable.
    pub unsafe fn entry<F: Copy>(&self, block: &CodeBlock) -> Result<F, ArenaError> {
        let () = AssertPtrSized::<F>::OK;
        let c = self.validate(block)?;
        if block.offset + block.size > c.sealed_end {
            return Err(ArenaError::NotCommitted);
        }
        // SAFETY: `F` is pointer sized (const assertion above); validity of the code is the caller's contract.
        Ok(unsafe { std::mem::transmute_copy::<*mut u8, F>(&block.ptr) })
    }

    /// Releases a block. When the last live block of a chunk is freed the chunk is pooled (up to 2
    /// standard-size chunks, kept RX until reused), unmapped, or in quarantine mode made `PROT_NONE` and
    /// kept. A double free or a foreign/stale block is `InvalidBlock`.
    ///
    /// # Safety
    /// No thread may be executing inside the block, and no copy of its entry pointer may be called after.
    pub unsafe fn free(&mut self, block: CodeBlock) -> Result<(), ArenaError> {
        self.validate(&block)?;
        let id = block.chunk;
        let c = self.slots[id].as_mut().expect("code chunk slot is live");
        c.live.remove(&block.offset);
        let empty = c.live.is_empty();
        self.used -= block.size;
        if empty {
            self.release_chunk(id);
        } else {
            // give the whole pages inside the freed block back to the OS (the partial pages at its ends
            // may hold neighbours); the space is reused only when the chunk empties
            let pg = page_size();
            let s = align_up(block.offset, pg).unwrap_or(usize::MAX);
            let e = (block.offset + block.size) & !(pg - 1);
            if e > s {
                // SAFETY: `[s, e)` is page aligned and lies inside the freed block, hence inside this
                // arena's private anonymous mapping and unused by any live block; MADV_DONTNEED only drops
                // the pages (they read back as zero).
                unsafe {
                    libc::madvise(
                        block.ptr.add(s - block.offset).cast::<libc::c_void>(),
                        e - s,
                        libc::MADV_DONTNEED,
                    );
                }
            }
        }
        Ok(())
    }

    fn release_chunk(&mut self, id: usize) {
        self.pending.retain(|&p| p != id);
        if self.current == Some(id) {
            self.current = None;
        }
        let mut c = self.slots[id].take().expect("code chunk slot is live");
        self.free_slots.push(id);
        // Stale handles to this chunk must fail validation even if the slot is reused.
        c.gen = self.next_gen;
        self.next_gen += 1;
        if self.quarantine {
            // Best effort: a failed mprotect leaves the chunk mapped and charged, never reused.
            let _ = self.protect(c.base, c.len, libc::PROT_NONE);
            self.quarantined.push(c);
            return;
        }
        if c.len == self.chunk_size && self.pool.len() < MAX_POOL {
            // The RX -> RW flip is deferred to `revive` (reuse). Trim memory beyond the resident budget.
            let pooled: usize = self.pool.iter().map(|p| p.len).sum();
            if pooled + c.len > POOL_RESIDENT {
                // SAFETY: the range is exactly this arena's private anonymous mapping with no live blocks;
                // MADV_DONTNEED only drops its pages (they read back as zero and are re-filled on use).
                unsafe {
                    libc::madvise(c.base as *mut libc::c_void, c.len, libc::MADV_DONTNEED);
                }
            }
            self.pool.push(c);
            return;
        }
        self.unmap_chunk(c);
    }

    /// Registers `block` under `name` in the perf map (no-op unless `RUSTYNAIL_JIT_PERFMAP=1`).
    pub fn register_symbol(&mut self, block: &CodeBlock, name: &str) -> std::io::Result<()> {
        match &mut self.perf {
            Some(p) => p.write_symbol(block.addr(), block.size, name),
            None => Ok(()),
        }
    }

    /// Replaces the perf map (tests, or explicit enabling by the embedder).
    pub fn set_perf_map(&mut self, map: Option<PerfMap>) {
        self.perf = map;
    }

    /// `(base address, length, sealed_end)` of every mapped chunk (live and pooled), for layout tests.
    #[cfg(test)]
    fn chunk_layout(&self) -> Vec<(usize, usize, usize)> {
        self.slots
            .iter()
            .flatten()
            .chain(self.pool.iter())
            .map(|c| (c.base as usize, c.len, c.sealed_end))
            .collect()
    }
}

impl Drop for CodeArena {
    /// Unmaps every chunk, including live code: entry pointers obtained from this arena dangle afterwards.
    fn drop(&mut self) {
        for c in self.slots.drain(..).flatten() {
            c.unmap();
        }
        for c in self.pool.drain(..) {
            c.unmap();
        }
        for c in self.quarantined.drain(..) {
            c.unmap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // mov eax, 42; ret
    const RET42: [u8; 6] = [0xb8, 0x2a, 0, 0, 0, 0xc3];
    // lea rax,[rdi+rdi*2+1]; ret
    const LEA3: [u8; 6] = [0x48, 0x8d, 0x44, 0x7f, 0x01, 0xc3];
    // lea rax,[rdi+rsi]; ret
    const ADD: [u8; 5] = [0x48, 0x8d, 0x04, 0x37, 0xc3];

    fn put(a: &mut CodeArena, code: &[u8]) -> CodeBlock {
        let mut b = a.alloc(code.len(), 16).unwrap();
        a.writer(&mut b).unwrap().write_at(0, code);
        b
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn probe_works() {
        assert_eq!(probe_exec(), Ok(()));
        assert_eq!(probe_exec(), Ok(()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn call_functions() {
        let mut a = CodeArena::new();
        let b1 = put(&mut a, &RET42);
        let b2 = put(&mut a, &LEA3);
        let b3 = put(&mut a, &ADD);
        assert_eq!(b1.addr() % 16, 0);
        assert_eq!(b2.addr() % 16, 0);
        // SAFETY: only checks the error path (nothing is called).
        let early = unsafe { a.entry::<extern "C" fn() -> u32>(&b1) };
        assert_eq!(early.err(), Some(ArenaError::NotCommitted));
        a.commit().unwrap();
        // SAFETY: blocks are live, committed, and hold code matching these signatures.
        unsafe {
            let f1: extern "C" fn() -> u32 = a.entry(&b1).unwrap();
            let f2: extern "C" fn(u64) -> u64 = a.entry(&b2).unwrap();
            let f3: extern "C" fn(u64, u64) -> u64 = a.entry(&b3).unwrap();
            assert_eq!(f1(), 42);
            assert_eq!(f2(5), 16);
            assert_eq!(f3(40, 2), 42);
            // sub rsp,8; mov rax, imm64; call rax; add rsp,8; ret
            let mut code = vec![0x48, 0x83, 0xec, 0x08, 0x48, 0xb8];
            code.extend_from_slice(&(b2.addr() as u64).to_le_bytes());
            code.extend_from_slice(&[0xff, 0xd0, 0x48, 0x83, 0xc4, 0x08, 0xc3]);
            let b4 = put(&mut a, &code);
            // Writing to a committed block is refused.
            let mut b1 = b1;
            assert_eq!(a.writer(&mut b1).err(), Some(ArenaError::NotWritable));
            a.commit().unwrap();
            let f4: extern "C" fn(u64) -> u64 = a.entry(&b4).unwrap();
            assert_eq!(f4(10), 31);
            assert_eq!(f1(), 42, "earlier code unaffected by later commit");
            for b in [b1, b2, b3, b4] {
                a.free(b).unwrap();
            }
        }
        assert_eq!(a.used(), 0);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn padding_and_tail_are_int3() {
        let mut a = CodeArena::new();
        let b1 = put(&mut a, &[0xc3]);
        let b2 = put(&mut a, &[0xc3]);
        a.commit().unwrap();
        assert_eq!(b2.addr() - b1.addr(), 16);
        // SAFETY: reading committed (RX, readable) bytes inside the chunk.
        unsafe {
            let p = b1.addr() as *const u8;
            for i in 1..16 {
                assert_eq!(*p.add(i), 0xCC);
            }
            assert_eq!(*(b2.addr() as *const u8).add(1), 0xCC);
            let last = (b2.addr() | 4095) as *const u8;
            assert_eq!(*last, 0xCC);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn commit_batches_mprotect() {
        let mut a = CodeArena::new();
        let mut v = Vec::new();
        for _ in 0..50 {
            v.push(put(&mut a, &RET42));
        }
        assert_eq!(a.mprotect_calls(), 0);
        a.commit().unwrap();
        assert_eq!(a.mprotect_calls(), 1);
        a.commit().unwrap();
        assert_eq!(a.mprotect_calls(), 1, "empty commit is free");
        // Two chunks pending -> two mprotects.
        let mut a = CodeArena::with_config(4096, usize::MAX);
        let _x = a.alloc(3000, 16).unwrap();
        let _y = a.alloc(3000, 16).unwrap();
        a.commit().unwrap();
        assert_eq!(a.mprotect_calls(), 2);
        assert_eq!(a.live_chunks(), 2);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn rest_of_chunk_reused_after_commit() {
        let mut a = CodeArena::new();
        let x = put(&mut a, &RET42);
        a.commit().unwrap();
        let y = put(&mut a, &RET42);
        assert_eq!(y.addr(), x.addr() + 4096, "next page of the same chunk");
        a.commit().unwrap();
        assert_eq!(a.live_chunks(), 1);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cap_exceeded() {
        let mut a = CodeArena::with_cap(4096);
        let b = a.alloc(64, 16).unwrap();
        assert_eq!((a.used(), a.mapped(), a.cap()), (64, 4096, 4096));
        // Needs a second chunk: over the cap.
        assert_eq!(a.alloc(4096, 16).err(), Some(ArenaError::CapExceeded));
        assert_eq!(
            a.alloc(MAX_BLOCK_SIZE, 16).err(),
            Some(ArenaError::CapExceeded)
        );
        // Same chunk still has room.
        let c = a.alloc(100, 16).unwrap();
        // SAFETY: never executed.
        unsafe {
            a.free(b).unwrap();
            a.free(c).unwrap();
        }
        assert_eq!(a.mapped(), 0, "odd-sized chunk is unmapped and refunded");
        a.alloc(4096, 16).unwrap();
        assert_eq!(a.alloc(0, 16).err(), Some(ArenaError::BadRequest));
        assert_eq!(a.alloc(8, 3).err(), Some(ArenaError::BadRequest));
        // Absurd sizes: error, not abort.
        let mut a = CodeArena::new();
        assert_eq!(
            a.alloc(usize::MAX - 8, 16).err(),
            Some(ArenaError::BadRequest)
        );
        assert_eq!(a.alloc(1 << 62, 16).err(), Some(ArenaError::BadRequest));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn alloc_align_32_and_oversized() {
        let mut a = CodeArena::with_config(4096, usize::MAX);
        a.alloc(1, 16).unwrap();
        let b = a.alloc(5, 32).unwrap();
        assert_eq!(b.addr() % 32, 0);
        let mut big = a.alloc(10_000, 16).unwrap();
        a.writer(&mut big).unwrap().write_at(9_999, &[0xc3]);
        a.commit().unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn free_and_reuse_chunk() {
        let mut a = CodeArena::new();
        let b = put(&mut a, &RET42);
        a.commit().unwrap();
        let first = b.addr();
        let calls = a.mprotect_calls();
        // SAFETY: not executing.
        unsafe { a.free(b).unwrap() };
        assert_eq!(a.live_chunks(), 1, "pooled");
        assert_eq!(
            a.mprotect_calls(),
            calls,
            "RX->RW flip is deferred until reuse"
        );
        let mut b = a.alloc(6, 16).unwrap();
        assert_eq!(a.mprotect_calls(), calls + 1, "flipped back to RW on reuse");
        assert_eq!(b.addr(), first, "same chunk reused");
        a.writer(&mut b).unwrap().write_at(0, &RET42);
        a.commit().unwrap();
        // SAFETY: live, committed, matches signature.
        let f: extern "C" fn() -> u32 = unsafe { a.entry(&b).unwrap() };
        assert_eq!(f(), 42);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn double_free_and_foreign_block_are_errors() {
        let mut a = CodeArena::new();
        let mut other = CodeArena::new();
        let b = a.alloc(8, 16).unwrap();
        let foreign = other.alloc(8, 16).unwrap();
        // Forge a second handle to the same block (private fields; test-only).
        let dup = CodeBlock {
            arena: b.arena,
            chunk: b.chunk,
            gen: b.gen,
            offset: b.offset,
            size: b.size,
            ptr: b.ptr,
        };
        // SAFETY: nothing executes.
        unsafe {
            assert_eq!(a.free(foreign).err(), Some(ArenaError::InvalidBlock));
            let keep = a.alloc(8, 16).unwrap(); // keeps the chunk alive: exercises the offset check
            a.free(b).unwrap();
            assert_eq!(a.free(dup).err(), Some(ArenaError::InvalidBlock));
            a.free(keep).unwrap();
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn stress_many_blocks_no_leak() {
        let mut a = CodeArena::with_config(64 * 1024, usize::MAX);
        let mut blocks = Vec::new();
        for i in 0..10_000 {
            blocks.push(put(&mut a, &RET42));
            // Commit rarely so that many chunks fill up (16 bytes per block).
            if i % 2500 == 0 {
                a.commit().unwrap();
            }
        }
        a.commit().unwrap();
        assert!(a.live_chunks() > 1);
        assert_eq!(a.used(), 10_000 * RET42.len());
        // SAFETY: live, committed, signature matches.
        let f: extern "C" fn() -> u32 = unsafe { a.entry(&blocks[1234]).unwrap() };
        assert_eq!(f(), 42);
        for b in blocks {
            // SAFETY: not executing.
            unsafe { a.free(b).unwrap() };
        }
        assert_eq!(a.used(), 0);
        assert!(
            a.live_chunks() <= MAX_POOL,
            "chunks left: {}",
            a.live_chunks()
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn perf_map_format() {
        let mut a = CodeArena::new();
        let path = format!("/tmp/perf-{}.map", std::process::id());
        let _ = std::fs::remove_file(&path);
        a.set_perf_map(Some(PerfMap::open(std::process::id()).unwrap()));
        let b = a.alloc(6, 16).unwrap();
        a.register_symbol(&b, "lua:test_proto").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(
            text.contains(&format!("{:x} 6 lua:test_proto\n", b.addr())),
            "{text}"
        );
    }

    const MDWE_ENV: &str = "RUSTYNAIL_MDWE_CHILD";

    /// Child half of `probe_fails_under_mdwe`; a no-op in a normal run.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn mdwe_child() {
        if std::env::var(MDWE_ENV).is_err() {
            return;
        }
        const PR_SET_MDWE: i32 = 65;
        const PR_MDWE_REFUSE_EXEC_GAIN: u64 = 1;
        // SAFETY: prctl with integer arguments.
        let rc = unsafe { libc::prctl(PR_SET_MDWE, PR_MDWE_REFUSE_EXEC_GAIN, 0u64, 0u64, 0u64) };
        if rc != 0 {
            println!("MDWE-SKIP errno={}", errno());
            std::process::exit(77);
        }
        match probe_exec() {
            Err(e) => {
                println!("MDWE-OK probe failed gracefully: {e}");
                std::process::exit(0)
            }
            Ok(()) => {
                println!("MDWE-BAD probe succeeded under MDWE");
                std::process::exit(1)
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn probe_fails_under_mdwe() {
        let exe = std::env::current_exe().unwrap();
        let out = std::process::Command::new(exe)
            .args([
                "--exact",
                "mem::tests::mdwe_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(MDWE_ENV, "1")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        match out.status.code() {
            Some(0) => assert!(text.contains("MDWE-OK"), "{text}"),
            Some(77) => eprintln!("SKIP: kernel does not support PR_SET_MDWE ({text})"),
            c => panic!(
                "child failed: {c:?}\n{text}\n{}",
                String::from_utf8_lossy(&out.stderr)
            ),
        }
    }

    // ------------------------------------------------------------------------------------------------
    // Review additions
    // ------------------------------------------------------------------------------------------------

    /// A1: the review's scenario. Every alloc commits one page, one block is kept and the rest freed; mapped
    /// memory must never exceed the cap and chunks must not pile up.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a1_cap_bounds_mapped_memory() {
        for cap in [4096usize, 3 * 4096, 1 << 20] {
            let mut a = CodeArena::with_cap(cap);
            let mut kept: Vec<CodeBlock> = Vec::new();
            let mut hit_cap = false;
            for _ in 0..300 {
                let mut round = Vec::new();
                for _ in 0..3 {
                    match a.alloc(1, 16) {
                        Ok(b) => round.push(b),
                        Err(e) => {
                            assert_eq!(e, ArenaError::CapExceeded);
                            hit_cap = true;
                        }
                    }
                    assert!(a.mapped() <= cap, "mapped {} > cap {cap}", a.mapped());
                }
                a.commit().unwrap();
                assert!(a.mapped() <= cap);
                let mut it = round.into_iter();
                if let Some(first) = it.next() {
                    kept.push(first);
                }
                for b in it {
                    // SAFETY: never executed.
                    unsafe { a.free(b).unwrap() };
                }
                assert!(a.live_chunks() <= cap / 4096 + MAX_POOL);
                assert!(a.mapped() <= cap);
            }
            assert!(hit_cap, "cap {cap} never reached");
            assert!(a.mapped() <= cap);
            // Everything freed: all chunks other than the pool are gone, mapped is within the cap.
            for b in kept {
                // SAFETY: never executed.
                unsafe { a.free(b).unwrap() };
            }
            assert_eq!(a.used(), 0);
            assert!(a.live_chunks() <= MAX_POOL);
            assert!(a.mapped() <= cap);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn a1_pool_is_reclaimed_to_make_room_under_cap() {
        let mut a = CodeArena::with_config(8192, 3 * 8192);
        let blocks: Vec<_> = (0..3).map(|_| a.alloc(8192, 16).unwrap()).collect();
        assert_eq!(a.mapped(), 3 * 8192);
        for b in blocks {
            // SAFETY: never executed.
            unsafe { a.free(b).unwrap() };
        }
        // Two chunks pooled (still charged), one unmapped.
        assert_eq!((a.live_chunks(), a.mapped()), (2, 2 * 8192));
        // An oversized request must evict the pool to fit, not fail.
        let big = a.alloc(3 * 8192, 16).unwrap();
        assert_eq!(a.mapped(), 3 * 8192);
        assert_eq!(a.alloc(1, 16).err(), Some(ArenaError::CapExceeded));
        let _ = big;
    }

    /// A5: finite default cap, hard per-block maximum, `with_cap` raises it.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a5_default_cap_and_max_block() {
        let mut a = CodeArena::new();
        assert_eq!(a.cap(), DEFAULT_CAP);
        assert_eq!(DEFAULT_CAP, 256 << 20);
        assert_eq!(
            a.alloc(MAX_BLOCK_SIZE + 1, 16).err(),
            Some(ArenaError::BadRequest)
        );
        drop(a);
        let a = CodeArena::with_cap(1 << 30);
        assert_eq!(a.cap(), 1 << 30);
    }

    /// A4: chunk size clamping and checked arithmetic.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a4_with_config_clamps_and_checked_math() {
        let page = page_size();
        assert_eq!(CodeArena::with_config(0, 1 << 20).chunk_size(), page);
        assert_eq!(CodeArena::with_config(1, 1 << 20).chunk_size(), page);
        assert_eq!(
            CodeArena::with_config(page + 1, 1 << 20).chunk_size(),
            2 * page
        );
        assert_eq!(
            CodeArena::with_config(usize::MAX, 1 << 20).chunk_size(),
            MAX_CHUNK_SIZE
        );
        assert_eq!(
            CodeArena::with_config(usize::MAX - 3, 1 << 20).chunk_size(),
            MAX_CHUNK_SIZE
        );
        assert_eq!(align_up(usize::MAX, 4096), None);
        assert_eq!(align_up(1, 4096), Some(4096));
        let mut a = CodeArena::new();
        let mut b = a.alloc(8, 16).unwrap();
        let mut w = a.writer(&mut b).unwrap();
        assert_eq!(w.try_write_at(0, &[1; 8]), Ok(()));
        assert_eq!(w.try_write_at(1, &[1; 8]), Err(ArenaError::BadRequest));
        assert_eq!(w.try_write_at(9, &[]), Err(ArenaError::BadRequest));
        assert_eq!(w.try_write_at(8, &[]), Ok(()));
        assert_eq!(
            w.try_write_at(usize::MAX, &[1, 2]),
            Err(ArenaError::BadRequest)
        );
        assert_eq!(&w[..], &[1; 8]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[should_panic(expected = "write_at out of range")]
    fn a4_write_at_panics_clearly() {
        let mut a = CodeArena::new();
        let mut b = a.alloc(4, 16).unwrap();
        a.writer(&mut b).unwrap().write_at(usize::MAX, &[1, 2, 3]);
    }

    /// A8: freeing the last block costs no syscall; the chunk stays RX until reused.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a8_deferred_flip_and_pool_trim() {
        let mut a = CodeArena::new();
        let b = put(&mut a, &RET42);
        a.commit().unwrap();
        let calls = a.mprotect_calls();
        // SAFETY: not executing.
        unsafe { a.free(b).unwrap() };
        assert_eq!(a.mprotect_calls(), calls);
        assert_eq!(a.live_chunks(), 1);
        let (base, _len, sealed) = a.chunk_layout()[0];
        assert_eq!(sealed, 4096);
        assert_eq!(perms_at(base), "r-xp", "pooled chunk is still RX");
        // Pool trimming (second pooled chunk) must leave a usable arena.
        let x = a.alloc(8, 16).unwrap();
        let y = a.alloc(DEFAULT_CHUNK_SIZE, 16).unwrap();
        a.commit().unwrap();
        // SAFETY: not executing.
        unsafe {
            a.free(x).unwrap();
            a.free(y).unwrap();
        }
        let mut z = a.alloc(16, 16).unwrap();
        a.writer(&mut z).unwrap().write_at(0, &RET42);
        a.commit().unwrap();
        // SAFETY: live, committed, matches signature.
        let f: extern "C" fn() -> u32 = unsafe { a.entry(&z).unwrap() };
        assert_eq!(f(), 42);
    }

    /// A9/(5): stale handles never validate after slot or chunk reuse.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn stale_handles_are_invalid_after_reuse() {
        fn forge(b: &CodeBlock) -> CodeBlock {
            CodeBlock {
                arena: b.arena,
                chunk: b.chunk,
                gen: b.gen,
                offset: b.offset,
                size: b.size,
                ptr: b.ptr,
            }
        }
        let mut a = CodeArena::new();
        let mut old = a.alloc(32, 16).unwrap();
        a.writer(&mut old).unwrap().write_at(0, &RET42);
        a.commit().unwrap();
        let stale = forge(&old);
        // SAFETY: not executing; `stale` is never called.
        unsafe { a.free(old).unwrap() };
        // The chunk is re-pooled and handed out again in the same slot at the same offset.
        let fresh = a.alloc(32, 16).unwrap();
        assert_eq!((fresh.chunk, fresh.offset), (stale.chunk, stale.offset));
        assert_ne!(fresh.gen, stale.gen);
        // SAFETY: only checks error paths.
        unsafe {
            assert_eq!(
                a.entry::<extern "C" fn() -> u32>(&stale).err(),
                Some(ArenaError::InvalidBlock)
            );
            assert_eq!(a.free(forge(&stale)).err(), Some(ArenaError::InvalidBlock));
        }
        let mut s2 = forge(&stale);
        assert_eq!(a.writer(&mut s2).err(), Some(ArenaError::InvalidBlock));
        // Second round: re-pooled again, still invalid.
        // SAFETY: not executing.
        unsafe { a.free(fresh).unwrap() };
        let again = a.alloc(32, 16).unwrap();
        assert_ne!(again.gen, stale.gen);
        // SAFETY: error path only.
        unsafe { assert_eq!(a.free(stale).err(), Some(ArenaError::InvalidBlock)) };
        // SAFETY: not executing.
        unsafe { a.free(again).unwrap() };
    }

    // -- /proc/self/maps helpers ------------------------------------------------------------------

    fn maps() -> Vec<(usize, usize, String)> {
        let text = std::fs::read_to_string("/proc/self/maps").unwrap();
        text.lines()
            .map(|l| {
                let mut it = l.split_whitespace();
                let range = it.next().unwrap();
                let perms = it.next().unwrap().to_string();
                let (s, e) = range.split_once('-').unwrap();
                (
                    usize::from_str_radix(s, 16).unwrap(),
                    usize::from_str_radix(e, 16).unwrap(),
                    perms,
                )
            })
            .collect()
    }

    fn perms_at(addr: usize) -> String {
        maps()
            .into_iter()
            .find(|(s, e, _)| (*s..*e).contains(&addr))
            .map(|m| m.2)
            .unwrap_or_else(|| panic!("{addr:#x} not mapped"))
    }

    /// No rwx mapping anywhere, and each chunk is RX below `sealed_end`, RW above.
    fn assert_wx(a: &CodeArena, what: &str) {
        let m = maps();
        assert!(
            m.iter().all(|(_, _, p)| !p.starts_with("rwx")),
            "rwx mapping after {what}"
        );
        for (base, len, sealed) in a.chunk_layout() {
            for off in (0..len).step_by(page_size()) {
                let addr = base + off;
                let p = &m
                    .iter()
                    .find(|(s, e, _)| (*s..*e).contains(&addr))
                    .unwrap_or_else(|| panic!("chunk page {addr:#x} unmapped after {what}"))
                    .2;
                let want = if off < sealed { "r-xp" } else { "rw-p" };
                assert_eq!(
                    p, want,
                    "page +{off:#x} of chunk (sealed {sealed:#x}) after {what}"
                );
            }
        }
    }

    /// (2) W^X holds after every operation of a mixed workload.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn wx_invariant_in_proc_maps() {
        let mut a = CodeArena::with_config(16 * 4096, usize::MAX);
        assert_wx(&a, "new");
        let mut live: Vec<CodeBlock> = Vec::new();
        for round in 0..12usize {
            for k in 0..5 {
                let size = [6usize, 300, 4096, 5000, 70_000][(round + k) % 5];
                let mut b = a.alloc(size, 16).unwrap();
                a.writer(&mut b).unwrap().write_at(0, &RET42);
                live.push(b);
                assert_wx(&a, "alloc");
            }
            a.commit().unwrap();
            assert_wx(&a, "commit");
            if round % 2 == 1 {
                let n = live.len();
                for i in (0..n).rev().step_by(2) {
                    let b = live.remove(i);
                    // SAFETY: not executing.
                    unsafe { a.free(b).unwrap() };
                    assert_wx(&a, "free");
                }
            }
        }
        for b in live {
            // SAFETY: not executing.
            unsafe { a.free(b).unwrap() };
            assert_wx(&a, "final free");
        }
    }

    // -- child-process tests ------------------------------------------------------------------------

    const CHILD_ENV: &str = "RUSTYNAIL_JIT_CHILD";

    fn in_child() -> bool {
        std::env::var(CHILD_ENV).is_ok()
    }

    /// Re-executes this test binary running only `name`; returns (exit code, terminating signal, stdout).
    fn run_child(name: &str, extra_env: &[(&str, &str)]) -> (Option<i32>, Option<i32>, String) {
        use std::os::unix::process::ExitStatusExt;
        let exe = std::env::current_exe().unwrap();
        let mut cmd = std::process::Command::new(exe);
        cmd.args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(CHILD_ENV, "1")
            .stdin(std::process::Stdio::null());
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let out = cmd.output().unwrap();
        (
            out.status.code(),
            out.status.signal(),
            String::from_utf8_lossy(&out.stdout).into_owned()
                + &String::from_utf8_lossy(&out.stderr),
        )
    }

    fn committed_ret42(a: &mut CodeArena) -> CodeBlock {
        let b = put(a, &RET42);
        a.commit().unwrap();
        b
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn child_write_to_committed_page() {
        if !in_child() {
            return;
        }
        let mut a = CodeArena::new();
        let b = committed_ret42(&mut a);
        // SAFETY: deliberately faulting store into an RX page; the process must die with SIGSEGV.
        unsafe { std::ptr::write_volatile(b.addr() as *mut u8, 0) };
        println!("SURVIVED");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn child_call_uncommitted_page() {
        if !in_child() {
            return;
        }
        let mut a = CodeArena::new();
        let b = put(&mut a, &RET42);
        // SAFETY: deliberately calls a non-executable (still RW) page; must die with SIGSEGV.
        let f: extern "C" fn() -> u32 = unsafe { std::mem::transmute(b.addr()) };
        println!("{}", f());
        println!("SURVIVED");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn child_call_freed_block_in_quarantine() {
        if !in_child() {
            return;
        }
        // Enabled through the environment, as the parent sets RUSTYNAIL_JIT=verify.
        let mut a = CodeArena::new();
        assert!(a.quarantine_enabled());
        let b = committed_ret42(&mut a);
        // SAFETY: live, committed, matches the signature.
        let f: extern "C" fn() -> u32 = unsafe { a.entry(&b).unwrap() };
        assert_eq!(f(), 42);
        // SAFETY: the stale pointer `f` is deliberately called after the free below.
        unsafe { a.free(b).unwrap() };
        assert_eq!(a.live_chunks(), 1, "quarantined chunk stays mapped");
        println!("CALLING-FREED");
        println!("{}", f());
        println!("SURVIVED");
    }

    fn expect_sigsegv(name: &str, env: &[(&str, &str)], marker: Option<&str>) {
        let (code, sig, out) = run_child(name, env);
        // Under ASan the sanitizer's SEGV handler reports the fault and exits with status 1 instead.
        let asan = code == Some(1) && out.contains("AddressSanitizer: SEGV");
        assert!(
            sig == Some(libc::SIGSEGV) || asan,
            "exit {code:?}, signal {sig:?}, output: {out}"
        );
        assert!(!out.contains("SURVIVED"), "{out}");
        if let Some(m) = marker {
            assert!(out.contains(m), "{out}");
        }
    }

    /// (3) writing to a committed page and calling an uncommitted page both fault.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn faults_on_write_to_committed_and_call_uncommitted() {
        expect_sigsegv("mem::tests::child_write_to_committed_page", &[], None);
        expect_sigsegv("mem::tests::child_call_uncommitted_page", &[], None);
    }

    /// A6: a freed block's entry pointer faults in quarantine mode (enabled via `RUSTYNAIL_JIT=verify`).
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a6_quarantine_faults_on_stale_call() {
        expect_sigsegv(
            "mem::tests::child_call_freed_block_in_quarantine",
            &[("RUSTYNAIL_JIT", "verify")],
            Some("CALLING-FREED"),
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn a6_quarantine_api() {
        let mut a = CodeArena::with_config(4096, usize::MAX);
        a.set_quarantine(true);
        let b = put(&mut a, &RET42);
        let keep = put(&mut a, &RET42);
        a.commit().unwrap();
        let base = b.addr();
        // SAFETY: not executing.
        unsafe { a.free(b).unwrap() };
        assert_eq!(
            perms_at(base),
            "r-xp",
            "chunk shared with a live block stays executable"
        );
        // SAFETY: not executing.
        unsafe { a.free(keep).unwrap() };
        assert_eq!(perms_at(base), "---p", "fully freed chunk is PROT_NONE");
        // Never reused: the next allocation maps a different chunk, the quarantined one stays charged.
        let n = a.alloc(8, 16).unwrap();
        assert_ne!(n.addr() & !4095, base & !4095);
        assert_eq!(a.live_chunks(), 2);
        assert_eq!(a.mapped(), 2 * 4096);
        a.set_quarantine(false);
        assert!(!a.quarantine_enabled());
    }

    /// (4) Under MDWE the commit fails, the writer keeps working, entry says NotCommitted, free/drop are clean.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn child_commit_fails_under_mdwe() {
        if !in_child() {
            return;
        }
        const PR_SET_MDWE: i32 = 65;
        const PR_MDWE_REFUSE_EXEC_GAIN: u64 = 1;
        // SAFETY: prctl with integer arguments.
        let rc = unsafe { libc::prctl(PR_SET_MDWE, PR_MDWE_REFUSE_EXEC_GAIN, 0u64, 0u64, 0u64) };
        if rc != 0 {
            println!("MDWE-SKIP errno={}", errno());
            std::process::exit(77);
        }
        let mut a = CodeArena::new();
        let mut b = a.alloc(16, 16).unwrap();
        a.writer(&mut b).unwrap().write_at(0, &RET42);
        assert_eq!(a.commit(), Err(ArenaError::Os(libc::EACCES)));
        // The state is consistent: still writable, retry fails the same way, not executable.
        a.writer(&mut b).unwrap().write_at(0, &RET42);
        assert_eq!(a.commit(), Err(ArenaError::Os(libc::EACCES)));
        // SAFETY: only checks the error path; nothing is called.
        let e = unsafe { a.entry::<extern "C" fn() -> u32>(&b) };
        assert_eq!(e.err(), Some(ArenaError::NotCommitted));
        // SAFETY: never executed.
        unsafe { a.free(b).unwrap() };
        assert_eq!(a.used(), 0);
        let mut c = a.alloc(16, 16).unwrap();
        a.writer(&mut c).unwrap().write_at(0, &RET42);
        drop(a);
        println!("MDWE-COMMIT-OK");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn commit_failure_under_mdwe_is_clean() {
        let (code, sig, out) = run_child("mem::tests::child_commit_fails_under_mdwe", &[]);
        match code {
            Some(0) => assert!(out.contains("MDWE-COMMIT-OK"), "{out}"),
            Some(77) => eprintln!("SKIP: kernel does not support PR_SET_MDWE"),
            c => panic!("child failed: {c:?} sig {sig:?}\n{out}"),
        }
    }

    // -- A7: perf map ---------------------------------------------------------------------------------

    fn unique_pid() -> u32 {
        static N: AtomicU64 = AtomicU64::new(0);
        4_000_000_000
            + (std::process::id() % 100_000) * 1000
            + N.fetch_add(1, Ordering::Relaxed) as u32
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn a7_perf_map_refuses_symlinks_and_foreign_files() {
        let pid = unique_pid();
        let path = format!("/tmp/perf-{pid}.map");
        let _ = std::fs::remove_file(&path);
        let target = format!("/tmp/perf-target-{pid}");
        std::fs::write(&target, "x").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let r = PerfMap::open(pid);
        assert!(r.is_err(), "symlink must be refused");
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "x",
            "target untouched"
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&target);
        // A directory is not a regular file: also refused (open for append fails).
        std::fs::create_dir(&path).unwrap();
        assert!(PerfMap::open(pid).is_err());
        let _ = std::fs::remove_dir(&path);
        // A normal file works, mode 0600, newlines in names are neutralised.
        let mut m = PerfMap::open(pid).unwrap();
        m.write_symbol(0x1000, 0x20, "a\nb").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, "1000 20 a b\n");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Child half: fork, the grandchild writes a symbol through a map opened before the fork.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn child_perf_map_fork() {
        if !in_child() {
            return;
        }
        let mut m = PerfMap::open_current().unwrap();
        m.write_symbol(0x1000, 0x10, "parent").unwrap();
        // SAFETY: the test binary runs with --test-threads=1 so this is the only thread; the forked child
        // only calls write_symbol (allocation is fine single-threaded) and `_exit`.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            let ok = m.write_symbol(0x2000, 0x10, "child").is_ok();
            // SAFETY: terminating the forked child without running parent-owned destructors.
            unsafe { libc::_exit(if ok { 0 } else { 3 }) };
        }
        let mut status = 0;
        // SAFETY: waiting for the child we just forked.
        unsafe { libc::waitpid(pid, &mut status, 0) };
        assert!(
            libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
            "{status}"
        );
        let me = std::fs::read_to_string(format!("/tmp/perf-{}.map", std::process::id())).unwrap();
        let ch = std::fs::read_to_string(format!("/tmp/perf-{pid}.map")).unwrap();
        let _ = std::fs::remove_file(format!("/tmp/perf-{}.map", std::process::id()));
        let _ = std::fs::remove_file(format!("/tmp/perf-{pid}.map"));
        assert_eq!(
            me, "1000 10 parent\n",
            "child must not write into the parent's file"
        );
        assert_eq!(ch, "2000 10 child\n");
        println!("FORK-OK");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn a7_perf_map_follows_fork() {
        let (code, sig, out) = run_child("mem::tests::child_perf_map_fork", &[]);
        assert_eq!(code, Some(0), "sig {sig:?}\n{out}");
        assert!(out.contains("FORK-OK"), "{out}");
    }

    // -- (6) integration with the encoder ------------------------------------------------------------

    /// Assemble with the `Assembler`, load into a `CodeArena`, call it.
    #[test]
    #[cfg(target_arch = "x86_64")]
    #[cfg_attr(miri, ignore)]
    fn assembler_to_arena_integration() {
        use crate::arch::x86_64::*;
        // fn(n: u64, p: *const u64) -> u64 { sum of p[0..n] * 3 + *constant }; uses a loop, labels and a
        // rip-relative constant placed after the code.
        let mut asm = Assembler::new();
        let (top, done, konst) = (asm.new_label(), asm.new_label(), asm.new_label());
        asm.mov_ri(RAX, 0);
        asm.mov_ri(RCX, 0);
        asm.bind(top);
        asm.alu_rr(Alu::Cmp, Size::S64, RCX, RDI);
        asm.jcc_label(Cond::Ae, done);
        asm.alu_rm(Alu::Add, Size::S64, RAX, Mem::sib(RSI, RCX, 8, 0));
        asm.inc(Size::S64, RCX);
        asm.jmp_label(top);
        asm.bind(done);
        asm.imul_rri(Size::S64, RAX, RAX, 3);
        asm.alu_rm(Alu::Add, Size::S64, RAX, Mem::rip(konst));
        asm.ret();
        asm.align(8);
        asm.bind(konst);
        asm.emit_bytes(&1000u64.to_le_bytes());
        let code = asm.finish().unwrap();

        let mut arena = CodeArena::new();
        let mut blk = arena.alloc(code.len(), 16).unwrap();
        arena.writer(&mut blk).unwrap().write_at(0, &code);
        arena.commit().unwrap();
        // SAFETY: the block holds the function assembled above with this exact signature.
        let f: extern "C" fn(u64, *const u64) -> u64 = unsafe { arena.entry(&blk).unwrap() };
        let data = [1u64, 2, 3, 4];
        assert_eq!(f(4, data.as_ptr()), 10 * 3 + 1000);
        assert_eq!(f(0, data.as_ptr()), 1000);
        // call_abs from a second block into the first
        let mut asm = Assembler::new();
        asm.push(RBX); // align the stack for the call
        asm.call_abs(blk.addr() as u64);
        asm.pop(RBX);
        asm.ret();
        let code2 = asm.finish().unwrap();
        let mut blk2 = arena.alloc(code2.len(), 16).unwrap();
        arena.writer(&mut blk2).unwrap().write_at(0, &code2);
        arena.commit().unwrap();
        // SAFETY: same signature as the callee, which this stub forwards to.
        let g: extern "C" fn(u64, *const u64) -> u64 = unsafe { arena.entry(&blk2).unwrap() };
        assert_eq!(g(3, data.as_ptr()), 6 * 3 + 1000);
        // SAFETY: nothing is executing.
        unsafe {
            arena.free(blk2).unwrap();
            arena.free(blk).unwrap();
        }
    }
}
