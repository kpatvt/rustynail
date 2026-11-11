//! Compile a text file and write the binary chunk (`dumpbin [-s] in.lua out.luac`).
use rn_compiler::testhost::TestHost;
use rn_compiler::{dump, load, skip_file_prefix, LoadMode};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let strip = args.iter().any(|a| a == "-s");
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let src = std::fs::read(files[0]).expect("read");
    let src = skip_file_prefix(&src);
    let mut host = TestHost::default();
    let mode = LoadMode {
        text: true,
        binary: false,
    };
    match load(&mut host, src, format!("@{}", files[0]).as_bytes(), mode) {
        Ok(p) => std::fs::write(files[1], dump(&p, &host, strip)).expect("write"),
        Err(e) => {
            eprintln!("{}", String::from_utf8_lossy(&e.message));
            std::process::exit(1);
        }
    }
}
