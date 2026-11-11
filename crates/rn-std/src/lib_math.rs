//! Library `math` (port of `lmathlib.c`). The xoshiro256** state lives in the four integer upvalues of the
//! `random` closure (`randomseed` holds that closure in its upvalue), so no userdata is needed.
#![forbid(unsafe_code)]

use rn_core::number::float_in_int_range;
use rn_core::state::State;
use rn_core::status::LuaResult;
use rn_core::value::Value;

use crate::random::Rand;

const PI: f64 = std::f64::consts::PI;

#[inline]
fn push_float(st: &mut State, f: f64) -> LuaResult<usize> {
    st.push(Value::float(f));
    Ok(1)
}

fn math_abs(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    if let Some(n) = v.as_int() {
        st.push(Value::int(n.wrapping_abs()));
        return Ok(1);
    }
    let f = st.check_number(1)?;
    push_float(st, f.abs())
}

macro_rules! float_fn {
    ($name:ident, $f:expr) => {
        fn $name(st: &mut State) -> LuaResult<usize> {
            let x = st.check_number(1)?;
            let f: fn(f64) -> f64 = $f;
            push_float(st, f(x))
        }
    };
}

float_fn!(math_sin, f64::sin);
float_fn!(math_cos, f64::cos);
float_fn!(math_tan, f64::tan);
float_fn!(math_asin, f64::asin);
float_fn!(math_acos, f64::acos);
float_fn!(math_sqrt, f64::sqrt);
float_fn!(math_exp, f64::exp);
float_fn!(math_deg, |x| x * (180.0 / PI));
float_fn!(math_rad, |x| x * (PI / 180.0));
#[cfg(rustynail_lua54)]
float_fn!(math_cosh, f64::cosh);
#[cfg(rustynail_lua54)]
float_fn!(math_sinh, f64::sinh);
#[cfg(rustynail_lua54)]
float_fn!(math_tanh, f64::tanh);
#[cfg(rustynail_lua54)]
float_fn!(math_log10, f64::log10);

fn math_atan(st: &mut State) -> LuaResult<usize> {
    let y = st.check_number(1)?;
    let x = st.opt_number(2, 1.0)?;
    push_float(st, y.atan2(x))
}

#[cfg(rustynail_lua54)]
fn math_pow(st: &mut State) -> LuaResult<usize> {
    let x = st.check_number(1)?;
    let y = st.check_number(2)?;
    push_float(st, x.powf(y))
}

fn math_toint(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    if let Some(n) = State::to_integer(&v) {
        st.push(Value::int(n));
    } else {
        st.check_any(1)?;
        st.push(Value::NIL);
    }
    Ok(1)
}

#[inline]
fn pushnumint(st: &mut State, d: f64) {
    match float_in_int_range(d) {
        Some(n) => st.push(Value::int(n)),
        None => st.push(Value::float(d)),
    }
}

fn math_floor(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    if v.is_int() {
        st.push(v);
    } else {
        let d = st.check_number(1)?.floor();
        pushnumint(st, d);
    }
    Ok(1)
}

fn math_ceil(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    if v.is_int() {
        st.push(v);
    } else {
        let d = st.check_number(1)?.ceil();
        pushnumint(st, d);
    }
    Ok(1)
}

fn math_fmod(st: &mut State) -> LuaResult<usize> {
    let (a, b) = (st.arg(0), st.arg(1));
    if let (Some(x), Some(d)) = (a.as_int(), b.as_int()) {
        if (d as u64).wrapping_add(1) <= 1 {
            st.arg_check(d != 0, 2, b"zero")?;
            st.push(Value::int(0));
        } else {
            st.push(Value::int(x % d));
        }
        Ok(1)
    } else {
        // the reference evaluates the arguments right to left
        let y = st.check_number(2)?;
        let x = st.check_number(1)?;
        push_float(st, x % y)
    }
}

fn math_modf(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    if v.is_int() {
        st.push(v);
        st.push(Value::float(0.0));
    } else {
        let n = st.check_number(1)?;
        let ip = if n < 0.0 { n.ceil() } else { n.floor() };
        pushnumint(st, ip);
        st.push(Value::float(if n == ip { 0.0 } else { n - ip }));
    }
    Ok(2)
}

fn math_ult(st: &mut State) -> LuaResult<usize> {
    let a = st.check_integer(1)?;
    let b = st.check_integer(2)?;
    st.push(Value::boolean((a as u64) < (b as u64)));
    Ok(1)
}

fn math_log(st: &mut State) -> LuaResult<usize> {
    let x = st.check_number(1)?;
    let res = if st.arg_is_none_or_nil(2) {
        x.ln()
    } else {
        let base = st.check_number(2)?;
        if base == 2.0 {
            x.log2()
        } else if base == 10.0 {
            x.log10()
        } else {
            x.ln() / base.ln()
        }
    };
    push_float(st, res)
}

fn frexp(x: f64) -> (f64, i32) {
    if x == 0.0 || !x.is_finite() {
        return (x, 0);
    }
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32;
    if exp == 0 {
        // subnormal: scale up
        let (m, e) = frexp(x * (1u64 << 54) as f64);
        return (m, e - 54);
    }
    let e = exp - 1022;
    let m = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52));
    (m, e)
}

fn ldexp(x: f64, e: i32) -> f64 {
    // exact scaling in steps to avoid intermediate overflow/underflow
    let mut x = x;
    let mut e = e;
    while e > 1000 {
        x *= 2f64.powi(1000);
        e -= 1000;
        if x.is_infinite() {
            return x;
        }
    }
    while e < -1000 {
        x *= 2f64.powi(-1000);
        e += 1000;
        if x == 0.0 {
            return x;
        }
    }
    x * 2f64.powi(e)
}

fn math_frexp(st: &mut State) -> LuaResult<usize> {
    let x = st.check_number(1)?;
    let (m, e) = frexp(x);
    st.push(Value::float(m));
    st.push(Value::int(e as i64));
    Ok(2)
}

fn math_ldexp(st: &mut State) -> LuaResult<usize> {
    let x = st.check_number(1)?;
    let e = st.check_integer(2)? as i32;
    push_float(st, ldexp(x, e))
}

fn math_min(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    st.arg_check(n >= 1, 1, b"value expected")?;
    let mut imin = st.arg(0);
    for i in 1..n {
        let v = st.arg(i);
        if st.compare_lt_le(v, imin, false)? {
            imin = v;
        }
    }
    st.push(imin);
    Ok(1)
}

fn math_max(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    st.arg_check(n >= 1, 1, b"value expected")?;
    let mut imax = st.arg(0);
    for i in 1..n {
        let v = st.arg(i);
        if st.compare_lt_le(imax, v, false)? {
            imax = v;
        }
    }
    st.push(imax);
    Ok(1)
}

fn math_type(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    if st.nargs() >= 1 && v.is_number() {
        st.push_str(if v.is_int() { "integer" } else { "float" })?;
    } else {
        st.check_any(1)?;
        st.push(Value::NIL);
    }
    Ok(1)
}

// ---------------------------------------------------------------------------------------------
// random
// ---------------------------------------------------------------------------------------------

/// The closure (as a value) that holds the generator state: `math.random` itself (the state words are its four integer upvalues, so a
/// call reads and writes four slots of its own closure) and, for `math.randomseed`, the closure in its upvalue 0.
fn rand_closure(st: &mut State, seed_fn: bool) -> Option<Value> {
    let f = st.get(st.ci().r().func);
    let cl = f.as_cclosure()?;
    if seed_fn {
        let v = cl.upvalue(0);
        v.as_cclosure().map(|_| v)
    } else {
        Some(f)
    }
}

fn load_rand(cl: &Value) -> Rand {
    let cl = cl.as_cclosure().expect("random closure");
    let g = |i: usize| cl.upvalue(i).as_int().unwrap_or(0) as u64;
    Rand::from_state([g(0), g(1), g(2), g(3)])
}

/// (Integers are not collectable: no barrier is needed to store them into the closure.)
fn store_rand(st: &mut State, cl: &Value, r: &Rand) {
    let cl = cl.as_cclosure().expect("random closure");
    for (i, w) in r.to_state().iter().enumerate() {
        cl.set_upvalue(st.heap(), i, Value::int(*w as i64));
    }
}

fn math_random(st: &mut State) -> LuaResult<usize> {
    let Some(cl) = rand_closure(st, false) else {
        return Ok(0);
    };
    let mut rng = load_rand(&cl);
    let rv = rng.next_rand(); // the reference draws before looking at the arguments
    let res = random_args(st, &mut rng, rv);
    store_rand(st, &cl, &rng);
    res
}

fn random_args(st: &mut State, rng: &mut Rand, rv: u64) -> LuaResult<usize> {
    let (low, up) = match st.nargs() {
        0 => {
            st.push(Value::float(Rand::to_float(rv)));
            return Ok(1);
        }
        1 => {
            let up = st.check_integer(1)?;
            if up == 0 {
                st.push(Value::int(rv as i64));
                return Ok(1);
            }
            (1, up)
        }
        2 => (st.check_integer(1)?, st.check_integer(2)?),
        _ => return Err(st.l_error(b"wrong number of arguments")),
    };
    st.arg_check(low <= up, 1, b"interval is empty")?;
    let p = rng.project(rv, (up as u64).wrapping_sub(low as u64));
    st.push(Value::int(p.wrapping_add(low as u64) as i64));
    Ok(1)
}

/// F2: the address of `math.random` (a C closure whose four integer upvalues are the generator state).
#[cfg(feature = "jit")]
pub fn jit_random_addr() -> usize {
    math_random as *const () as usize
}

/// F2: `math.random(low, up)` of the J5 `RANDOM` helper: draws from the generator held by the closure `cl` and returns the integer in
/// `low..=up`, or `None` having changed nothing when the interpreter must run the call (an empty interval, which raises; the value `-1`,
/// which the trace's result convention `value + 1 == 0` cannot carry; a closure that is not `math.random`'s).
#[cfg(feature = "jit")]
pub fn jit_random_range(st: &mut State, cl: &Value, low: i64, up: i64) -> Option<i64> {
    let (v, rng) = random_draw(cl, low, up)?;
    store_rand(st, cl, &rng);
    Some(v)
}

/// F2: what the next `math.random(low, up)` of the closure `cl` would return (`None` as [`jit_random_range`]), without drawing: the
/// recorder asks it to keep a draw of -1 out of the recordings.
#[cfg(feature = "jit")]
pub fn jit_random_peek(cl: &Value, low: i64, up: i64) -> Option<i64> {
    random_draw(cl, low, up).map(|d| d.0)
}

/// BUNDLE-H (`libcall`): `math.random()` of the J5 `RANDF` helper: draws from the generator of the closure `cl` and returns the top 53 bits of the
/// draw (`Rand::to_float` is that integer times 2^-53), or `None` having changed nothing for a closure that is not `math.random`'s.
#[cfg(feature = "jit")]
pub fn jit_random_float(st: &mut State, cl: &Value) -> Option<u64> {
    let c = cl.as_cclosure()?;
    if c.f as usize != math_random as *const () as usize || c.nupvalues() != 4 {
        return None;
    }
    let mut rng = load_rand(cl);
    let rv = rng.next_rand();
    store_rand(st, cl, &rng);
    Some(rv >> 11)
}

#[cfg(feature = "jit")]
fn random_draw(cl: &Value, low: i64, up: i64) -> Option<(i64, Rand)> {
    let c = cl.as_cclosure()?;
    if c.f as usize != math_random as *const () as usize || c.nupvalues() != 4 || low > up {
        return None;
    }
    let mut rng = load_rand(cl);
    let rv = rng.next_rand();
    let p = rng.project(rv, (up as u64).wrapping_sub(low as u64));
    let v = p.wrapping_add(low as u64) as i64;
    (v != -1).then_some((v, rng))
}

fn math_randomseed(st: &mut State) -> LuaResult<usize> {
    let Some(cl) = rand_closure(st, true) else {
        return Ok(0);
    };
    let mut rng = load_rand(&cl);
    let (n1, n2) = if st.nargs() == 0 {
        rng.set_random_seed()
    } else {
        let n1 = st.check_integer(1)? as u64;
        let n2 = st.opt_integer(2, 0)? as u64;
        rng.set_seed(n1, n2)
    };
    store_rand(st, &cl, &rng);
    st.push(Value::int(n1 as i64));
    st.push(Value::int(n2 as i64));
    Ok(2)
}

/// `luaopen_math`.
/// Addresses of the functions tier 1.5 regions run inline, in the order of `rn_core::jit_hooks::intr`
/// (`sqrt abs floor ceil min max sin cos exp log`).
#[cfg(feature = "jit")]
pub fn jit_builtins() -> [usize; rn_core::jit_hooks::NUM_INTR] {
    [
        math_sqrt as *const () as usize,
        math_abs as *const () as usize,
        math_floor as *const () as usize,
        math_ceil as *const () as usize,
        math_min as *const () as usize,
        math_max as *const () as usize,
        math_sin as *const () as usize,
        math_cos as *const () as usize,
        math_exp as *const () as usize,
        math_log as *const () as usize,
    ]
}

pub fn open(st: &mut State) -> LuaResult<usize> {
    let lib = st.new_table_sized(0, 40)?;
    st.push(Value::table(lib));
    st.set_funcs(
        lib,
        &[
            ("abs", Some(math_abs)),
            ("acos", Some(math_acos)),
            ("asin", Some(math_asin)),
            ("atan", Some(math_atan)),
            ("ceil", Some(math_ceil)),
            ("cos", Some(math_cos)),
            ("deg", Some(math_deg)),
            ("exp", Some(math_exp)),
            ("tointeger", Some(math_toint)),
            ("floor", Some(math_floor)),
            ("fmod", Some(math_fmod)),
            #[cfg(not(rustynail_lua54))]
            ("frexp", Some(math_frexp)),
            ("ult", Some(math_ult)),
            #[cfg(not(rustynail_lua54))]
            ("ldexp", Some(math_ldexp)),
            ("log", Some(math_log)),
            ("max", Some(math_max)),
            ("min", Some(math_min)),
            ("modf", Some(math_modf)),
            ("rad", Some(math_rad)),
            ("sin", Some(math_sin)),
            ("sqrt", Some(math_sqrt)),
            ("tan", Some(math_tan)),
            ("type", Some(math_type)),
            #[cfg(rustynail_lua54)]
            ("atan2", Some(math_atan)),
            #[cfg(rustynail_lua54)]
            ("cosh", Some(math_cosh)),
            #[cfg(rustynail_lua54)]
            ("sinh", Some(math_sinh)),
            #[cfg(rustynail_lua54)]
            ("tanh", Some(math_tanh)),
            #[cfg(rustynail_lua54)]
            ("pow", Some(math_pow)),
            #[cfg(rustynail_lua54)]
            ("log10", Some(math_log10)),
            #[cfg(rustynail_lua54)]
            ("frexp", Some(math_frexp)),
            #[cfg(rustynail_lua54)]
            ("ldexp", Some(math_ldexp)),
        ],
    )?;
    st.raw_set_str(lib, b"pi", Value::float(PI))?;
    st.raw_set_str(lib, b"huge", Value::float(f64::INFINITY))?;
    st.raw_set_str(lib, b"maxinteger", Value::int(i64::MAX))?;
    st.raw_set_str(lib, b"mininteger", Value::int(i64::MIN))?;

    // random functions: `random` keeps the generator state in its four integer upvalues, `randomseed` holds `random`
    let rng = Rand::new_random();
    let random = st.try_alloc(|h| h.new_cclosure(math_random as rn_core::state::NativeFn, 4))?;
    st.push(Value::cclosure(random));
    store_rand(st, &Value::cclosure(random), &rng);
    st.raw_set_str(lib, b"random", Value::cclosure(random))?;
    let seed = st.try_alloc(|h| h.new_cclosure(math_randomseed as rn_core::state::NativeFn, 1))?;
    st.push(Value::cclosure(seed));
    seed.set_upvalue(st.heap(), 0, Value::cclosure(random));
    st.raw_set_str(lib, b"randomseed", Value::cclosure(seed))?;
    st.pop(); // seed
    st.pop(); // random
    Ok(1)
}

/// P-B: the addresses of `math.type`, `math.fmod` and `math.tointeger`.
#[cfg(feature = "jit")]
pub fn jit_libfns() -> [usize; 3] {
    [
        math_type as *const () as usize,
        math_fmod as *const () as usize,
        math_toint as *const () as usize,
    ]
}
