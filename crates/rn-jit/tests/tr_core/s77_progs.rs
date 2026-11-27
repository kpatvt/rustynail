//! Section 77.7 programs (`retf` bits 4-7). Kept out of `PROGS`: its `pcall` / `error` case uses a helper the emulator modes of
//! `twins_emulator_modes` do not model (`emuverify: unexpected helper`, also with `retf=3`).

use super::progs::Prog;

pub const S77_PROGS: &[Prog] = &[Prog {
    name: "s77_rself",
    small: true,
    // the vm-corpus program, also compared with the reference there
    src: include_str!("../../../../tests/vm/s77_rself.lua"),
}];
