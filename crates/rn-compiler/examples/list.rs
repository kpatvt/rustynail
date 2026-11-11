//! `cargo run -p rn-compiler --example list -- [-l] file.lua`: print the `luac -l -l`-style listing.
use rn_compiler::listing::{list, ListOpts};
use rn_compiler::testhost::TestHost;
use rn_compiler::{load, LoadMode};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let short = args.iter().any(|a| a == "-l");
    let normalize = args.iter().any(|a| a == "-n");
    let Some(file) = args.iter().find(|a| !a.starts_with('-')) else {
        eprintln!("usage: list [-l] [-n] file");
        std::process::exit(2);
    };
    let mut src = std::fs::read(file).expect("cannot read file");
    if src.first() == Some(&b'#') {
        let nl = src.iter().position(|&c| c == b'\n').unwrap_or(src.len());
        src.drain(..nl);
    }
    let mut host = TestHost::default();
    let mode = LoadMode {
        text: true,
        binary: true,
    };
    let name = format!("@{file}");
    match load(&mut host, &src, name.as_bytes(), mode) {
        Ok(p) => print!(
            "{}",
            list(
                &p,
                &host,
                ListOpts {
                    full: !short,
                    normalize_addresses: normalize
                }
            )
        ),
        Err(e) => {
            eprintln!("{}", String::from_utf8_lossy(&e.message));
            std::process::exit(1);
        }
    }
}
