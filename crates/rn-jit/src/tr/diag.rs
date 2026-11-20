//! H8 diagnostics: why does a function or loop head not run in a trace?
//!
//! Two layers, both off by default:
//!
//! * **Counters** (`RUSTYNAIL_JIT_VERBOSE >= 1`): per head (prototype key, line, pc) the recordings started, the aborts by reason
//!   (root and side), the `StartRefused` sub-reasons, the flushes by cause, the installs, the entries of the flushed generations
//!   and when the head was blacklisted. They are updated on the slow paths of the policy only (a recording starting, ending, a
//!   flush), never on the dispatch path, and the table is printed when the state closes (`[heads]` lines).
//! * **Sampler** (`RUSTYNAIL_JIT_SAMPLE=<hz>` or `on` = 1000 Hz (the kernel timer tick bounds the real rate), with verbose >= 1): an `ITIMER_PROF` signal records the interrupted
//!   instruction pointer in a static buffer (no allocation, no lock in the handler); at close the samples are classified against
//!   the code ranges of the installed traces (per head) and of the tier-1 code of every prototype. Samples outside both (Rust
//!   helpers, the collector, the compiler) are reported as `runtime`. x86-64 and aarch64 Linux only.
//!
//! `RUSTYNAIL_JIT_PERFMAP=2` is the `perf` side of the same question: it names every installed trace `TRACE-<n>` and the tier-1
//! blocks `lua_proto:<line>@<ptr>` in `/tmp/perf-<pid>.map` (mem.rs); `[trace] <n> start proto:<line> pc` of `VERBOSE=2` maps a
//! trace number to its head, `scripts/heads-perf.py` joins the two.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

/// Counters of one head.
#[derive(Default, Debug, Clone)]
pub struct HeadDiag {
    /// Recordings started (root recordings of this head).
    pub started: u32,
    /// Root traces installed.
    pub installed: u32,
    /// Root recordings aborted, by reason.
    pub aborts: BTreeMap<&'static str, u32>,
    /// Side recordings (of the traces of this head) aborted, by reason.
    pub side_aborts: BTreeMap<&'static str, u32>,
    /// `StartRefused` sub-reasons of root starts (`side:` prefix for side starts).
    pub refused: BTreeMap<&'static str, u32>,
    /// Where the recorder aborted (`<reason>@L<line>:<pc>:<op>`, root and side recordings): the sites of the abort reasons.
    pub sites: BTreeMap<String, u32>,
    /// Flushes of the head's root trace, by cause.
    pub flushes: BTreeMap<&'static str, u32>,
    /// Most successful entries the head's counter showed (the counter is cumulative over the head's generations).
    pub entries_flushed: u64,
    /// The head as the prototype died with it: (state, kind, entry failures). `None` while it lives.
    pub fin: Option<(&'static str, &'static str, u32)>,
    /// When the head went `Black`: (`stats.started` at that time, ms since the first event, reason).
    pub black: Option<(u64, u64, &'static str)>,
}

/// All heads of a state.
#[derive(Default, Debug)]
pub struct Diag {
    /// (prototype key, line) -> `source:line` (filled when a prototype is compiled with the diagnostics on).
    pub names: BTreeMap<(usize, i32), String>,
    /// (prototype key, line, head pc) -> counters.
    pub heads: BTreeMap<(usize, i32, u32), HeadDiag>,
}

fn now_ms() -> u64 {
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    T0.get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u64
}

/// Which limit made the recorder abort `TooLong` (consumed with the site).
pub fn set_limit(l: &'static str) {
    LIMIT.with(|c| c.set(l));
}

fn take_limit() -> &'static str {
    LIMIT.with(|c| c.replace(""))
}

/// Takes the pending limit tag (the recorder's abort path appends it to the site).
pub fn limit_tag() -> &'static str {
    take_limit()
}

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Are the per-head counters on (`RUSTYNAIL_JIT_VERBOSE >= 1` in a state with traces)?
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Switches the counters on (a state with `verbose >= 1` was created).
pub fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

thread_local! {
    static LIMIT: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
    static SITE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    static WHY: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
}

/// The recorder aborted at `L<line>:<pc>:<op>` (the frame the recorder was in, which may be an inlined callee): consumed by the next
/// [`Diag::aborted`] of a recorder abort.
pub fn set_site(site: String) {
    SITE.with(|s| *s.borrow_mut() = site);
}

fn take_site() -> String {
    SITE.with(|s| std::mem::take(&mut *s.borrow_mut()))
}

/// The recorder start (root or side) is about to return "refused": remembers why (consumed by [`take_why`]).
pub fn set_why(w: &'static str) {
    WHY.with(|c| c.set(w));
}

/// The reason of the last refusal (`"unspecified"` if nobody said), cleared.
pub fn take_why() -> &'static str {
    WHY.with(|c| {
        let w = c.replace("");
        if w.is_empty() {
            "unspecified"
        } else {
            w
        }
    })
}

impl Diag {
    fn head(&mut self, key: usize, line: i32, pc: u32) -> &mut HeadDiag {
        self.heads.entry((key, line, pc)).or_default()
    }
    /// `source:line` of a prototype.
    pub fn name(&self, key: usize, line: i32) -> String {
        self.names
            .get(&(key, line))
            .cloned()
            .unwrap_or_else(|| format!("?:{line}"))
    }
    pub fn started(&mut self, key: usize, line: i32, pc: u32) {
        let _ = now_ms();
        self.head(key, line, pc).started += 1;
    }
    pub fn installed(&mut self, key: usize, line: i32, pc: u32) {
        self.head(key, line, pc).installed += 1;
    }
    #[allow(clippy::too_many_arguments)]
    pub fn aborted(
        &mut self,
        key: usize,
        line: i32,
        pc: u32,
        reason: &'static str,
        side: bool,
        why: Option<&'static str>,
    ) {
        let h = self.head(key, line, pc);
        let site = take_site();
        if !site.is_empty() && why.is_none() {
            *h.sites.entry(format!("{reason}@{site}")).or_insert(0) += 1;
        }
        *(if side {
            &mut h.side_aborts
        } else {
            &mut h.aborts
        })
        .entry(reason)
        .or_insert(0) += 1;
        if let Some(w) = why {
            let w = if side {
                match w {
                    "unspecified" => "side:unspecified",
                    _ => w,
                }
            } else {
                w
            };
            *h.refused.entry(w).or_insert(0) += 1;
        }
    }
    pub fn flushed(&mut self, key: usize, line: i32, pc: u32, why: &'static str, entries: u64) {
        let h = self.head(key, line, pc);
        *h.flushes.entry(why).or_insert(0) += 1;
        h.entries_flushed = h.entries_flushed.max(entries);
    }
    /// The prototype dies: remembers how its head ended.
    pub fn snapshot(&mut self, key: usize, line: i32, pc: u32, live: LiveHead) {
        if live.state == "Warm" && !self.heads.contains_key(&(key, line, pc)) {
            return;
        }
        let h = self.head(key, line, pc);
        h.entries_flushed = h.entries_flushed.max(u64::from(live.entry_ok));
        h.fin = Some((live.state, live.kind, live.entry_fail));
    }
    pub fn blacklisted(&mut self, key: usize, line: i32, pc: u32, nrec: u64, reason: &'static str) {
        let h = self.head(key, line, pc);
        if h.black.is_none() {
            h.black = Some((nrec, now_ms(), reason));
        }
    }
}

// ---- sampler ------------------------------------------------------------------------------------------------

const CAP: usize = 1 << 18;
static SAMPLES: [AtomicUsize; CAP] = [const { AtomicUsize::new(0) }; CAP];
static NSAMPLES: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// What a code range belongs to.
#[derive(Clone, Copy, Debug)]
pub enum Owner {
    /// Tier-1 code of the prototype (key, line).
    Tier1(usize, i32),
    /// A trace (number, root prototype key, line, head pc).
    Trace(u32, usize, i32, u32),
}

static RANGES: Mutex<BTreeMap<usize, (usize, Owner)>> = Mutex::new(BTreeMap::new());

/// Is the sampler running? (the registration sites ask)
pub fn sampling() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Registers a code range for the classification (no-op unless the sampler runs).
pub fn note_range(start: usize, size: usize, owner: Owner) {
    if !sampling() {
        return;
    }
    if let Ok(mut m) = RANGES.lock() {
        m.insert(start, (start + size, owner));
    }
}

/// The sampling rate asked for by `RUSTYNAIL_JIT_SAMPLE` (0 = off).
pub fn sample_hz(v: Option<&str>) -> u32 {
    match v.map(str::trim) {
        None | Some("" | "0" | "off") => 0,
        Some("on" | "1") => 1000,
        Some(s) => s.parse::<u32>().unwrap_or(0).min(20000),
    }
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod sig {
    use super::*;

    extern "C" fn handler(_: libc::c_int, _: *mut libc::siginfo_t, uc: *mut libc::c_void) {
        // SAFETY: the kernel passes a valid `ucontext_t` to an `SA_SIGINFO` handler; only atomics are touched.
        let rip = unsafe {
            let uc = uc.cast::<libc::ucontext_t>();
            #[cfg(target_arch = "x86_64")]
            let r = (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] as usize;
            #[cfg(target_arch = "aarch64")]
            let r = (*uc).uc_mcontext.pc as usize;
            r
        };
        let i = NSAMPLES.fetch_add(1, Ordering::Relaxed);
        if i < CAP {
            SAMPLES[i].store(rip, Ordering::Relaxed);
        } else {
            NSAMPLES.store(CAP, Ordering::Relaxed);
        }
    }

    pub fn start(hz: u32) -> bool {
        // SAFETY: plain `sigaction` / `setitimer` calls with zero-initialised structs.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = handler as *const () as usize;
            sa.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
            libc::sigemptyset(&mut sa.sa_mask);
            if libc::sigaction(libc::SIGPROF, &sa, std::ptr::null_mut()) != 0 {
                return false;
            }
            let us = (1_000_000 / hz.max(1)).max(50) as libc::suseconds_t;
            let it = libc::itimerval {
                it_interval: libc::timeval {
                    tv_sec: 0,
                    tv_usec: us,
                },
                it_value: libc::timeval {
                    tv_sec: 0,
                    tv_usec: us,
                },
            };
            libc::setitimer(libc::ITIMER_PROF, &it, std::ptr::null_mut()) == 0
        }
    }

    pub fn stop() {
        // SAFETY: as above.
        unsafe {
            let it: libc::itimerval = std::mem::zeroed();
            libc::setitimer(libc::ITIMER_PROF, &it, std::ptr::null_mut());
            libc::signal(libc::SIGPROF, libc::SIG_IGN);
        }
    }
}

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
mod sig {
    pub fn start(_: u32) -> bool {
        false
    }
    pub fn stop() {}
}

/// Starts the sampler (once per process) at `hz`.
pub fn start_sampler(hz: u32) {
    if hz == 0 || ACTIVE.swap(true, Ordering::Relaxed) {
        return;
    }
    if !sig::start(hz) {
        ACTIVE.store(false, Ordering::Relaxed);
    }
}

/// Samples per owner, from the buffer (stops the timer first).
#[derive(Default, Debug)]
pub struct Profile {
    pub total: u64,
    pub runtime: u64,
    pub tier1: BTreeMap<(usize, i32), u64>,
    pub trace: BTreeMap<(usize, i32, u32), u64>,
    pub traces: BTreeMap<u32, u64>,
}

/// Classifies the samples taken so far (`None`: the sampler was not running).
pub fn profile() -> Option<Profile> {
    if !sampling() {
        return None;
    }
    sig::stop();
    let n = NSAMPLES.load(Ordering::Relaxed).min(CAP);
    let ranges = RANGES.lock().ok()?;
    let mut p = Profile::default();
    for s in &SAMPLES[..n] {
        let a = s.load(Ordering::Relaxed);
        p.total += 1;
        match ranges.range(..=a).next_back() {
            Some((_, (end, o))) if a < *end => match *o {
                Owner::Tier1(k, l) => *p.tier1.entry((k, l)).or_insert(0) += 1,
                Owner::Trace(no, k, l, pc) => {
                    *p.trace.entry((k, l, pc)).or_insert(0) += 1;
                    *p.traces.entry(no).or_insert(0) += 1;
                }
            },
            _ => p.runtime += 1,
        }
    }
    Some(p)
}

fn pct(n: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        n as f64 * 100.0 / total as f64
    }
}

fn counts(m: &BTreeMap<&'static str, u32>) -> String {
    let mut v: Vec<_> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    v.iter()
        .map(|(k, n)| format!("{k}:{n}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// What the report needs to know about a live head (the cache fills it).
pub struct LiveHead {
    pub state: &'static str,
    pub kind: &'static str,
    pub entry_ok: u32,
    pub entry_fail: u32,
}

/// The `[heads]` block. `live(key, line, pc)` gives the state of a head that still exists. A head is listed when it is not `Traced`
/// (or is gone) or had any abort, refusal or flush; with samples the rows are sorted by their share, else by recordings started.
pub fn head_report(
    d: &Diag,
    prof: Option<&Profile>,
    live: &dyn Fn(usize, i32, u32) -> Option<LiveHead>,
) -> String {
    let mut o = String::new();
    struct Row {
        key: (usize, i32, u32),
        text: String,
        weight: u64,
    }
    let mut rows: Vec<Row> = Vec::new();
    let total = prof.map_or(0, |p| p.total);
    for (&(k, l, pc), h) in &d.heads {
        let lv = live(k, l, pc).or_else(|| {
            h.fin.map(|(state, kind, f)| LiveHead {
                state,
                kind,
                entry_ok: 0,
                entry_fail: f,
            })
        });
        let state = lv.as_ref().map_or("gone", |x| x.state);
        let interesting = state != "Traced"
            || !h.aborts.is_empty()
            || !h.side_aborts.is_empty()
            || !h.refused.is_empty()
            || !h.flushes.is_empty();
        let tsamp = prof.and_then(|p| p.trace.get(&(k, l, pc)).copied());
        if !interesting {
            continue;
        }
        let mut t = String::new();
        let _ = write!(
            t,
            "[heads] {} pc {} {} {state}: rec {} inst {}",
            d.name(k, l),
            pc,
            lv.as_ref().map_or("?", |x| x.kind),
            h.started,
            h.installed
        );
        if !h.aborts.is_empty() {
            let _ = write!(t, " aborts {{{}}}", counts(&h.aborts));
        }
        if !h.side_aborts.is_empty() {
            let _ = write!(t, " side-aborts {{{}}}", counts(&h.side_aborts));
        }
        if !h.refused.is_empty() {
            let _ = write!(t, " start-refused {{{}}}", counts(&h.refused));
        }
        if !h.sites.is_empty() {
            let mut v: Vec<_> = h.sites.iter().collect();
            v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let s: Vec<String> = v.iter().take(5).map(|(k, n)| format!("{k}:{n}")).collect();
            let _ = write!(t, " at {{{}}}", s.join(","));
        }
        if !h.flushes.is_empty() {
            let _ = write!(t, " flushes {{{}}}", counts(&h.flushes));
        }
        let live_entries = lv.as_ref().map_or(0, |x| u64::from(x.entry_ok));
        let _ = write!(
            t,
            " entries {} (fail {})",
            h.entries_flushed + live_entries,
            lv.as_ref().map_or(0, |x| x.entry_fail)
        );
        if let Some((n, ms, why)) = h.black {
            let _ = write!(t, " black@rec#{n}/{ms}ms({why})");
        }
        let w = tsamp.unwrap_or(0);
        if prof.is_some() {
            let _ = write!(t, " trace {:.1}%", pct(w, total));
        }
        rows.push(Row {
            key: (k, l, pc),
            text: t,
            weight: if prof.is_some() {
                w
            } else {
                u64::from(h.started)
            },
        });
    }
    rows.sort_by(|a, b| b.weight.cmp(&a.weight).then(a.key.cmp(&b.key)));
    let _ = writeln!(o, "[heads] {} heads with activity", rows.len());
    for r in &rows {
        let _ = writeln!(o, "{}", r.text);
    }
    if let Some(p) = prof {
        let t1: u64 = p.tier1.values().sum();
        let tr: u64 = p.trace.values().sum();
        let _ = writeln!(
            o,
            "[heads] samples {}: tier1 {:.1}% traces {:.1}% runtime (helpers, gc, compile, other) {:.1}%",
            p.total,
            pct(t1, total),
            pct(tr, total),
            pct(p.runtime, total)
        );
        // per prototype: tier-1 share, trace share (all its heads), and the state of its heads
        let mut per: BTreeMap<(usize, i32), (u64, u64)> = BTreeMap::new();
        for (&k, &n) in &p.tier1 {
            per.entry(k).or_default().0 += n;
        }
        for (&(k, l, _), &n) in &p.trace {
            per.entry((k, l)).or_default().1 += n;
        }
        let mut v: Vec<_> = per.into_iter().collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.1 .0 + a.1 .1));
        for ((k, l), (a, b)) in v.into_iter().take(30) {
            let name = d.name(k, l);
            let _ = writeln!(
                o,
                "[heads] proto {name}: tier1 {:.1}% traces {:.1}%",
                pct(a, total),
                pct(b, total)
            );
        }
    }
    o
}
