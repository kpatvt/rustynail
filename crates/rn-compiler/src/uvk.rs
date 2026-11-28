//! Section 77.3 (`uvk`, docs/JIT_TRACE.md): which captured local variables are never assigned after their initialisation.
//!
//! Every variable declaration gets an id (`vid`); every upvalue descriptor remembers the id of the variable it finally captures (through
//! the chain of enclosing upvalues); every assignment target that names a local or an upvalue marks its variable as assigned. The upvalue
//! descriptors of a nested function are final before the end of the variable's scope (where a later assignment can still appear), so
//! the flags are applied at the end of the compilation, walking the prototype tree in the order the functions were opened (preorder:
//! a parent opens before its children, siblings open and close in source order).
//!
//! Exactness: after `local x = e` (or `local function x`), the only writes to `x`'s register while it is active are assignment
//! statements naming `x` (or an upvalue of it in a nested function); the loop variables of numeric / generic `for` are written by
//! `FORLOOP` / `TFORCALL` only after the body block closed its upvalues (`OP_CLOSE` at the end of each iteration), so a closure sees one
//! value per variable instance. Writes outside the language (`debug.setlocal`, `debug.setupvalue`, `debug.upvaluejoin`, the C API) are
//! reported to the JIT at run time (`rn_core::jit_hooks::upval_changed`).

use rn_core::protodata::ProtoData;

const NONE: u32 = u32::MAX;

#[derive(Default)]
pub(crate) struct UvkTrack {
    next: u32,
    assigned: Vec<bool>,
    /// vid of each `actvar` entry (same index).
    var_vid: Vec<u32>,
    /// Per open function except the main one (`fstack[1..]`, index `fi - 1`): the vid of each upvalue.
    stack: Vec<Vec<u32>>,
    pid_stack: Vec<usize>,
    /// Per function in open order, the main one left out (index `pid - 1`): the vids of its upvalues.
    done: Vec<Vec<u32>>,
    /// Open functions, the main one included.
    depth: u32,
}

// (section 100.3: the main function is not tracked: its only upvalue is `_ENV`, which captures no variable (vid `NONE`, never `ro`). A chunk
// without nested functions, e.g. `load("return " .. k .. " + 1")`, then allocates nothing here: `load_small` +15 % instructions came from
// the five vectors this tracker grew per compiled chunk.)

impl UvkTrack {
    /// A variable descriptor was written at `actvar[at]`.
    pub(crate) fn new_var(&mut self, at: usize) {
        let vid = self.next;
        self.next += 1;
        self.assigned.push(false);
        if self.var_vid.len() <= at {
            self.var_vid.resize(at + 1, NONE);
        }
        self.var_vid[at] = vid;
    }
    pub(crate) fn var_vid(&self, at: usize) -> u32 {
        self.var_vid.get(at).copied().unwrap_or(NONE)
    }
    pub(crate) fn open(&mut self) {
        self.depth += 1;
        if self.depth == 1 {
            return;
        }
        self.pid_stack.push(self.done.len() + 1);
        self.done.push(Vec::new());
        self.stack.push(Vec::new());
    }
    pub(crate) fn close(&mut self) {
        let main = self.depth <= 1;
        self.depth = self.depth.saturating_sub(1);
        if main {
            return;
        }
        if let (Some(v), Some(pid)) = (self.stack.pop(), self.pid_stack.pop()) {
            self.done[pid - 1] = v;
        }
    }
    pub(crate) fn alloc_uv(&mut self, fi: usize) {
        let Some(fi) = fi.checked_sub(1) else { return };
        if let Some(s) = self.stack.get_mut(fi) {
            s.push(NONE);
        }
    }
    pub(crate) fn set_uv(&mut self, fi: usize, idx: usize, vid: u32) {
        let Some(fi) = fi.checked_sub(1) else { return };
        if let Some(e) = self.stack.get_mut(fi).and_then(|s| s.get_mut(idx)) {
            *e = vid;
        }
    }
    pub(crate) fn uv_vid(&self, fi: usize, idx: usize) -> u32 {
        let Some(fi) = fi.checked_sub(1) else {
            return NONE;
        };
        self.stack
            .get(fi)
            .and_then(|s| s.get(idx))
            .copied()
            .unwrap_or(NONE)
    }
    pub(crate) fn mark(&mut self, vid: u32) {
        if let Some(a) = self.assigned.get_mut(vid as usize) {
            *a = true;
        }
    }
    /// Sets `ro` on every upvalue descriptor of the tree rooted at `pd` (the main function, opened first).
    pub(crate) fn apply<S>(&self, pd: &mut ProtoData<S>) {
        if self.done.is_empty() {
            return;
        }
        let mut pid = 0;
        self.walk(pd, &mut pid);
    }
    fn walk<S>(&self, pd: &mut ProtoData<S>, pid: &mut usize) {
        let me = *pid;
        *pid += 1;
        if let Some(v) = me.checked_sub(1).and_then(|i| self.done.get(i)) {
            for (u, &vid) in pd.upvalues.iter_mut().zip(v.iter()) {
                u.ro = vid != NONE && !self.assigned.get(vid as usize).copied().unwrap_or(true);
            }
        }
        for c in pd.protos.iter_mut() {
            self.walk(c, pid);
        }
    }
}
