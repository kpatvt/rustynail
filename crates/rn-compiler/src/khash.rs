//! B3-5: multiplicative (Fx-style) hasher for the compiler's own maps (constant caches, the lexer's long-string table, the dump's
//! string map). The keys are small integers, bit patterns, pointers and the compiler's own strings, where SipHash's DoS resistance buys
//! nothing, and no output depends on the iteration order of these maps.

#[derive(Default, Clone, Copy)]
pub(crate) struct KHasher(u64);

impl KHasher {
    #[inline]
    fn add(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

impl std::hash::Hasher for KHasher {
    #[inline]
    fn finish(&self) -> u64 {
        // the well-mixed high bits of the product end up in the bits that select the bucket
        self.0.rotate_left(26)
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut b = bytes;
        while b.len() >= 8 {
            self.add(u64::from_le_bytes([
                b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
            ]));
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
    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }
    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }
    #[inline]
    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }
    #[inline]
    fn write_isize(&mut self, n: isize) {
        self.add(n as u64);
    }
    #[inline]
    fn write_i64(&mut self, n: i64) {
        self.add(n as u64);
    }
}

pub(crate) type KBuild = std::hash::BuildHasherDefault<KHasher>;
pub(crate) type KMap<K, V> = std::collections::HashMap<K, V, KBuild>;
