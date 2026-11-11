//! rnluac — port of `luac.c` (list / dump / parse-only) over `rn_compiler`.
//!
//! SPDX-License-Identifier: MIT
//! Copyright (c) 2026 Kiran Pashikanti <kpatvt@gmail.com> (derived from Lua, Copyright (C) 1994-2026 Lua.org,
//! PUC-Rio; see LICENSE and LICENSE-LUA).
//!
//! Extensions to `luac.c`: `--version` (like `-v`) and `--license`.
use rn_compiler::listing::{list, ListOpts};
use rn_core::heap::Heap;
use rn_core::protodata::ProtoData;
use rn_core::string::StrRef;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStringExt;
use std::process::exit;

#[path = "license.rs"]
mod license;

const PROGNAME: &str = "luac";
const OUTPUT: &str = "luac.out";

#[cfg(not(rustynail_lua54))]
const COPYRIGHT: &str = "Lua 5.5.1  Copyright (C) 1994-2026 Lua.org, PUC-Rio";
#[cfg(rustynail_lua54)]
const COPYRIGHT: &str = "Lua 5.4.9  Copyright (C) 1994-2026 Lua.org, PUC-Rio";

struct Opts {
    listing: u32,
    dumping: bool,
    stripping: bool,
    output: Option<String>,
    progname: String,
}

fn fatal(progname: &str, msg: &str) -> ! {
    eprintln!("{progname}: {msg}");
    exit(1)
}

fn usage(o: &Opts, message: &str) -> ! {
    let p = &o.progname;
    if message.starts_with('-') {
        eprintln!("{p}: unrecognized option '{message}'");
    } else {
        eprintln!("{p}: {message}");
    }
    eprintln!(
        "usage: {p} [options] [filenames]\n\
Available options are:\n\
\x20 -l       list (use -l -l for full listing)\n\
\x20 -o name  output to file 'name' (default is \"{OUTPUT}\")\n\
\x20 -p       parse only\n\
\x20 -s       strip debug information\n\
\x20 -v       show version information\n\
\x20 --       stop handling options\n\
\x20 -        stop handling options and process stdin"
    );
    exit(1)
}

/// `doargs`: returns the index of the first file argument (and may push `luac.out` for -l/-p alone).
fn doargs(argv: &mut [String], o: &mut Opts) -> usize {
    let mut version = 0;
    if !argv[0].is_empty() {
        o.progname = argv[0].clone();
    }
    let argc = argv.len();
    let mut i = 1;
    while i < argc {
        let a = argv[i].clone();
        if !a.starts_with('-') {
            break;
        } else if a == "--" {
            i += 1;
            if version > 0 {
                version += 1;
            }
            break;
        } else if a == "-" {
            break;
        } else if a == "-l" {
            o.listing += 1;
        } else if a == "-o" {
            i += 1;
            let out = argv.get(i).cloned();
            match out {
                Some(s) if !s.is_empty() && !(s.starts_with('-') && s.len() > 1) => {
                    o.output = if s == "-" { None } else { Some(s) };
                }
                _ => usage(o, "'-o' needs argument"),
            }
        } else if a == "-p" {
            o.dumping = false;
        } else if a == "-s" {
            o.stripping = true;
        } else if a == "-v" || a == "--version" {
            version += 1;
        } else if a == "--license" {
            license::print_license("rnluac");
            exit(0);
        } else {
            usage(o, &a);
        }
        i += 1;
    }
    if i == argc && (o.listing > 0 || !o.dumping) {
        o.dumping = false;
        i -= 1;
        argv[i] = OUTPUT.to_string();
    }
    if version > 0 {
        license::quiet_println(COPYRIGHT);
        license::quiet_println(&format!(
            "rnluac {} (Lua {} compatible bytecode compiler) {}",
            env!("CARGO_PKG_VERSION"),
            license::LUA_SERIES,
            license::AUTHOR_LINE
        ));
        if version == argc - 1 {
            exit(0);
        }
    }
    i
}

fn load_file(heap: &mut Heap, name: &str, progname: &str) -> ProtoData<StrRef> {
    let (data, chunkname): (Vec<u8>, Vec<u8>) = if name == "-" {
        let mut v = Vec::new();
        if let Err(e) = std::io::stdin().read_to_end(&mut v) {
            fatal(progname, &format!("cannot read stdin: {e}"));
        }
        (v, b"=stdin".to_vec())
    } else {
        match std::fs::read(name) {
            Ok(v) => {
                let mut n = b"@".to_vec();
                n.extend_from_slice(name.as_bytes());
                (v, n)
            }
            Err(e) => {
                let msg = match e.raw_os_error() {
                    Some(c) => format!("cannot open {name}: {}", rn_core::auxlib::strerror(c)),
                    None => format!("cannot open {name}"),
                };
                fatal(progname, &msg)
            }
        }
    };
    // luaL_loadfilex: BOM, first-line '#' comment (the newline is kept for line numbers)
    let body = rn_compiler::skip_file_prefix(&data);
    match rn_compiler::load_with_mode_str(heap, body, &chunkname, None) {
        Ok(p) => p,
        Err(e) => fatal(progname, &String::from_utf8_lossy(&e.message)),
    }
}

/// `combine`: several chunks become the nested functions of a synthetic main function.
fn combine(
    heap: &mut Heap,
    mut protos: Vec<ProtoData<StrRef>>,
    progname: &str,
) -> ProtoData<StrRef> {
    if protos.len() == 1 {
        return protos.pop().expect("one proto");
    }
    let src = "(function()end)();\n".repeat(protos.len());
    let mut f = match rn_compiler::load_with_mode_str(heap, src.as_bytes(), b"=(luac)", Some(b"t"))
    {
        Ok(p) => p,
        Err(e) => fatal(progname, &String::from_utf8_lossy(&e.message)),
    };
    for (i, mut p) in protos.into_iter().enumerate() {
        if let Some(u) = p.upvalues.first_mut() {
            u.in_stack = false;
        }
        f.protos[i] = p;
    }
    f
}

fn main() {
    let mut argv: Vec<String> = std::env::args_os()
        .map(|a| String::from_utf8_lossy(&a.into_vec()).into_owned())
        .collect();
    if argv.is_empty() {
        argv.push(PROGNAME.to_string());
    }
    let mut o = Opts {
        listing: 0,
        dumping: true,
        stripping: false,
        output: Some(OUTPUT.to_string()),
        progname: PROGNAME.to_string(),
    };
    let i = doargs(&mut argv, &mut o);
    let files = &argv[i..];
    if files.is_empty() {
        usage(&o, "no input files given");
    }
    let mut heap = match Heap::new(0) {
        Ok(h) => h,
        Err(_) => fatal(&o.progname, "cannot create state: not enough memory"),
    };
    let protos: Vec<_> = files
        .iter()
        .map(|f| load_file(&mut heap, f, &o.progname))
        .collect();
    let f = combine(&mut heap, protos, &o.progname);
    if o.listing > 0 {
        let text = list(
            &f,
            &heap,
            ListOpts {
                full: o.listing > 1,
                normalize_addresses: false,
            },
        );
        let _ = std::io::stdout().write_all(text.as_bytes()); // (a closed pipe is not an error worth a panic)
    }
    if o.dumping {
        let bytes = rn_compiler::dump(&f, &heap, o.stripping);
        match &o.output {
            None => {
                let _ = std::io::stdout().write_all(&bytes);
            }
            Some(path) => {
                if let Err(e) = std::fs::write(path, &bytes) {
                    let c = e
                        .raw_os_error()
                        .map_or(String::new(), rn_core::auxlib::strerror);
                    fatal(&o.progname, &format!("cannot open {path}: {c}"));
                }
            }
        }
    }
}
