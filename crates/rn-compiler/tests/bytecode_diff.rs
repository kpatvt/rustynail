//! Bytecode differential test: our compiler vs the reference `luac` (listing and binary dump).
mod common;
use common::*;
use rn_compiler::listing::{list, ListOpts};
use rn_compiler::testhost::TestHost;
use rn_compiler::{dump, load, skip_file_prefix, LoadMode};
use std::path::Path;
use std::process::Command;

/// Normalize the addresses printed by `%p` in the reference listing.
pub fn normalize(s: &str) -> String {
    let mut out = String::new();
    for line in s.lines() {
        let mut l = line.to_string();
        let hex_tail = |l: &str, marker: &str, end: &str| -> Option<String> {
            let i = l.rfind(marker)?;
            let rest = &l[i + marker.len()..];
            let h = rest.strip_suffix(end)?;
            if !h.is_empty() && h.bytes().all(|c| c.is_ascii_hexdigit()) {
                Some(format!(
                    "{}{}0xX{}",
                    &l[..i],
                    marker.trim_end_matches("0x"),
                    end
                ))
            } else {
                None
            }
        };
        if l.starts_with("constants (") || l.starts_with("locals (") || l.starts_with("upvalues (")
        {
            if let Some(x) = hex_tail(&l, " for 0x", ":") {
                l = x;
            }
        } else if let Some(x) = hex_tail(&l, " at 0x", ")") {
            l = x;
        } else if l.contains("\tCLOSURE") {
            if let Some(x) = hex_tail(&l, "\t; 0x", "") {
                l = x;
            }
        }
        out.push_str(&l);
        out.push('\n');
    }
    out
}

fn first_diff(a: &str, b: &str) -> String {
    for (i, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("line {}:\n  ref : {x}\n  ours: {y}", i + 1);
        }
    }
    format!(
        "line counts differ: ref {} vs ours {}",
        a.lines().count(),
        b.lines().count()
    )
}

fn run_luac(luac: &Path, args: &[&str], file: &Path) -> Option<Vec<u8>> {
    let out = Command::new(luac).args(args).arg(file).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(out.stdout)
}

pub fn compile_file(
    file: &Path,
) -> Result<(TestHost, rn_core::protodata::ProtoData<u32>), String> {
    let src = std::fs::read(file).map_err(|e| e.to_string())?;
    let src = skip_file_prefix(&src);
    let mut host = TestHost::default();
    let name = format!("@{}", file.display());
    let mode = LoadMode {
        text: true,
        binary: false,
    };
    // run on a thread with a modest explicit stack to prove the nesting limit protects it
    let r = std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(1 << 20)
            .spawn_scoped(s, || {
                let r = load(&mut host, src, name.as_bytes(), mode);
                (host, r)
            })
            .map_err(|e| e.to_string())?
            .join()
            .map_err(|_| "compiler panicked".to_string())
    })?;
    let (host, r) = r;
    r.map(|p| (host, p))
        .map_err(|e| String::from_utf8_lossy(&e.message).into_owned())
}

pub fn check_file(luac: &Path, file: &Path) -> Result<(), String> {
    // files the reference rejects must be rejected with the identical message
    let probe = Command::new(luac)
        .arg("-p")
        .arg(file)
        .output()
        .map_err(|e| e.to_string())?;
    if !probe.status.success() {
        let want = String::from_utf8_lossy(&probe.stderr)
            .trim_end()
            .to_string();
        let want = want
            .strip_prefix(&format!("{}: ", luac.display()))
            .unwrap_or(&want)
            .to_string();
        return match compile_file(file) {
            Err(got) if got == want => Ok(()),
            Err(got) => Err(format!(
                "error message differs:\n  ref : {want}\n  ours: {got}"
            )),
            Ok(_) => Err(format!("we accepted a file the reference rejects ({want})")),
        };
    }
    let (host, p) = compile_file(file)?;
    for (flag, full) in [(&["-l", "-l", "-p"][..], true), (&["-l", "-p"][..], false)] {
        let Some(refout) = run_luac(luac, flag, file) else {
            return Err("reference luac rejected the file".into());
        };
        let refout = normalize(&String::from_utf8_lossy(&refout));
        let ours = list(
            &p,
            &host,
            ListOpts {
                full,
                normalize_addresses: true,
            },
        );
        let ours = normalize(&ours);
        if refout != ours {
            return Err(format!(
                "listing (full={full}) differs at {}",
                first_diff(&refout, &ours)
            ));
        }
    }
    for strip in [false, true] {
        let Some(orig) = luac_compile(luac, file, strip) else {
            return Err("luac -o failed".into());
        };
        let ours = dump(&p, &host, strip);
        if ours != orig {
            let at = orig.iter().zip(&ours).position(|(a, b)| a != b);
            return Err(format!(
                "dump (strip={strip}) differs: lens {}/{} first diff at {at:?}",
                orig.len(),
                ours.len()
            ));
        }
    }
    Ok(())
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn bytecode_matches_reference() {
    let Some(luac) = luac() else { return };
    let files = corpus();
    let (mut pass, mut fails) = (0, Vec::new());
    for f in &files {
        match check_file(&luac, f) {
            Ok(()) => pass += 1,
            Err(e) => fails.push(format!("{}: {e}", f.display())),
        }
    }
    eprintln!("bytecode diff: {pass}/{} files identical", files.len());
    for f in &fails {
        eprintln!("FAIL {f}");
    }
    assert!(
        fails.is_empty(),
        "{} of {} files differ",
        fails.len(),
        files.len()
    );
}
