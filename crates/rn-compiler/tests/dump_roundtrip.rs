//! Undump/dump differential against the reference `luac`, plus fuzz-lite robustness.
mod common;
use common::*;
use rn_compiler::testhost::TestHost;
use rn_compiler::{dump, undump};
use rn_core::instruction::Instruction;
use rn_core::protodata::ProtoData;
use rn_core::ver::active::{OpCode, OP_NAMES};
use std::process::Command;

fn check_codes(p: &ProtoData<u32>) {
    assert!(!p.code.is_empty());
    for &w in &p.code {
        assert!(
            Instruction(w).opcode().is_some(),
            "invalid opcode in {w:#x}"
        );
    }
    let last = Instruction(*p.code.last().unwrap()).opcode().unwrap();
    assert!(matches!(
        last,
        OpCode::Return | OpCode::Return0 | OpCode::Return1
    ));
    p.protos.iter().for_each(check_codes);
}

fn names(p: &ProtoData<u32>, out: &mut Vec<&'static str>) {
    for &w in &p.code {
        out.push(OP_NAMES[Instruction(w).opcode().unwrap() as usize]);
    }
    for c in &p.protos {
        names(c, out);
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn roundtrip_byte_identical() {
    let Some(luac) = luac() else { return };
    let (mut n, mut skipped) = (0, 0);
    for f in corpus() {
        for strip in [false, true] {
            let Some(orig) = luac_compile(&luac, &f, strip) else {
                skipped += 1;
                continue;
            };
            let mut host = TestHost::default();
            let p = undump(&mut host, &orig, b"=test").unwrap_or_else(|e| {
                panic!("{}: {}", f.display(), String::from_utf8_lossy(&e.message))
            });
            check_codes(&p);
            let again = dump(&p, &host, strip);
            if again != orig {
                panic!("{} (strip={strip}) not byte-identical", f.display());
            }
            n += 1;
        }
    }
    eprintln!("roundtrip: {n} chunks identical, {skipped} rejected by luac");
    assert!(n > 20);
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn opcodes_match_luac_listing() {
    let Some(luac) = luac() else { return };
    for f in corpus().into_iter().filter(|f| {
        f.starts_with(root().join("tests/lua"))
            || f.ends_with("code.lua")
            || f.ends_with("calls.lua")
    }) {
        let Some(bin) = luac_compile(&luac, &f, false) else {
            continue;
        };
        let out = Command::new(&luac)
            .args(["-l", "-l", "-p"])
            .arg(&f)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let mut want: Vec<String> = Vec::new();
        for line in text.lines() {
            let t: Vec<&str> = line.split_whitespace().collect();
            if t.len() >= 3
                && t[0].bytes().all(|b| b.is_ascii_digit())
                && t[1].starts_with('[')
                && t[2]
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                want.push(t[2].to_string());
            }
        }
        let mut host = TestHost::default();
        let p = undump(&mut host, &bin, b"=t").unwrap();
        let mut got = Vec::new();
        names(&p, &mut got);
        assert_eq!(got, want, "{}", f.display());
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn fuzz_lite_never_panics() {
    let Some(luac) = luac() else { return };
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut cases = 0u32;
    let files: Vec<_> = corpus()
        .into_iter()
        .filter(|f| {
            std::fs::metadata(f)
                .map(|m| m.len() < 20_000)
                .unwrap_or(false)
        })
        .collect();
    for f in files {
        for strip in [false, true] {
            let Some(orig) = luac_compile(&luac, &f, strip) else {
                continue;
            };
            // every truncation must fail cleanly
            let step = (orig.len() / 400).max(1);
            for cut in (0..orig.len()).step_by(step) {
                let mut h = TestHost::default();
                assert!(
                    undump(&mut h, &orig[..cut], b"=t").is_err(),
                    "{} cut {cut}",
                    f.display()
                );
                cases += 1;
            }
            // random byte flips: may succeed or fail, must not panic/hang
            for _ in 0..300 {
                let mut b = orig.clone();
                for _ in 0..1 + rng.next() % 4 {
                    let i = (rng.next() as usize) % b.len();
                    b[i] = rng.next() as u8;
                }
                let mut h = TestHost::default();
                let _ = undump(&mut h, &b, b"=t");
                cases += 1;
            }
        }
    }
    eprintln!("fuzz-lite: {cases} cases");
    assert!(cases > 1000);
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn hostile_sizes_fail_fast() {
    // valid header, then absurd counts
    let Some(luac) = luac() else { return };
    let f = root().join("tests/lua/basic.lua");
    let orig = luac_compile(&luac, &f, true).unwrap();
    // 5.5 header = 4+1+1+6+1+4+1+4+1+8+1+8 = 40, 5.4 header = 4+1+1+6+1+1+1+8+8 = 31; then nupvalues byte
    let hdr = if cfg!(rustynail_lua54) { 32 } else { 41 };
    let mut b = orig[..hdr].to_vec();
    // linedefined=0 lastline=0 numparams flag maxstack, code size = 2^28 varint
    // (5.4: source string first; sizes end with the 0x80-marked byte)
    #[cfg(not(rustynail_lua54))]
    b.extend_from_slice(&[0, 0, 0, 0, 2, 0x81, 0x80, 0x80, 0x80, 0x00]);
    #[cfg(rustynail_lua54)]
    b.extend_from_slice(&[0x80, 0x80, 0x80, 0, 0, 2, 1, 0, 0, 0, 0x80]);
    let mut h = TestHost::default();
    let e = undump(&mut h, &b, b"=t").unwrap_err();
    assert_eq!(
        String::from_utf8_lossy(&e.message),
        "t: bad binary format (truncated chunk)"
    );
    // deeply nested prototypes: n=1 protos at every level
    let mut b = orig[..hdr].to_vec();
    for _ in 0..5000 {
        // lines, params, flag, stack, code(0 + no padding assumed), k(0), upvalues(0), protos(1)
        #[cfg(not(rustynail_lua54))]
        b.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        // 5.4: source(none), lines, params, flag, stack, code(0), k(0), upvalues(0), protos(1)
        #[cfg(rustynail_lua54)]
        b.extend_from_slice(&[0x80, 0x80, 0x80, 0, 0, 0, 0x80, 0x80, 0x80, 0x81]);
    }
    let mut h = TestHost::default();
    assert!(undump(&mut h, &b, b"=t").is_err());
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn header_errors_match_reference() {
    let Some(lua) = lua() else { return };
    let Some(luac) = luac() else { return };
    let f = root().join("tests/lua/basic.lua");
    let orig = luac_compile(&luac, &f, true).unwrap();
    // mutate header positions and compare messages with `load` in the reference interpreter
    #[cfg(not(rustynail_lua54))]
    let cases: Vec<(usize, u8)> = vec![
        (1, b'X'), // not a binary chunk
        (4, 0x54), // version mismatch
        (5, 1),    // format mismatch
        (6, 0),    // corrupted chunk
        (12, 8),   // int size
        (13, 0),   // int format
        (17, 8),   // instruction size
        (18, 0),   // instruction format
        (22, 4),   // integer size
        (23, 1),   // integer format
        (31, 4),   // number size
        (32, 1),   // number format
    ];
    #[cfg(rustynail_lua54)]
    let cases: Vec<(usize, u8)> = vec![
        (1, b'X'),  // not a binary chunk
        (4, 0x55),  // version mismatch
        (5, 1),     // format mismatch
        (6, 0),     // corrupted chunk
        (12, 8),    // Instruction size
        (13, 4),    // lua_Integer size
        (14, 4),    // lua_Number size
        (15, 1),    // integer format
        (23, 1),    // float format
        (30, 0x7f), // float format (high byte)
    ];
    for (pos, val) in cases {
        let mut b = orig.clone();
        b[pos] = val;
        let mut h = TestHost::default();
        let ours = undump(&mut h, &b, b"=?").unwrap_err();
        let tmp = std::env::temp_dir().join(format!("rn-a1-hdr-{}-{pos}.luac", std::process::id()));
        std::fs::write(&tmp, &b).unwrap();
        let script = format!(
            "local f = io.open({:?}, 'rb'); local s = f:read('a'); f:close(); print(select(2, load(s, '=?', 'b')))",
            tmp.to_str().unwrap()
        );
        let out = Command::new(&lua).arg("-e").arg(script).output().unwrap();
        let _ = std::fs::remove_file(&tmp);
        let want = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
        assert_eq!(String::from_utf8_lossy(&ours.message), want, "pos {pos}");
    }
}

#[test]
fn load_mode_and_stub() {
    use rn_compiler::{load, LoadMode};
    let mut h = TestHost::default();
    let t = LoadMode {
        text: true,
        binary: false,
    };
    let e = load(&mut h, b"\x1bLua", b"=x", t).unwrap_err();
    assert_eq!(e.message, b"attempt to load a binary chunk (mode is 't')");
    let bmode = LoadMode {
        text: false,
        binary: true,
    };
    let e = load(&mut h, b"return 1", b"=x", bmode).unwrap_err();
    assert_eq!(e.message, b"attempt to load a text chunk (mode is 'b')");
    let both = LoadMode {
        text: true,
        binary: true,
    };
    assert!(load(&mut h, b"return 1", b"=x", both).is_ok());
    let e = load(&mut h, b"\x1bLua", b"=x", both).unwrap_err();
    assert_eq!(e.message, b"x: bad binary format (truncated chunk)");
}
