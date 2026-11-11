//! `math.random` PRNG: xoshiro256** exactly as `lmathlib.c` (64-bit `Rand64` variant), identical in 5.4 and 5.5.
//!
//! Default seeding: the reference seeds at library open with `luai_makeseed()` (5.5: `luaL_makeseed`;
//! 5.4: `time(NULL)` and the address of the `lua_State`). We cannot (and need not) reproduce those:
//! [`make_seed`] mixes the wall-clock nanoseconds, a process-wide counter and the address of a local,
//! which is "random" in the same sense. Explicit seeds (`math.randomseed(n[, m])`) reproduce the
//! reference sequences bit for bit.
#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};

/// State of the generator (`RanState`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rand {
    s: [u64; 4],
}

/// Errors of `math.random`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RandError {
    /// `luaL_error(L, "wrong number of arguments")`.
    WrongArgs,
    /// `luaL_argcheck(low <= up, 1, "interval is empty")`: argument index and message.
    Arg(u8, &'static str),
}

/// Result of `math.random`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RandValue {
    Float(f64),
    Int(i64),
}

static SEED_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A "random" 64-bit seed (see module docs).
pub fn make_seed() -> u64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let local = 0u8;
    let addr = &local as *const u8 as usize as u64;
    let c = SEED_COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    let mut x = t ^ addr.rotate_left(32) ^ c;
    // splitmix64 finalizer
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

impl Rand {
    /// State seeded like `setseed(L, state, n1, n2)` (the pushed return values are `n1`, `n2`).
    pub fn with_seed(n1: u64, n2: u64) -> Rand {
        let mut r = Rand { s: [0; 4] };
        r.set_seed(n1, n2);
        r
    }

    /// Initialised with a random seed, as at library open.
    pub fn new_random() -> Rand {
        Rand::with_seed(make_seed(), 0)
    }

    /// `setseed`: re-seed; returns `(n1, n2)` (what `math.randomseed` returns).
    pub fn set_seed(&mut self, n1: u64, n2: u64) -> (u64, u64) {
        self.s = [n1, 0xff, n2, 0];
        for _ in 0..16 {
            self.next_rand();
        }
        (n1, n2)
    }

    /// `math.randomseed()` with no arguments: new random seed (5.5 also mixes in a state draw).
    pub fn set_random_seed(&mut self) -> (u64, u64) {
        if cfg!(rustynail_lua54) {
            let n1 = make_seed();
            let n2 = make_seed();
            self.set_seed(n1, n2)
        } else {
            let n1 = make_seed();
            let n2 = self.next_rand();
            self.set_seed(n1, n2)
        }
    }

    /// `nextrand`.
    pub fn next_rand(&mut self) -> u64 {
        let state0 = self.s[0];
        let state1 = self.s[1];
        let state2 = self.s[2] ^ state0;
        let state3 = self.s[3] ^ state1;
        let res = state1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        self.s[0] = state0 ^ state3;
        self.s[1] = state1 ^ state2;
        self.s[2] = state2 ^ (state1 << 17);
        self.s[3] = state3.rotate_left(45);
        res
    }

    /// `I2d`: float in `[0, 1)` from the top 53 bits.
    pub fn to_float(x: u64) -> f64 {
        (x >> 11) as f64 * (0.5 / (1u64 << 52) as f64)
    }

    /// `project`: uniform integer in `[0, n]`, starting from `ran`.
    pub fn project(&mut self, mut ran: u64, n: u64) -> u64 {
        let mut lim = n;
        let mut sh = 1u32;
        while lim & lim.wrapping_add(1) != 0 {
            lim |= lim >> sh;
            sh *= 2;
        }
        loop {
            ran &= lim;
            if ran <= n {
                return ran;
            }
            ran = self.next_rand();
        }
    }

    /// `math.random(...)` with `args` being the (already integer-converted) arguments.
    pub fn random(&mut self, args: &[i64]) -> Result<RandValue, RandError> {
        let rv = self.next_rand();
        let (low, up) = match args {
            [] => return Ok(RandValue::Float(Self::to_float(rv))),
            [up] => {
                if *up == 0 {
                    return Ok(RandValue::Int(rv as i64));
                }
                (1, *up)
            }
            [low, up] => (*low, *up),
            _ => return Err(RandError::WrongArgs),
        };
        if low > up {
            return Err(RandError::Arg(1, "interval is empty"));
        }
        let p = self.project(rv, (up as u64).wrapping_sub(low as u64));
        Ok(RandValue::Int(p.wrapping_add(low as u64) as i64))
    }
}

impl Rand {
    /// Raw generator state (for storing it in Lua values).
    pub fn to_state(&self) -> [u64; 4] {
        self.s
    }
    /// Rebuild a generator from [`Rand::to_state`].
    pub fn from_state(s: [u64; 4]) -> Rand {
        Rand { s }
    }
}
