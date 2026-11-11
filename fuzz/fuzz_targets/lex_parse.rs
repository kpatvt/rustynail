//! Compile arbitrary text: must never panic, hang or read out of bounds (errors are fine).
#![no_main]
use libfuzzer_sys::fuzz_target;
use rn_compiler::testhost::TestHost;

fuzz_target!(|data: &[u8]| {
    let mut host = TestHost::default();
    let _ = rn_compiler::load_with_mode_str(&mut host, data, b"=fuzz", Some(b"t"));
    // also with a shebang-skipped prefix, as `luaL_loadfile` does
    let mut host = TestHost::default();
    let _ = rn_compiler::load_with_mode_str(&mut host, rn_compiler::skip_file_prefix(data), b"@f.lua", Some(b"t"));
});
