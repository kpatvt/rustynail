//! BUNDLE-J / J2 "hash-D": the lazy PUC hash of short strings (rn-core `string.rs`: a new short string stores
//! `fast_hash(bytes) ^ seed` with `extra == 0`; the `luaS_hash` value is computed when the string first becomes a table key).
//! In-process tests of the three things the scheme must keep: (1) the `pairs` order of run-time created keys equals the
//! reference interpreter's (checked in as `tests/data/hashd_pairs_order.expected`, produced by `ref/55/build-seed0/refrun`
//! on `tests/vm_seed0/lazy_hash_pairs_order.lua`, seed 0), (2) the promoted hash equals `lua_hash(bytes, seed)` for a
//! nonzero seed, (3) the string table stays consistent (interning, removal at sweep, shrink / grow rehash) with promoted and
//! unpromoted strings mixed.
//!
//! Detectors of the mutants `promote_skip`, `strtab_idx_raw`, `chain_cmp_hash_only`, `promote_seed0` (and, with the program
//! `hashd_absent_then_insert` of `tr_core`, `record_mainpos_raw`).
#![cfg(feature = "trace")]
#![cfg_attr(rustynail_lua54, allow(dead_code))] // every test is Lua 5.5 only: the helpers are unused on 5.4

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_core::string::lua_hash;

const SCRIPT: &str = include_str!("../../../tests/vm_seed0/lazy_hash_pairs_order.lua");
#[cfg(not(rustynail_lua54))]
const EXPECTED: &str = include_str!("data/hashd_pairs_order.expected");

const PRE: &str = "local __buf = {}\n\
local function print(...) local n = select('#', ...) local t = {} \
for i = 1, n do t[i] = tostring((select(i, ...))) end __buf[#__buf + 1] = table.concat(t, '\\t') end\n";
const POST: &str = "\nreturn table.concat(__buf, '\\n') .. '\\n'";

fn state(seed: u32) -> LuaState {
    let mut st = rn_std::new_state(seed).expect("state");
    rn_std::open_libs(&mut st).expect("libs");
    #[cfg(feature = "tr_mutants")]
    rn_jit::tr::mutants::apply_vm_mutants();
    st
}

fn run(st: &mut LuaState, src: &str) -> String {
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=hashd", None),
        Status::Ok,
        "load"
    );
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    assert_eq!(status, Status::Ok, "script failed: {v:?}");
    let s = String::from_utf8_lossy(v.as_string().expect("string result").as_bytes()).into_owned();
    st.set_top(base);
    s
}

#[cfg(not(rustynail_lua54))]
#[test]
fn hashd_pairs_order_matches_the_reference() {
    let mut st = state(0);
    let out = run(&mut st, &format!("{PRE}{SCRIPT}{POST}"));
    let (got, want): (Vec<&str>, Vec<&str>) = (out.lines().collect(), EXPECTED.lines().collect());
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "output line {}", i + 1);
    }
    assert_eq!(got.len(), want.len());
}

/// The PUC hash of a run-time created short string, with a nonzero seed: `hash()` must equal `lua_hash(bytes, seed)`.
#[test]
fn hashd_promoted_hash_is_the_puc_hash() {
    for seed in [0u32, 0xA5A5_1234, 0xFFFF_FFFF, 1] {
        let mut st = state(seed);
        for len in 0..=40usize {
            for variant in 0..3u8 {
                let bytes: Vec<u8> = (0..len)
                    .map(|i| {
                        (i as u8)
                            .wrapping_mul(37)
                            .wrapping_add(variant)
                            .wrapping_add(len as u8)
                    })
                    .collect();
                let s = st.new_str(&bytes).expect("string");
                assert_eq!(
                    s.get().hash(),
                    lua_hash(&bytes, seed),
                    "seed {seed:#x} len {len} variant {variant}"
                );
                // the second request returns the cached value
                assert_eq!(s.get().hash(), lua_hash(&bytes, seed));
            }
        }
    }
}

/// Interning, removal at sweep, shrink / grow of the string table with promoted and unpromoted strings mixed, nonzero seed.
#[test]
fn hashd_string_table_stays_consistent() {
    const CHECK: &str = r#"
local function mk(i) return "s" .. i .. "_" .. (i * 7919 % 1000) end
local keep, t = {}, {}
-- grow the string table, promote every 3rd string by using it as a key
for i = 1, 6000 do
  local s = mk(i)
  if i % 3 == 0 then t[s] = i; keep[#keep + 1] = s end
end
-- equal strings made again are the same object (interning finds promoted and unpromoted strings alike)
local bad = 0
for i = 1, 6000 do
  local a, b = mk(i), mk(i)
  if not rawequal(a, b) then bad = bad + 1 end
  if i % 3 == 0 and t[a] ~= i then bad = bad + 1000000 end
end
-- sweep the garbage (removes strings from the table, shrinks it), then check again
collectgarbage(); collectgarbage()
for i = 1, 6000 do
  local a, b = mk(i), ("s" .. i .. "_") .. (i * 7919 % 1000)
  if not rawequal(a, b) then bad = bad + 1 end
  if i % 3 == 0 and t[a] ~= i then bad = bad + 1000000 end
  if i % 3 ~= 0 and t[a] ~= nil then bad = bad + 1000000 end
end
for i, s in ipairs(keep) do if t[s] ~= i * 3 then bad = bad + 1000000 end end
local n = 0
for k, v in pairs(t) do n = n + 1 end
-- churn: many dead strings, promoted and not
for round = 1, 4 do
  for i = 1, 20000 do local x = "churn" .. round .. "_" .. i; if i % 5 == 0 then t[x] = true end end
  for k in pairs(t) do if type(k) == "string" and k:sub(1, 5) == "churn" then t[k] = nil end end
  collectgarbage()
end
for i = 1, 6000 do
  local a = mk(i)
  if i % 3 == 0 and t[a] ~= i then bad = bad + 1000000 end
end
return bad .. " " .. n
"#;
    for seed in [0u32, 0xDEAD_BEEF] {
        let mut st = state(seed);
        let out = run(&mut st, CHECK);
        assert_eq!(out, "0 2000", "seed {seed:#x}");
    }
}

/// BUNDLE-M: the string table grows past the `1 << TAG_BITS` (2^19) buckets of the strtab tag, where promoted strings are re-indexed from the bytes
/// again, and shrinks back: every string is still found (interned equal strings are the same object) and keys still work. About 40 MB.
/// Detector of `strtag_wide` (and `strtag_zero` / `strtag_shift` at any size).
#[test]
fn hashd_string_table_beyond_the_tag() {
    const CHECK: &str = r#"
local N = 530000
local t, s = {}, {}
for i = 1, N do
  local k = "k" .. i
  s[i] = k
  if i % 2 == 0 then t[k] = i end      -- promoted
end
local bad = 0
for i = 1, N, 7 do
  local a = "k" .. i
  if not rawequal(a, s[i]) then bad = bad + 1 end
  if i % 2 == 0 and t[a] ~= i then bad = bad + 1000000 end
  if i % 2 == 1 and t[a] ~= nil then bad = bad + 1000000 end
end
s = nil
for i = 2, N, 2 do t["k" .. i] = nil end
t = nil
collectgarbage(); collectgarbage()
local u = {}
for i = 1, 1000 do u["k" .. i] = i end
for i = 1, 1000, 3 do if u["k" .. i] ~= i then bad = bad + 1 end end
return bad .. ""
"#;
    for seed in [0u32, 0x1234_5678] {
        let mut st = state(seed);
        assert_eq!(run(&mut st, CHECK), "0", "seed {seed:#x}");
    }
}

/// The recorder's main-position queries (`short_str_mainpos`, the absent-key proof) of a string that is not a key anywhere yet must
/// use the PUC hash (the string is promoted by the query): where the key will be inserted is where the proof looks. (Compiler
/// constants are always promoted -- the parser keys its constant cache by them -- so only run-time strings reach this.)
/// Detector of `record_mainpos_raw`.
#[test]
fn hashd_recorder_mainpos_uses_the_puc_hash() {
    for seed in [0u32, 0xC0FF_EE11] {
        let mut st = state(seed);
        let base = st.top();
        assert_eq!(
            st.load_buffer(
                b"return {a = 1, b = 2, c = 3, d = 4, e = 5, f = 6}",
                b"=t",
                None
            ),
            Status::Ok
        );
        let func = st.top() - 1;
        assert_eq!(st.pcall(func, LUA_MULTRET, 0), Status::Ok);
        let tv = st.get(func);
        let tbl = tv.as_table().expect("table");
        let size = 1usize << tbl.lsizenode();
        for i in 0..64 {
            let bytes = format!("fresh_runtime_key_{i}").into_bytes();
            let key = st.new_str(&bytes).expect("string");
            let want = (lua_hash(&bytes, seed) as usize) & (size - 1);
            // (hashpow2 is `hash & (size - 1)` for sizes that are powers of two, which node vectors are)
            assert_eq!(tbl.short_str_mainpos(key), want, "seed {seed:#x} key {i}");
        }
        st.set_top(base);
    }
}

/// BUNDLE-P P3: the string-table hash `fast_hash` (byte loads for 4..7 bytes) equals the plain-load J2 algorithm for every length 0..=64, on fixed and random
/// contents at every alignment, and `short_key_hash` is `fast_hash ^ seed`. Detector of the mutant `bytetail_hi_byte_skip`.
#[test]
fn bp_fast_hash_equals_reference() {
    #[cfg(feature = "tr_mutants")]
    rn_jit::tr::mutants::apply_vm_mutants();
    fn mix(a: u64, b: u64) -> u64 {
        let r = (a as u128) * (b as u128);
        (r as u64) ^ ((r >> 64) as u64)
    }
    fn reference(s: &[u8]) -> u32 {
        const K: u64 = 0x9E37_79B9_7F4A_7C15;
        let n = s.len();
        let mut h = (n as u64).wrapping_mul(K);
        let last: u64;
        let rd64 = |i: usize| u64::from_le_bytes(s[i..i + 8].try_into().unwrap());
        let rd32 = |i: usize| u64::from(u32::from_le_bytes(s[i..i + 4].try_into().unwrap()));
        if n >= 8 {
            let mut i = 0;
            while i + 8 < n {
                h = mix(h ^ rd64(i), K);
                i += 8;
            }
            let w = rd64(n - 8);
            last = w >> 56;
            h = mix(h ^ (w & 0x00FF_FFFF_FFFF_FFFF), K);
        } else if n >= 4 {
            let (a, b) = (rd32(0), rd32(n - 4));
            last = b >> 24;
            h = mix(h ^ (a | ((b & 0xFF_FFFF) << 32)), K);
        } else if n > 0 {
            let (a, b) = (u64::from(s[0]), u64::from(s[n / 2]));
            last = u64::from(s[n - 1]);
            h = mix(h ^ (a | (b << 8)), K);
        } else {
            last = 0;
        }
        ((h ^ (h >> 32)) as u32).wrapping_add(last as u32)
    }
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    let mut rnd = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let mut buf = [0u8; 64 + 8];
    for round in 0..100 {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = match round {
                0 => 0,
                1 => 0xFF,
                2 => i as u8,
                _ => rnd() as u8,
            };
        }
        for off in 0..8 {
            for n in 0..=64usize {
                let s = &buf[off..off + n];
                let want = reference(s);
                assert_eq!(
                    rn_core::string::fast_hash(s),
                    want,
                    "len {n} off {off} round {round}"
                );
                assert_eq!(
                    rn_core::string::fast_hash_fresh(s),
                    want,
                    "fresh: len {n} off {off} round {round}"
                );
                for seed in [0u32, 1, 0x9E37_79B9, 0xFFFF_FFFF, rnd() as u32] {
                    assert_eq!(rn_core::string::short_key_hash(s, seed), want ^ seed);
                    assert_eq!(rn_core::string::short_key_hash_fresh(s, seed), want ^ seed);
                }
            }
        }
    }
}
