//! Section 77.7 programs (`retf` bits 4-7). Kept out of `PROGS`: its `pcall` / `error` case uses a helper the emulator modes of
//! `twins_emulator_modes` do not model (`emuverify: unexpected helper`, also with `retf=3`).

use super::progs::Prog;

pub const S77_PROGS: &[Prog] = &[
    Prog {
        name: "s77_rself",
        small: true,
        // the vm-corpus program, also compared with the reference there
        src: include_str!("../../../../tests/vm/s77_rself.lua"),
    },
    Prog {
        name: "s77_tflat",
        small: true,
        // section 77.14 (`t1g` bit 2): the vm-corpus program, also compared with the reference there
        src: include_str!("../../../../tests/vm/s77_tflat.lua"),
    },
    Prog {
        name: "s77_polyhead",
        small: true,
        // section 77.17 (`polyhead`): the vm-corpus program, also compared with the reference there
        src: include_str!("../../../../tests/vm/s77_polyhead.lua"),
    },
    Prog {
        name: "s82_pfam",
        small: true,
        // section 82.1 (`pfam`): the vm-corpus program, also compared with the reference there
        src: include_str!("../../../../tests/vm/s82_pfam.lua"),
    },
    Prog {
        name: "s86_tailf",
        small: true,
        // section 86.1 (TAILF, `retf` bit 10): the vm-corpus program, also compared with the reference there
        src: include_str!("../../../../tests/vm/s86_tailf.lua"),
    },
    Prog {
        name: "s101_tailf0",
        small: true,
        // section 101.1 (`tailf0`): the vm-corpus program, also compared with the reference there
        src: include_str!("../../../../tests/vm/s101_tailf0.lua"),
    },
];
