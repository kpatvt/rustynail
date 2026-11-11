//! Load arbitrary bytes as a binary chunk: must never panic or touch memory out of bounds (the
//! chunk is *not* run: executing malformed bytecode is as undefined as in the reference). A
//! chunk that loads must re-dump and re-load to an equal chunk.
#![no_main]
use libfuzzer_sys::fuzz_target;
use rn_compiler::testhost::TestHost;

fuzz_target!(|data: &[u8]| {
    let mut bytes = Vec::with_capacity(data.len() + 1);
    bytes.push(0x1b); // chunks start with ESC
    bytes.extend_from_slice(data);
    let mut host = TestHost::default();
    if let Ok(p) = rn_compiler::undump(&mut host, &bytes, b"=fuzz") {
        let out = rn_compiler::dump(&p, &host, false);
        let mut host2 = TestHost::default();
        let q = rn_compiler::undump(&mut host2, &out, b"=fuzz")
            .expect("a dumped chunk must load again");
        let out2 = rn_compiler::dump(&q, &host2, false);
        assert_eq!(out, out2, "dump(undump(dump(p))) differs");
    }
    // through the full loader (mode checks, headers)
    let mut host = TestHost::default();
    let _ = rn_compiler::load_with_mode_str(&mut host, &bytes, b"=fuzz", Some(b"b"));
    let _ = rn_compiler::load_with_mode_str(&mut host, data, b"=fuzz", None);
});
