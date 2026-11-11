//! `math` builtins that regions run inline (task T6; docs/JIT_TIER15.md section 20).
//!
//! A `CALL` site is an *intrinsic site* when the warm-up profile saw a light C function that is one of the
//! registered `math` functions (`rn_core::jit_hooks::set_intrinsics`). The analysis classifies such a call as
//! [`super::InsClass::Intr`]; the code generator emits the inline form when the facts at the call allow it
//! ([`result_set`]) and falls back to the tier-1 `CALL` template (the *slow path*) at run time whenever one of
//! the guards fails (callee identity, hooks, interrupt, stack room, spare `CallInfo`, representability of the
//! result). Everything here is pure.

use super::TagSet;
use rn_core::jit_hooks::intr as id;

/// An inlinable `math` function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Intr {
    /// `math.sqrt(x)`
    Sqrt,
    /// `math.abs(x)`
    Abs,
    /// `math.floor(x)`
    Floor,
    /// `math.ceil(x)`
    Ceil,
    /// `math.min(a, b)`
    Min,
    /// `math.max(a, b)`
    Max,
    /// `math.sin(x)` (libm through a helper)
    Sin,
    /// `math.cos(x)` (libm through a helper)
    Cos,
    /// `math.exp(x)` (libm through a helper)
    Exp,
    /// `math.log(x)` with one argument (libm through a helper)
    Log,
}

impl Intr {
    /// From the index of `rn_core::jit_hooks::intr`.
    pub fn from_index(i: u8) -> Option<Intr> {
        Some(match i {
            id::SQRT => Intr::Sqrt,
            id::ABS => Intr::Abs,
            id::FLOOR => Intr::Floor,
            id::CEIL => Intr::Ceil,
            id::MIN => Intr::Min,
            id::MAX => Intr::Max,
            id::SIN => Intr::Sin,
            id::COS => Intr::Cos,
            id::EXP => Intr::Exp,
            id::LOG => Intr::Log,
            _ => return None,
        })
    }

    /// The index of `rn_core::jit_hooks::intr`.
    pub fn index(self) -> u8 {
        match self {
            Intr::Sqrt => id::SQRT,
            Intr::Abs => id::ABS,
            Intr::Floor => id::FLOOR,
            Intr::Ceil => id::CEIL,
            Intr::Min => id::MIN,
            Intr::Max => id::MAX,
            Intr::Sin => id::SIN,
            Intr::Cos => id::COS,
            Intr::Exp => id::EXP,
            Intr::Log => id::LOG,
        }
    }

    /// Number of arguments the inline form takes (the call's `B` must be `nargs + 1`).
    pub fn nargs(self) -> u32 {
        match self {
            Intr::Min | Intr::Max => 2,
            _ => 1,
        }
    }

    /// Computed by a libm helper (the registers are flushed around it) instead of inline instructions.
    pub fn is_libm(self) -> bool {
        matches!(self, Intr::Sin | Intr::Cos | Intr::Exp | Intr::Log)
    }
}

/// An intrinsic call site: the pc of the `CALL` and the function it was seen to call.
pub type Site = (u32, Intr);

/// The static result type of the inline form of `kind` for argument facts `args` (`INT` / `FLT` singletons are
/// the only inlinable operands) and the speculated result `spec` (the profile byte of the call; only
/// `floor`/`ceil` of a float use it). `None`: the call stays an ordinary call.
pub fn result_set(kind: Intr, args: &[TagSet], spec: TagSet) -> Option<TagSet> {
    if args.len() != kind.nargs() as usize {
        return None;
    }
    let num = |t: TagSet| t == TagSet::INT || t == TagSet::FLT;
    if !args.iter().all(|&t| num(t)) {
        return None;
    }
    let a = args[0];
    match kind {
        Intr::Sqrt | Intr::Sin | Intr::Cos | Intr::Exp | Intr::Log => Some(TagSet::FLT),
        Intr::Abs => Some(a),
        Intr::Floor | Intr::Ceil => {
            // (an integer is its own floor; a float needs the profile to say the result is an integer)
            (a == TagSet::INT || spec == TagSet::INT).then_some(TagSet::INT)
        }
        Intr::Min | Intr::Max => (args[1] == a).then_some(a),
    }
}
