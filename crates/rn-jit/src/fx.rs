//! BUNDLE-2: a small, deterministic, non-DoS-resistant hasher (the `FxHash` of rustc) for the compiler's own maps. The recorder, the loop
//! optimiser and the sink pass keep many `HashMap<usize, _>` / `HashSet<u16>` keyed by addresses, refs and slots; `SipHash` (the std default)
//! was 1.5-2.5% of the cycles of the compile-heavy awfy programs (deltablue, cd, richards, raytrace). The keys are never attacker-controlled
//! (they are addresses and indices of the compiler's own data), and no output depends on the iteration order of these maps.

use std::hash::{BuildHasherDefault, Hasher};

/// The `FxHash` mixing function (one multiply-rotate per word).
#[derive(Default, Clone, Copy)]
pub struct FxHasher {
    hash: u64,
}

const SEED: u64 = 0x517c_c1b7_2722_0a95;

impl FxHasher {
    #[inline]
    fn add(&mut self, w: u64) {
        self.hash = (self.hash.rotate_left(5) ^ w).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut b = bytes;
        while b.len() >= 8 {
            self.add(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]));
            b = &b[8..];
        }
        if b.len() >= 4 {
            self.add(u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])));
            b = &b[4..];
        }
        for &x in b {
            self.add(u64::from(x));
        }
    }
    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(u64::from(i));
    }
    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add(u64::from(i));
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

/// `HashMap` with [`FxHasher`] (construct with `default()`).
pub type HashMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<FxHasher>>;
/// `HashSet` with [`FxHasher`] (construct with `default()`).
pub type HashSet<K> = std::collections::HashSet<K, BuildHasherDefault<FxHasher>>;
