//! `lua.h`, part 3: calls, loading, coroutines, garbage collector and miscellaneous functions.
use crate::glue::*;
use crate::types::*;
use rn_core::state::{StkId, CIST_TBC};
use rn_core::status::Status;
use rn_core::value::Value;
use std::ffi::{c_char, c_int, c_void};

// ---- calls ------------------------------------------------------------------------------------

lua_api!(lua_callk(st, nargs: c_int, nresults: c_int, ctx: lua_KContext, k: lua_KFunction) {
    let func = st.top() - (nargs as usize + 1);
    let kf = set_k(&mut st, k);
    let r = st.call_k(func, nresults, ctx, kf);
    ck(&mut st, r);
});

lua_api!(lua_pcallk(
    st, nargs: c_int, nresults: c_int, errfunc: c_int, ctx: lua_KContext, k: lua_KFunction
) -> c_int {
    let func = st.top() - (nargs as usize + 1);
    let ef = if errfunc == 0 { 0 } else { stack_slot(&st, errfunc).0 };
    let kf = set_k(&mut st, k);
    let r = st.pcall_k(func, nresults, ef, ctx, kf);
    ck(&mut st, r) as c_int
});

// ---- load / dump -------------------------------------------------------------------------------

lua_api!(lua_load(
    st, reader: lua_Reader, data: *mut c_void, chunkname: *const c_char, mode: *const c_char
) -> c_int {
    let reader = reader.expect("lua_load: null reader");
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let mut sz: usize = 0;
        let p = reader(lptr(&st), data, &mut sz);
        if p.is_null() || sz == 0 {
            break;
        }
        buf.extend_from_slice(std::slice::from_raw_parts(p.cast::<u8>(), sz));
    }
    let name: &[u8] = cstr_opt(chunkname).unwrap_or(b"?");
    let status = st.load_chunk(&buf, name, cstr_opt(mode));
    status as c_int
});

lua_api!(lua_dump(st, writer: lua_Writer, data: *mut c_void, strip: c_int) -> c_int {
    let f = st.get(st.top() - 1);
    match f.as_lclosure() {
        Some(cl) => {
            let bytes = rn_std::lib_string::dump_closure(&st, cl, strip != 0);
            match writer {
                Some(w) => w(lptr(&st), bytes.as_ptr().cast(), bytes.len(), data),
                None => 1,
            }
        }
        None => 1,
    }
});

// ---- coroutines --------------------------------------------------------------------------------

lua_api!(lua_yieldk(st, nresults: c_int, ctx: lua_KContext, k: lua_KFunction) -> c_int {
    let kf = set_k(&mut st, k);
    match st.yield_k(nresults as usize, ctx, kf) {
        Ok(n) => n as c_int, // inside a hook: returns normally
        Err(s) => throw(&mut st, s),
    }
});

lua_api!(lua_resume(st, from: *mut lua_State, narg: c_int, nres: *mut c_int) -> c_int {
    let f = if from.is_null() { None } else { Some(state(from)) };
    // like the reference, `*nres` is not written when the resume is refused up front
    let refused = match st.status() {
        Status::Ok => {
            st.ci() != st.base_ci() || st.top() - (st.ci().r().func + 1) == narg as usize
        }
        Status::Yield => false,
        _ => true,
    };
    let (status, n) = st.resume(f.as_ref(), narg as usize);
    if !nres.is_null() && !refused {
        *nres = n as c_int;
    }
    status as c_int
});

lua_api!(lua_status(st) -> c_int { st.status() as c_int });
lua_api!(lua_isyieldable(st) -> c_int { st.yieldable() as c_int });

// ---- warnings ----------------------------------------------------------------------------------

lua_api!(lua_setwarnf(st, f: lua_WarnFunction, ud: *mut c_void) {
    match f {
        None => st.set_warn_function(None),
        Some(f) => {
            let ud = ud as usize;
            st.set_warn_function(Some(Box::new(move |msg: &[u8], tocont: bool| {
                let mut c = msg.to_vec();
                c.push(0);
                // the callback receives a NUL-terminated copy of the message piece
                f(ud as *mut c_void, c.as_ptr().cast(), tocont as c_int);
            })));
        }
    }
});

lua_api!(lua_warning(st, msg: *const c_char, tocont: c_int) {
    st.warning(cstr(msg), tocont != 0);
});

// ---- garbage collector -------------------------------------------------------------------------

/// Entry point of the C shim for the variadic `lua_gc`.
#[no_mangle]
pub unsafe extern "C-unwind" fn rn_gc(
    l: *mut lua_State,
    what: c_int,
    a_size: usize,
    a_param: c_int,
    a_value: c_int,
) -> c_int {
    // SAFETY: valid state per the API contract.
    unsafe {
        let mut st = state(l);
        if st.g().heap.gc_stp() & (rn_core::gc::GCSTP_GC | rn_core::gc::GCSTP_CLOSING) != 0 {
            return -1; // all options are invalid when stopped internally
        }
        match what {
            LUA_GCSTOP => {
                st.heap().gc_stop();
                0
            }
            LUA_GCRESTART => {
                st.heap().gc_restart();
                0
            }
            LUA_GCCOLLECT => {
                st.collect(false);
                0
            }
            LUA_GCCOUNT => (st.g().heap.total_bytes() >> 10) as c_int,
            LUA_GCCOUNTB => (st.g().heap.total_bytes() & 0x3ff) as c_int,
            #[cfg(not(rustynail_lua54))]
            LUA_GCSTEP => st.gc_api_step(a_size) as c_int,
            // 5.4: `data` in Kbytes (`a_param`)
            #[cfg(rustynail_lua54)]
            LUA_GCSTEP => st.gc_api_step54(a_param) as c_int,
            LUA_GCISRUNNING => st.g().heap.gc_is_running() as c_int,
            #[cfg(rustynail_lua54)]
            LUA_GCSETPAUSE => st.heap().gc_params().set_pause54(a_param as i64) as c_int,
            #[cfg(rustynail_lua54)]
            LUA_GCSETSTEPMUL => st.heap().gc_params().set_stepmul54(a_param as i64) as c_int,
            // 5.4: `LUA_GCGEN(minormul, majormul)`, `LUA_GCINC(pause, stepmul, stepsize)` (0 =
            // keep), passed in `a_param`, `a_value`, `a_size`
            LUA_GCGEN | LUA_GCINC => {
                #[cfg(rustynail_lua54)]
                {
                    let p = st.heap().gc_params();
                    if what == LUA_GCGEN {
                        if a_param != 0 {
                            p.set_genminormul54(a_param as i64);
                        }
                        if a_value != 0 {
                            p.set_genmajormul54(a_value as i64);
                        }
                    } else {
                        if a_param != 0 {
                            p.set_pause54(a_param as i64);
                        }
                        if a_value != 0 {
                            p.set_stepmul54(a_value as i64);
                        }
                        if a_size != 0 {
                            p.set_stepsize54(a_size as i32 as i64);
                        }
                    }
                }
                let new = if what == LUA_GCGEN {
                    rn_core::gc::GcKind::GenMinor
                } else {
                    rn_core::gc::GcKind::Incremental
                };
                let old = st.gc_change_mode(new);
                if old.is_generational() {
                    LUA_GCGEN
                } else {
                    LUA_GCINC
                }
            }
            #[cfg(not(rustynail_lua54))]
            LUA_GCPARAM => {
                let idx = a_param as usize;
                if idx >= rn_core::gc::GCP_N {
                    return -1;
                }
                let res = st.heap().gc_params().get(idx) as c_int;
                if a_value >= 0 {
                    st.heap().gc_params().set(idx, a_value as u32);
                }
                res
            }
            _ => -1,
        }
    }
}

// ---- miscellaneous -----------------------------------------------------------------------------

lua_api!(lua_error(st) -> c_int {
    let eo = st.get(st.top() - 1);
    if eo.raw_equal(&Value::string(st.g().memerrmsg)) {
        throw(&mut st, Status::ErrMem)
    }
    let s = st.error_msg();
    raise(&mut st, s)
});

lua_api!(lua_next(st, idx: c_int) -> c_int {
    let t = val(&st, idx).as_table().expect("table expected (API contract)");
    let key = st.get(st.top() - 1);
    match t.next(&key) {
        Ok(Some((k, v))) => {
            let top = st.top();
            st.set(top - 1, k);
            st.push(v);
            1
        }
        Ok(None) => {
            let top = st.top();
            st.set_top(top - 1); // remove key
            0
        }
        Err(e) => {
            let s = st.table_error(e);
            raise(&mut st, s)
        }
    }
});

lua_api!(lua_concat(st, n: c_int) {
    if n > 0 {
        let r = st.concat(n as usize);
        ck(&mut st, r);
    } else {
        let r = st.new_str_value(b"");
        let v = ck(&mut st, r);
        st.push(v);
    }
    st.check_gc();
});

lua_api!(lua_len(st, idx: c_int) {
    let v = val(&st, idx);
    let r = st.len_value(v);
    let l = ck(&mut st, r);
    st.push(l);
});

lua_api!(lua_toclose(st, idx: c_int) {
    let level = stack_slot(&st, idx);
    let r = st.new_tbc_upval(level);
    ck(&mut st, r);
    // the function's results must be closed when it returns
    st.ci().w().callstatus |= CIST_TBC;
});

lua_api!(lua_closeslot(st, idx: c_int) {
    let level = stack_slot(&st, idx);
    let r = st.close_slot(level);
    ck(&mut st, r);
});

#[allow(dead_code)]
fn _unused(_: StkId) {}
