//! Port of `lparser.c` (Lua 5.4.9).
use super::codegen::{BinOpr, UnOpr};
use super::*;
use rn_core::instruction::{Instruction, MAXARG_BX};
use rn_core::protodata::{LocVar, UpvalDesc, PF_VAHID};
use rn_core::ver::active::OpCode;

type E<H> = ExpDesc<<H as CompilerHost>::Str>;
type Tk<H> = Token<<H as CompilerHost>::Str>;

const UNARY_PRIORITY: i32 = 12;
const PRIORITY: [(i32, i32); 21] = [
    (10, 10),
    (10, 10),
    (11, 11),
    (11, 11),
    (14, 13),
    (11, 11),
    (11, 11),
    (6, 6),
    (4, 4),
    (5, 5),
    (7, 7),
    (7, 7),
    (9, 8),
    (3, 3),
    (3, 3),
    (3, 3),
    (3, 3),
    (3, 3),
    (3, 3),
    (2, 2),
    (1, 1),
];

fn mem_err() -> CompileError {
    CompileError {
        message: b"not enough memory".to_vec(),
    }
}

/// Compile Lua 5.4 source text into a prototype. `chunk_name` is the usual `@file` / `=name` / source
/// string; `base_ccalls` is the C-call depth already consumed by the caller (1 for `luac`).
pub fn compile<H: CompilerHost>(
    host: &mut H,
    input: &[u8],
    chunk_name: &[u8],
    base_ccalls: u32,
) -> Result<ProtoData<H::Str>, CompileError> {
    compile_boxed(host, input, chunk_name, base_ccalls).map_err(|e| *e)
}

fn compile_boxed<H: CompilerHost>(
    host: &mut H,
    input: &[u8],
    chunk_name: &[u8],
    base_ccalls: u32,
) -> R<ProtoData<H::Str>> {
    let source = host.intern(chunk_name).map_err(|_| mem_err())?;
    let ls = LexState::new(host, input, chunk_name)?;
    let mut p = Parser {
        ls,
        fstack: Vec::new(),
        actvar: Vec::new(),
        nact: 0,
        gt: Vec::new(),
        label: Vec::new(),
        nccalls: base_ccalls,
        limit_error: None,
        kcache: Default::default(),
    };
    p.mainfunc(source)
}

impl<'a, H: CompilerHost> Parser<'a, H> {
    // ---- token helpers -----------------------------------------------------------------------

    fn next(&mut self) -> R<()> {
        Ok(self.ls.next_token()?)
    }

    fn is_char(&self, c: u8) -> bool {
        is_char(&self.ls.t, c)
    }

    fn error_expected(&self, t: &Tk<H>) -> CompileError {
        let mut m = self.ls.token2str(t);
        m.extend_from_slice(b" expected");
        self.ls.lex_error(&m, Some(&self.ls.t))
    }

    fn testnext(&mut self, t: Tk<H>) -> R<bool> {
        if tok_is(&self.ls.t, &t) {
            self.next()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn testnext_char(&mut self, c: u8) -> R<bool> {
        self.testnext(Token::Char(c))
    }

    fn check(&self, t: Tk<H>) -> R<()> {
        if !tok_is(&self.ls.t, &t) {
            return perr(self.error_expected(&t));
        }
        Ok(())
    }

    fn checknext(&mut self, t: Tk<H>) -> R<()> {
        self.check(t)?;
        self.next()
    }

    fn checknext_char(&mut self, c: u8) -> R<()> {
        self.checknext(Token::Char(c))
    }

    fn check_match(&mut self, what: Tk<H>, who: Tk<H>, where_: i32) -> R<()> {
        if !self.testnext(what)? {
            if where_ == self.ls.line_number {
                return perr(self.error_expected(&what));
            }
            let mut m = self.ls.token2str(&what);
            m.extend_from_slice(b" expected (to close ");
            m.extend_from_slice(&self.ls.token2str(&who));
            m.extend_from_slice(format!(" at line {where_})").as_bytes());
            return perr(self.ls.lex_error(&m, Some(&self.ls.t)));
        }
        Ok(())
    }

    fn syntax_error(&self, msg: &str) -> CompileError {
        self.ls.syntax_error(msg)
    }

    fn str_checkname(&mut self) -> R<H::Str> {
        match self.ls.t {
            Token::Name(s) => {
                self.next()?;
                Ok(s)
            }
            _ => perr(self.error_expected(&Token::Name(self.ls.envn))),
        }
    }

    fn name_text(&self, s: H::Str) -> String {
        String::from_utf8_lossy(self.ls.host.bytes(s)).into_owned()
    }

    fn init_exp(e: &mut E<H>, k: ExpKind, i: i32) {
        e.f = NO_JUMP;
        e.t = NO_JUMP;
        e.k = k;
        e.info = i;
    }

    fn codestring(e: &mut E<H>, s: H::Str) {
        e.f = NO_JUMP;
        e.t = NO_JUMP;
        e.k = ExpKind::KStr;
        e.strval = Some(s);
    }

    fn codename(&mut self, e: &mut E<H>) -> R<()> {
        let s = self.str_checkname()?;
        Self::codestring(e, s);
        Ok(())
    }

    fn new_e() -> E<H> {
        ExpDesc::new(ExpKind::Void, 0)
    }

    fn intern_lit(&mut self, s: &str) -> R<H::Str> {
        Ok(self.ls.host.intern(s.as_bytes()).map_err(|_| mem_err())?)
    }

    fn enter_level(&mut self) -> R<()> {
        self.nccalls += 1;
        if self.nccalls >= LUAI_MAXCCALLS {
            return perr(CompileError {
                message: b"C stack overflow".to_vec(),
            });
        }
        Ok(())
    }

    fn leave_level(&mut self) {
        self.nccalls -= 1;
    }

    // ---- variables ------------------------------------------------------------------------------

    fn register_local_var(&mut self, name: H::Str) -> R<i32> {
        if self.fsr().f.loc_vars.len() + 1 > SHRT_MAX {
            return perr(CompileError {
                message: b"too many local variables (limit is 32767)".to_vec(),
            });
        }
        let pc = self.fsr().pc() as u32;
        let fs = self.fs();
        fs.f.loc_vars.push(LocVar {
            name,
            start_pc: pc,
            end_pc: 0,
        });
        Ok(fs.f.loc_vars.len() as i32 - 1)
    }

    fn new_localvar(&mut self, name: H::Str) -> R<i32> {
        let fi = self.cur();
        let used = self.nact as i32 + 1 - self.fsr().firstlocal as i32;
        self.check_limit(fi, used, MAXVARS, "local variables")?;
        if self.nact + 1 > SHRT_MAX {
            return perr(CompileError {
                message: b"too many local variables (limit is 32767)".to_vec(),
            });
        }
        let vd = VarDesc {
            kind: VDKREG,
            ridx: 0,
            pidx: 0,
            name,
            k: Const::Nil,
        };
        if self.nact < self.actvar.len() {
            self.actvar[self.nact] = vd;
        } else {
            self.actvar.push(vd);
        }
        self.nact += 1;
        Ok(self.nact as i32 - 1 - self.fsr().firstlocal as i32)
    }

    fn new_localvar_literal(&mut self, s: &str) -> R<i32> {
        let n = self.intern_lit(s)?;
        self.new_localvar(n)
    }

    fn vardesc(&self, fi: usize, vidx: i32) -> &VarDesc<H::Str> {
        &self.actvar[self.fstack[fi].firstlocal + vidx as usize]
    }

    fn vardesc_mut(&mut self, fi: usize, vidx: i32) -> &mut VarDesc<H::Str> {
        let idx = self.fstack[fi].firstlocal + vidx as usize;
        &mut self.actvar[idx]
    }

    fn cur(&self) -> usize {
        self.fstack.len() - 1
    }

    fn reglevel(&self, fi: usize, mut nvar: i32) -> i32 {
        while nvar > 0 {
            nvar -= 1;
            let vd = self.vardesc(fi, nvar);
            if vd.kind != RDKCTC {
                return vd.ridx + 1;
            }
        }
        0
    }

    pub(crate) fn nvarstack(&self, fi: usize) -> i32 {
        self.reglevel(fi, self.fstack[fi].nactvar)
    }

    pub(crate) fn nvarstack_cur(&self) -> i32 {
        self.nvarstack(self.cur())
    }

    fn local_debug_info(&self, fi: usize, vidx: i32) -> Option<usize> {
        let vd = self.vardesc(fi, vidx);
        if vd.kind == RDKCTC {
            None
        } else {
            Some(vd.pidx as usize)
        }
    }

    fn init_var(&self, fi: usize, e: &mut E<H>, vidx: i32) {
        e.f = NO_JUMP;
        e.t = NO_JUMP;
        e.k = ExpKind::Local;
        e.var_vidx = vidx;
        e.var_ridx = self.vardesc(fi, vidx).ridx;
    }

    fn check_readonly(&mut self, e: &E<H>) -> R<()> {
        let fi = self.cur();
        let mut varname: Option<H::Str> = None;
        match e.k {
            ExpKind::Const => varname = Some(self.actvar[e.info as usize].name),
            ExpKind::Local => {
                let vd = self.vardesc(fi, e.var_vidx);
                if vd.kind != VDKREG {
                    varname = Some(vd.name);
                }
            }
            ExpKind::Upval => {
                let up = &self.fstack[fi].f.upvalues[e.info as usize];
                if up.kind != VDKREG {
                    varname = up.name;
                }
            }
            _ => return Ok(()),
        }
        if let Some(n) = varname {
            let msg = format!(
                "attempt to assign to const variable '{}'",
                self.name_text(n)
            );
            return perr(self.sem_err(&msg));
        }
        Ok(())
    }

    fn adjust_local_vars(&mut self, nvars: i32) -> R<()> {
        let fi = self.cur();
        let mut reglevel = self.nvarstack(fi);
        #[allow(clippy::explicit_counter_loop)]
        for _ in 0..nvars {
            let vidx = self.fsr().nactvar;
            self.fs().nactvar += 1;
            let name = self.vardesc(fi, vidx).name;
            let pidx = self.register_local_var(name)?;
            let vd = self.vardesc_mut(fi, vidx);
            vd.ridx = reglevel;
            vd.pidx = pidx;
            reglevel += 1;
        }
        Ok(())
    }

    fn remove_vars(&mut self, tolevel: i32) {
        let fi = self.cur();
        let n = (self.fsr().nactvar - tolevel) as usize;
        let newlen = self.nact - n;
        // debug info must be read before the descriptors are dropped
        let pc = self.fsr().pc() as u32;
        while self.fsr().nactvar > tolevel {
            self.fs().nactvar -= 1;
            let v = self.fsr().nactvar;
            if let Some(i) = self.local_debug_info(fi, v) {
                self.fs().f.loc_vars[i].end_pc = pc;
            }
        }
        self.nact = newlen;
    }

    fn search_upvalue(&self, fi: usize, name: H::Str) -> i32 {
        for (i, u) in self.fstack[fi].f.upvalues.iter().enumerate() {
            if u.name.is_some_and(|un| H::str_id(un) == H::str_id(name)) {
                return i as i32;
            }
        }
        -1
    }

    fn alloc_upvalue(&mut self, fi: usize) -> R<usize> {
        let n = self.fstack[fi].f.upvalues.len() as i32;
        self.check_limit(fi, n + 1, MAXUPVAL, "upvalues")?;
        self.fstack[fi].f.upvalues.push(UpvalDesc {
            name: None,
            in_stack: false,
            idx: 0,
            kind: 0,
        });
        Ok(n as usize)
    }

    fn new_upvalue(&mut self, fi: usize, name: H::Str, v: &E<H>) -> R<i32> {
        let idx = self.alloc_upvalue(fi)?;
        let prev = fi - 1;
        let (in_stack, uidx, kind) = if v.k == ExpKind::Local {
            (true, v.var_ridx as u8, self.vardesc(prev, v.var_vidx).kind)
        } else {
            (
                false,
                v.info as u8,
                self.fstack[prev].f.upvalues[v.info as usize].kind,
            )
        };
        let up = &mut self.fstack[fi].f.upvalues[idx];
        up.in_stack = in_stack;
        up.idx = uidx;
        up.kind = kind;
        up.name = Some(name);
        Ok(idx as i32)
    }

    fn search_var(&self, fi: usize, n: H::Str, var: &mut E<H>) -> Option<ExpKind> {
        let fs = &self.fstack[fi];
        let mut i = fs.nactvar - 1;
        while i >= 0 {
            let vd = self.vardesc(fi, i);
            if H::str_id(vd.name) == H::str_id(n) {
                if vd.kind == RDKCTC {
                    Self::init_exp(var, ExpKind::Const, fs.firstlocal as i32 + i);
                } else {
                    self.init_var(fi, var, i);
                }
                return Some(var.k);
            }
            i -= 1;
        }
        None
    }

    fn mark_upval(&mut self, fi: usize, level: i32) {
        let fs = &mut self.fstack[fi];
        let mut b = fs.bl.len();
        while b > 0 && fs.bl[b - 1].nactvar > level {
            b -= 1;
        }
        if b > 0 {
            fs.bl[b - 1].upval = true;
        }
        fs.needclose = true;
    }

    fn mark_to_be_closed(&mut self, fi: usize) {
        let fs = &mut self.fstack[fi];
        if let Some(bl) = fs.bl.last_mut() {
            bl.upval = true;
            bl.insidetbc = true;
        }
        fs.needclose = true;
    }

    /// `singlevaraux`; `fi == None` is the "no more levels" case (the variable is a global).
    fn single_var_aux(
        &mut self,
        fi: Option<usize>,
        n: H::Str,
        var: &mut E<H>,
        base: bool,
    ) -> R<()> {
        let Some(fi) = fi else {
            Self::init_exp(var, ExpKind::Void, 0);
            return Ok(());
        };
        if let Some(v) = self.search_var(fi, n, var) {
            if v == ExpKind::Local && !base {
                self.mark_upval(fi, var.var_vidx);
            }
        } else {
            let mut idx = self.search_upvalue(fi, n);
            if idx < 0 {
                self.single_var_aux(fi.checked_sub(1), n, var, false)?;
                if var.k == ExpKind::Local || var.k == ExpKind::Upval {
                    idx = self.new_upvalue(fi, n, var)?;
                } else {
                    return Ok(());
                }
            }
            Self::init_exp(var, ExpKind::Upval, idx);
        }
        Ok(())
    }

    fn single_var(&mut self, var: &mut E<H>) -> R<()> {
        let varname = self.str_checkname()?;
        let fi = self.cur();
        self.single_var_aux(Some(fi), varname, var, true)?;
        if var.k == ExpKind::Void {
            let envn = self.ls.envn;
            self.single_var_aux(Some(fi), envn, var, true)?;
            self.k_exp2anyregup(var)?;
            let mut key = Self::new_e();
            Self::codestring(&mut key, varname);
            self.k_indexed(var, &mut key)?;
        }
        Ok(())
    }

    fn adjust_assign(&mut self, nvars: i32, nexps: i32, e: &mut E<H>) -> R<()> {
        let needed = nvars - nexps;
        if e.k.has_multret() {
            let extra = (needed + 1).max(0);
            self.k_set_returns(e, extra)?;
        } else {
            if e.k != ExpKind::Void {
                self.k_exp2nextreg(e)?;
            }
            if needed > 0 {
                let fr = self.fsr().freereg;
                self.k_nil(fr, needed)?;
            }
        }
        if needed > 0 {
            self.k_reserve_regs(needed)?;
        } else {
            self.fs().freereg += needed;
        }
        Ok(())
    }

    // ---- gotos and labels ---------------------------------------------------------------------------

    fn jump_scope_error(&mut self, gt: LabelDesc<H::Str>) -> CompileError {
        let fi = self.cur();
        let varname = self.vardesc(fi, gt.nactvar).name;
        let msg = format!(
            "<goto {}> at line {} jumps into the scope of local '{}'",
            self.name_text(gt.name),
            gt.line,
            self.name_text(varname)
        );
        self.sem_err(&msg)
    }

    fn solve_goto(&mut self, g: usize, label: LabelDesc<H::Str>) -> R<()> {
        let gt = self.gt[g];
        if gt.nactvar < label.nactvar {
            return perr(self.jump_scope_error(gt));
        }
        self.k_patch_list(gt.pc, label.pc)?;
        self.gt.remove(g);
        Ok(())
    }

    fn find_label(&self, name: H::Str) -> Option<LabelDesc<H::Str>> {
        self.label
            .iter()
            .skip(self.fsr().firstlabel)
            .find(|l| l.name == name)
            .copied()
    }

    fn new_label_entry(&mut self, is_goto: bool, name: H::Str, line: i32, pc: i32) -> R<usize> {
        let l = if is_goto { &self.gt } else { &self.label };
        if l.len() + 1 > SHRT_MAX {
            return perr(CompileError {
                message: b"too many labels/gotos (limit is 32767)".to_vec(),
            });
        }
        let entry = LabelDesc {
            name,
            pc,
            line,
            nactvar: self.fsr().nactvar,
            close: false,
        };
        let l = if is_goto {
            &mut self.gt
        } else {
            &mut self.label
        };
        l.push(entry);
        Ok(l.len() - 1)
    }

    fn new_goto_entry(&mut self, name: H::Str, line: i32, pc: i32) -> R<usize> {
        self.new_label_entry(true, name, line, pc)
    }

    /// `solvegotos`: solve pending gotos of the current block that match `lb`; true if any of them
    /// needs to close upvalues.
    fn solve_gotos(&mut self, lb: LabelDesc<H::Str>) -> R<bool> {
        let mut i = self.fsr().bl.last().map_or(0, |b| b.firstgoto);
        let mut needsclose = false;
        while i < self.gt.len() {
            if self.gt[i].name == lb.name {
                needsclose |= self.gt[i].close;
                self.solve_goto(i, lb)?;
            } else {
                i += 1;
            }
        }
        Ok(needsclose)
    }

    /// `createlabel`: true iff it added a close instruction.
    fn create_label(&mut self, name: H::Str, line: i32, last: bool) -> R<bool> {
        let pc = self.k_get_label();
        let l = self.new_label_entry(false, name, line, pc)?;
        if last {
            let n = self.fsr().bl.last().map_or(0, |b| b.nactvar);
            self.label[l].nactvar = n;
        }
        let lb = self.label[l];
        if self.solve_gotos(lb)? {
            let nv = self.nvarstack_cur();
            self.k_code_abc(OpCode::Close, nv, 0, 0)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn move_gotos_out(&mut self, bl: BlockCnt) {
        let fi = self.cur();
        let outlevel = self.reglevel(fi, bl.nactvar);
        for i in bl.firstgoto..self.gt.len() {
            let gt = self.gt[i];
            if self.reglevel(fi, gt.nactvar) > outlevel {
                self.gt[i].close |= bl.upval;
            }
            self.gt[i].nactvar = bl.nactvar;
        }
    }

    fn enter_block(&mut self, isloop: bool) {
        let nactvar = self.fsr().nactvar;
        let firstlabel = self.label.len();
        let firstgoto = self.gt.len();
        let inside = self.fsr().bl.last().is_some_and(|b| b.insidetbc);
        self.fs().bl.push(BlockCnt {
            firstlabel,
            firstgoto,
            nactvar,
            upval: false,
            isloop,
            insidetbc: inside,
        });
    }

    fn undef_goto(&mut self, gt: LabelDesc<H::Str>) -> CompileError {
        let msg = if gt.name == self.ls.brkn {
            format!("break outside loop at line {}", gt.line)
        } else {
            format!(
                "no visible label '{}' for <goto> at line {}",
                self.name_text(gt.name),
                gt.line
            )
        };
        self.sem_err(&msg)
    }

    fn leave_block(&mut self) -> R<()> {
        let fi = self.cur();
        let bl = match self.fsr().bl.last() {
            Some(b) => *b,
            None => return Ok(()),
        };
        let has_prev = self.fsr().bl.len() > 1;
        let mut hasclose = false;
        let stklevel = self.reglevel(fi, bl.nactvar);
        self.remove_vars(bl.nactvar);
        if bl.isloop {
            let brkn = self.ls.brkn;
            hasclose = self.create_label(brkn, 0, false)?;
        }
        if !hasclose && has_prev && bl.upval {
            self.k_code_abc(OpCode::Close, stklevel, 0, 0)?;
        }
        self.fs().freereg = stklevel;
        self.label.truncate(bl.firstlabel);
        self.fs().bl.pop();
        if has_prev {
            self.move_gotos_out(bl);
        } else if bl.firstgoto < self.gt.len() {
            let gt = self.gt[bl.firstgoto];
            return perr(self.undef_goto(gt));
        }
        Ok(())
    }

    // ---- functions -------------------------------------------------------------------------------------------

    fn open_func(&mut self, source: Option<H::Str>, line_defined: i32) {
        let mut f = ProtoData {
            num_params: 0,
            flags: 0,
            max_stack_size: 2,
            line_defined,
            last_line_defined: 0,
            code: Vec::new(),
            k: Vec::new(),
            protos: Vec::new(),
            upvalues: Vec::new(),
            line_info: Vec::new(),
            abs_line_info: Vec::new(),
            loc_vars: Vec::new(),
            source,
        };
        f.source = source;
        self.fstack.push(FuncState {
            f,
            bl: Vec::new(),
            lasttarget: 0,
            previousline: line_defined,
            np: 0,
            firstlocal: self.nact,
            firstlabel: self.label.len(),
            nactvar: 0,
            freereg: 0,
            iwthabs: 0,
            needclose: false,
        });
        self.enter_block(false);
    }

    fn close_func(&mut self) -> R<ProtoData<H::Str>> {
        let nv = self.nvarstack_cur();
        self.k_ret(nv, 0)?;
        self.leave_block()?;
        self.k_finish()?;
        match self.fstack.pop() {
            Some(fs) => Ok(fs.f),
            None => perr(mem_err()),
        }
    }

    fn mainfunc(&mut self, source: H::Str) -> R<ProtoData<H::Str>> {
        self.open_func(Some(source), 0);
        self.set_vararg(0)?;
        let fi = self.cur();
        let idx = self.alloc_upvalue(fi)?;
        let envn = self.ls.envn;
        let up = &mut self.fstack[fi].f.upvalues[idx];
        up.in_stack = true;
        up.idx = 0;
        up.kind = VDKREG;
        up.name = Some(envn);
        self.next()?;
        self.statlist()?;
        self.check(Token::Eos)?;
        self.close_func()
    }

    fn set_vararg(&mut self, nparams: i32) -> R<()> {
        self.fs().f.flags |= PF_VAHID;
        self.k_code_abc(OpCode::Varargprep, nparams, 0, 0)?;
        Ok(())
    }

    // ---- grammar ------------------------------------------------------------------------------------------------------

    fn block_follow(&self, withuntil: bool) -> bool {
        match self.ls.t {
            Token::Else | Token::Elseif | Token::End | Token::Eos => true,
            Token::Until => withuntil,
            _ => false,
        }
    }

    fn statlist(&mut self) -> R<()> {
        while !self.block_follow(true) {
            if matches!(self.ls.t, Token::Return) {
                self.statement()?;
                return Ok(());
            }
            self.statement()?;
        }
        Ok(())
    }

    fn fieldsel(&mut self, v: &mut E<H>) -> R<()> {
        self.k_exp2anyregup(v)?;
        self.next()?;
        let mut key = Self::new_e();
        self.codename(&mut key)?;
        self.k_indexed(v, &mut key)
    }

    fn yindex(&mut self, v: &mut E<H>) -> R<()> {
        self.next()?;
        self.expr(v)?;
        self.k_exp2val(v)?;
        self.checknext_char(b']')
    }

    // -- constructors

    fn recfield(&mut self, cc: &mut ConsControl<H>) -> R<()> {
        let reg = self.fsr().freereg;
        let mut key = Self::new_e();
        let mut val = Self::new_e();
        if matches!(self.ls.t, Token::Name(_)) {
            self.codename(&mut key)?;
        } else {
            self.yindex(&mut key)?;
        }
        cc.nh += 1;
        self.checknext_char(b'=')?;
        let mut tab = cc.t;
        self.k_indexed(&mut tab, &mut key)?;
        self.expr(&mut val)?;
        self.k_storevar(&mut tab, &mut val)?;
        self.fs().freereg = reg;
        Ok(())
    }

    fn close_list_field(&mut self, cc: &mut ConsControl<H>) -> R<()> {
        if cc.v.k == ExpKind::Void {
            return Ok(());
        }
        self.k_exp2nextreg(&mut cc.v)?;
        cc.v.k = ExpKind::Void;
        if cc.tostore == LFIELDS_PER_FLUSH {
            self.k_set_list(cc.t.info, cc.na, cc.tostore)?;
            cc.na += cc.tostore;
            cc.tostore = 0;
        }
        Ok(())
    }

    fn last_list_field(&mut self, cc: &mut ConsControl<H>) -> R<()> {
        if cc.tostore == 0 {
            return Ok(());
        }
        if cc.v.k.has_multret() {
            self.k_set_multret(&mut cc.v)?;
            self.k_set_list(cc.t.info, cc.na, LUA_MULTRET)?;
            cc.na -= 1;
        } else {
            if cc.v.k != ExpKind::Void {
                self.k_exp2nextreg(&mut cc.v)?;
            }
            self.k_set_list(cc.t.info, cc.na, cc.tostore)?;
        }
        cc.na += cc.tostore;
        Ok(())
    }

    fn list_field(&mut self, cc: &mut ConsControl<H>) -> R<()> {
        let mut v = Self::new_e();
        self.expr(&mut v)?;
        cc.v = v;
        cc.tostore += 1;
        Ok(())
    }

    fn field(&mut self, cc: &mut ConsControl<H>) -> R<()> {
        match self.ls.t {
            Token::Name(_) => {
                if !matches!(self.ls.lookahead()?, Token::Char(b'=')) {
                    self.list_field(cc)
                } else {
                    self.recfield(cc)
                }
            }
            Token::Char(b'[') => self.recfield(cc),
            _ => self.list_field(cc),
        }
    }

    fn constructor(&mut self, t: &mut E<H>) -> R<()> {
        let line = self.ls.line_number;
        let pc = self.k_code_abc(OpCode::Newtable, 0, 0, 0)?;
        self.k_code(Instruction(0))?;
        Self::init_exp(t, ExpKind::NonReloc, self.fsr().freereg);
        self.k_reserve_regs(1)?;
        let mut cc = ConsControl::<H> {
            v: Self::new_e(),
            t: *t,
            nh: 0,
            na: 0,
            tostore: 0,
        };
        Self::init_exp(&mut cc.v, ExpKind::Void, 0);
        self.checknext_char(b'{')?;
        loop {
            if self.is_char(b'}') {
                break;
            }
            self.close_list_field(&mut cc)?;
            self.field(&mut cc)?;
            let fi = self.cur();
            self.check_limit(
                fi,
                cc.tostore + cc.na + cc.nh,
                i32::MAX / 2,
                "items in a constructor",
            )?;
            if !(self.testnext_char(b',')? || self.testnext_char(b';')?) {
                break;
            }
        }
        self.check_match(Token::Char(b'}'), Token::Char(b'{'), line)?;
        self.last_list_field(&mut cc)?;
        self.k_set_table_size(pc, t.info, cc.na, cc.nh);
        Ok(())
    }

    fn parlist(&mut self) -> R<()> {
        let mut nparams = 0;
        let mut isvararg = false;
        if !self.is_char(b')') {
            loop {
                match self.ls.t {
                    Token::Name(_) => {
                        let n = self.str_checkname()?;
                        self.new_localvar(n)?;
                        nparams += 1;
                    }
                    Token::Dots => {
                        self.next()?;
                        isvararg = true;
                    }
                    _ => return perr(self.syntax_error("<name> or '...' expected")),
                }
                if isvararg || !self.testnext_char(b',')? {
                    break;
                }
            }
        }
        self.adjust_local_vars(nparams)?;
        let na = self.fsr().nactvar;
        self.fs().f.num_params = na as u8;
        if isvararg {
            self.set_vararg(na)?;
        }
        let n = self.fsr().nactvar;
        self.k_reserve_regs(n)
    }

    fn body(&mut self, e: &mut E<H>, ismethod: bool, line: i32) -> R<()> {
        let source = self.fsr().f.source;
        if self.fsr().np >= MAXARG_BX as usize {
            // addprototype: luaM_growvector(f->p, fs->np, ..., MAXARG_Bx, "functions")
            return perr(CompileError {
                message: format!("too many functions (limit is {MAXARG_BX})").into_bytes(),
            });
        }
        self.fs().np += 1;
        self.open_func(source, line);
        self.checknext_char(b'(')?;
        if ismethod {
            self.new_localvar_literal("self")?;
            self.adjust_local_vars(1)?;
        }
        self.parlist()?;
        self.checknext_char(b')')?;
        self.statlist()?;
        self.fs().f.last_line_defined = self.ls.line_number;
        self.check_match(Token::End, Token::Function, line)?;
        // codeclosure: emitted in the enclosing function while the child is still open
        let child_fs = self.fstack.pop();
        let np = self.fsr().np as i32;
        let pc = self.k_code_abx(OpCode::Closure, 0, np - 1)?;
        Self::init_exp(e, ExpKind::Reloc, pc);
        self.k_exp2nextreg(e)?;
        if let Some(c) = child_fs {
            self.fstack.push(c);
        }
        let child = self.close_func()?;
        self.fs().f.protos.push(child);
        Ok(())
    }

    fn explist(&mut self, v: &mut E<H>) -> R<i32> {
        let mut n = 1;
        self.expr(v)?;
        while self.testnext_char(b',')? {
            self.k_exp2nextreg(v)?;
            self.expr(v)?;
            n += 1;
        }
        Ok(n)
    }

    fn funcargs(&mut self, f: &mut E<H>) -> R<()> {
        let mut args = Self::new_e();
        let line = self.ls.line_number;
        match self.ls.t {
            Token::Char(b'(') => {
                self.next()?;
                if self.is_char(b')') {
                    args.k = ExpKind::Void;
                } else {
                    self.explist(&mut args)?;
                    if args.k.has_multret() {
                        self.k_set_multret(&mut args)?;
                    }
                }
                self.check_match(Token::Char(b')'), Token::Char(b'('), line)?;
            }
            Token::Char(b'{') => self.constructor(&mut args)?,
            Token::Str(s) => {
                Self::codestring(&mut args, s);
                self.next()?;
            }
            _ => return perr(self.syntax_error("function arguments expected")),
        }
        let base = f.info;
        let nparams = if args.k.has_multret() {
            LUA_MULTRET
        } else {
            if args.k != ExpKind::Void {
                self.k_exp2nextreg(&mut args)?;
            }
            self.fsr().freereg - (base + 1)
        };
        let pc = self.k_code_abc(OpCode::Call, base, nparams + 1, 2)?;
        Self::init_exp(f, ExpKind::Call, pc);
        self.k_fix_line(line)?;
        self.fs().freereg = base + 1;
        Ok(())
    }

    fn primaryexp(&mut self, v: &mut E<H>) -> R<()> {
        match self.ls.t {
            Token::Char(b'(') => {
                let line = self.ls.line_number;
                self.next()?;
                self.expr(v)?;
                self.check_match(Token::Char(b')'), Token::Char(b'('), line)?;
                self.k_discharge_vars(v)
            }
            Token::Name(_) => self.single_var(v),
            _ => perr(self.syntax_error("unexpected symbol")),
        }
    }

    fn suffixedexp(&mut self, v: &mut E<H>) -> R<()> {
        self.primaryexp(v)?;
        loop {
            match self.ls.t {
                Token::Char(b'.') => self.fieldsel(v)?,
                Token::Char(b'[') => {
                    let mut key = Self::new_e();
                    self.k_exp2anyregup(v)?;
                    self.yindex(&mut key)?;
                    self.k_indexed(v, &mut key)?;
                }
                Token::Char(b':') => {
                    let mut key = Self::new_e();
                    self.next()?;
                    self.codename(&mut key)?;
                    self.k_self(v, &mut key)?;
                    self.funcargs(v)?;
                }
                Token::Char(b'(') | Token::Str(_) | Token::Char(b'{') => {
                    self.k_exp2nextreg(v)?;
                    self.funcargs(v)?;
                }
                _ => return Ok(()),
            }
        }
    }

    fn simpleexp(&mut self, v: &mut E<H>) -> R<()> {
        match self.ls.t {
            Token::Flt(f) => {
                Self::init_exp(v, ExpKind::KFlt, 0);
                v.nval = f;
            }
            Token::Int(i) => {
                Self::init_exp(v, ExpKind::KInt, 0);
                v.ival = i;
            }
            Token::Str(s) => Self::codestring(v, s),
            Token::Nil => Self::init_exp(v, ExpKind::Nil, 0),
            Token::True => Self::init_exp(v, ExpKind::True, 0),
            Token::False => Self::init_exp(v, ExpKind::False, 0),
            Token::Dots => {
                if self.fsr().f.flags & PF_VAHID == 0 {
                    return perr(self.syntax_error("cannot use '...' outside a vararg function"));
                }
                let pc = self.k_code_abc(OpCode::Vararg, 0, 0, 1)?;
                Self::init_exp(v, ExpKind::Vararg, pc);
            }
            Token::Char(b'{') => return self.constructor(v),
            Token::Function => {
                self.next()?;
                let line = self.ls.line_number;
                return self.body(v, false, line);
            }
            _ => return self.suffixedexp(v),
        }
        self.next()
    }

    fn unopr(t: &Tk<H>) -> UnOpr {
        match t {
            Token::Not => UnOpr::Not,
            Token::Char(b'-') => UnOpr::Minus,
            Token::Char(b'~') => UnOpr::BNot,
            Token::Char(b'#') => UnOpr::Len,
            _ => UnOpr::NoUnOpr,
        }
    }

    fn binopr(t: &Tk<H>) -> BinOpr {
        match t {
            Token::Char(b'+') => BinOpr::Add,
            Token::Char(b'-') => BinOpr::Sub,
            Token::Char(b'*') => BinOpr::Mul,
            Token::Char(b'%') => BinOpr::Mod,
            Token::Char(b'^') => BinOpr::Pow,
            Token::Char(b'/') => BinOpr::Div,
            Token::IDiv => BinOpr::IDiv,
            Token::Char(b'&') => BinOpr::BAnd,
            Token::Char(b'|') => BinOpr::BOr,
            Token::Char(b'~') => BinOpr::BXor,
            Token::Shl => BinOpr::Shl,
            Token::Shr => BinOpr::Shr,
            Token::Concat => BinOpr::Concat,
            Token::Ne => BinOpr::Ne,
            Token::Eq => BinOpr::Eq,
            Token::Char(b'<') => BinOpr::Lt,
            Token::Le => BinOpr::Le,
            Token::Char(b'>') => BinOpr::Gt,
            Token::Ge => BinOpr::Ge,
            Token::And => BinOpr::And,
            Token::Or => BinOpr::Or,
            _ => BinOpr::NoBinOpr,
        }
    }

    fn subexpr(&mut self, v: &mut E<H>, limit: i32) -> R<BinOpr> {
        self.enter_level()?;
        let uop = Self::unopr(&self.ls.t);
        if uop != UnOpr::NoUnOpr {
            let line = self.ls.line_number;
            self.next()?;
            self.subexpr(v, UNARY_PRIORITY)?;
            self.k_prefix(uop, v, line)?;
        } else {
            self.simpleexp(v)?;
        }
        let mut op = Self::binopr(&self.ls.t);
        while op != BinOpr::NoBinOpr && PRIORITY[op as usize].0 > limit {
            let mut v2 = Self::new_e();
            let line = self.ls.line_number;
            self.next()?;
            self.k_infix(op, v)?;
            let nextop = self.subexpr(&mut v2, PRIORITY[op as usize].1)?;
            self.k_posfix(op, v, &mut v2, line)?;
            op = nextop;
        }
        self.leave_level();
        Ok(op)
    }

    fn expr(&mut self, v: &mut E<H>) -> R<()> {
        self.subexpr(v, 0)?;
        Ok(())
    }

    // -- statements

    fn block(&mut self) -> R<()> {
        self.enter_block(false);
        self.statlist()?;
        self.leave_block()
    }

    fn check_conflict(&mut self, lhs: &mut [E<H>], v: &E<H>) -> R<()> {
        let extra = self.fsr().freereg;
        let mut conflict = false;
        for lh in lhs.iter_mut() {
            if lh.k.is_indexed() {
                if lh.k == ExpKind::IndexUp {
                    if v.k == ExpKind::Upval && lh.ind_t == v.info {
                        conflict = true;
                        lh.k = ExpKind::IndexStr;
                        lh.ind_t = extra;
                    }
                } else {
                    if v.k == ExpKind::Local && lh.ind_t == v.var_ridx {
                        conflict = true;
                        lh.ind_t = extra;
                    }
                    if lh.k == ExpKind::Indexed && v.k == ExpKind::Local && lh.ind_idx == v.var_ridx
                    {
                        conflict = true;
                        lh.ind_idx = extra;
                    }
                }
            }
        }
        if conflict {
            if v.k == ExpKind::Local {
                self.k_code_abc(OpCode::Move, extra, v.var_ridx, 0)?;
            } else {
                self.k_code_abc(OpCode::Getupval, extra, v.info, 0)?;
            }
            self.k_reserve_regs(1)?;
        }
        Ok(())
    }

    fn restassign(&mut self, lhs: &mut Vec<E<H>>, nvars: i32) -> R<()> {
        let last = lhs.len() - 1;
        if !lhs[last].k.is_var() {
            return perr(self.syntax_error("syntax error"));
        }
        self.check_readonly(&lhs[last])?;
        if self.testnext_char(b',')? {
            let mut nv = Self::new_e();
            self.suffixedexp(&mut nv)?;
            if !nv.k.is_indexed() {
                self.check_conflict(lhs, &nv)?;
            }
            self.enter_level()?;
            lhs.push(nv);
            self.restassign(lhs, nvars + 1)?;
            lhs.pop();
            self.leave_level();
        } else {
            let mut e = Self::new_e();
            self.checknext_char(b'=')?;
            let nexps = self.explist(&mut e)?;
            if nexps != nvars {
                self.adjust_assign(nvars, nexps, &mut e)?;
            } else {
                self.k_set_one_ret(&mut e);
                self.k_storevar(&mut lhs[last], &mut e)?;
                return Ok(());
            }
        }
        let mut e = Self::new_e();
        Self::init_exp(&mut e, ExpKind::NonReloc, self.fsr().freereg - 1);
        self.k_storevar(&mut lhs[last], &mut e)
    }

    fn cond(&mut self) -> R<i32> {
        let mut v = Self::new_e();
        self.expr(&mut v)?;
        if v.k == ExpKind::Nil {
            v.k = ExpKind::False;
        }
        self.k_go_if_true(&mut v)?;
        Ok(v.f)
    }

    fn gotostat(&mut self) -> R<()> {
        let line = self.ls.line_number;
        let name = self.str_checkname()?;
        match self.find_label(name) {
            None => {
                // forward jump; will be resolved when the label is declared
                let pc = self.k_jump()?;
                self.new_goto_entry(name, line, pc)?;
            }
            Some(lb) => {
                // backward jump; will be resolved here
                let fi = self.cur();
                let lblevel = self.reglevel(fi, lb.nactvar);
                if self.nvarstack_cur() > lblevel {
                    self.k_code_abc(OpCode::Close, lblevel, 0, 0)?;
                }
                let j = self.k_jump()?;
                self.k_patch_list(j, lb.pc)?;
            }
        }
        Ok(())
    }

    /// Break statement. Semantically equivalent to "goto break".
    fn breakstat(&mut self) -> R<()> {
        let line = self.ls.line_number;
        self.next()?;
        let pc = self.k_jump()?;
        let brkn = self.ls.brkn;
        self.new_goto_entry(brkn, line, pc)?;
        Ok(())
    }

    fn check_repeated(&mut self, name: H::Str) -> R<()> {
        if let Some(lb) = self.find_label(name) {
            let msg = format!(
                "label '{}' already defined on line {}",
                self.name_text(name),
                lb.line
            );
            return perr(self.sem_err(&msg));
        }
        Ok(())
    }

    fn labelstat(&mut self, name: H::Str, line: i32) -> R<()> {
        self.checknext(Token::DbColon)?;
        while self.is_char(b';') || matches!(self.ls.t, Token::DbColon) {
            self.statement()?;
        }
        self.check_repeated(name)?;
        let last = self.block_follow(false);
        self.create_label(name, line, last)?;
        Ok(())
    }

    fn whilestat(&mut self, line: i32) -> R<()> {
        self.next()?;
        let whileinit = self.k_get_label();
        let condexit = self.cond()?;
        self.enter_block(true);
        self.checknext(Token::Do)?;
        self.block()?;
        let j = self.k_jump()?;
        self.k_patch_list(j, whileinit)?;
        self.check_match(Token::End, Token::While, line)?;
        self.leave_block()?;
        self.k_patch_to_here(condexit)
    }

    fn repeatstat(&mut self, line: i32) -> R<()> {
        let repeat_init = self.k_get_label();
        self.enter_block(true);
        self.enter_block(false);
        self.next()?;
        self.statlist()?;
        self.check_match(Token::Until, Token::Repeat, line)?;
        let mut condexit = self.cond()?;
        let bl2 = self.fsr().bl.last().copied();
        self.leave_block()?;
        let (bl2_upval, bl2_nactvar) = bl2.map_or((false, 0), |b| (b.upval, b.nactvar));
        if bl2_upval {
            let exit = self.k_jump()?;
            self.k_patch_to_here(condexit)?;
            let fi = self.cur();
            let lvl = self.reglevel(fi, bl2_nactvar);
            self.k_code_abc(OpCode::Close, lvl, 0, 0)?;
            condexit = self.k_jump()?;
            self.k_patch_to_here(exit)?;
        }
        self.k_patch_list(condexit, repeat_init)?;
        self.leave_block()
    }

    fn exp1(&mut self) -> R<()> {
        let mut e = Self::new_e();
        self.expr(&mut e)?;
        self.k_exp2nextreg(&mut e)
    }

    fn fix_for_jump(&mut self, pc: i32, dest: i32, back: bool) -> R<()> {
        let mut offset = dest - (pc + 1);
        if back {
            offset = -offset;
        }
        if offset > MAXARG_BX as i32 {
            return perr(self.syntax_error("control structure too long"));
        }
        let mut i = Instruction(self.fsr().f.code[pc as usize]);
        i.set_bx(offset as u32);
        self.fs().f.code[pc as usize] = i.0;
        Ok(())
    }

    fn forbody(&mut self, base: i32, line: i32, nvars: i32, isgen: bool) -> R<()> {
        let (forprep, forloop) = if isgen {
            (OpCode::Tforprep, OpCode::Tforloop)
        } else {
            (OpCode::Forprep, OpCode::Forloop)
        };
        self.checknext(Token::Do)?;
        let prep = self.k_code_abx(forprep, base, 0)?;
        self.enter_block(false);
        self.adjust_local_vars(nvars)?;
        self.k_reserve_regs(nvars)?;
        self.block()?;
        self.leave_block()?;
        let lbl = self.k_get_label();
        self.fix_for_jump(prep, lbl, false)?;
        if isgen {
            self.k_code_abc(OpCode::Tforcall, base, 0, nvars)?;
            self.k_fix_line(line)?;
        }
        let endfor = self.k_code_abx(forloop, base, 0)?;
        self.fix_for_jump(endfor, prep + 1, true)?;
        self.k_fix_line(line)
    }

    fn fornum(&mut self, varname: H::Str, line: i32) -> R<()> {
        let base = self.fsr().freereg;
        self.new_localvar_literal("(for state)")?;
        self.new_localvar_literal("(for state)")?;
        self.new_localvar_literal("(for state)")?;
        self.new_localvar(varname)?;
        self.checknext_char(b'=')?;
        self.exp1()?;
        self.checknext_char(b',')?;
        self.exp1()?;
        if self.testnext_char(b',')? {
            self.exp1()?;
        } else {
            let fr = self.fsr().freereg;
            self.k_int(fr, 1)?;
            self.k_reserve_regs(1)?;
        }
        self.adjust_local_vars(3)?;
        self.forbody(base, line, 1, false)
    }

    fn forlist(&mut self, indexname: H::Str) -> R<()> {
        let mut nvars = 5;
        let base = self.fsr().freereg;
        self.new_localvar_literal("(for state)")?;
        self.new_localvar_literal("(for state)")?;
        self.new_localvar_literal("(for state)")?;
        self.new_localvar_literal("(for state)")?;
        self.new_localvar(indexname)?;
        while self.testnext_char(b',')? {
            let n = self.str_checkname()?;
            self.new_localvar(n)?;
            nvars += 1;
        }
        self.checknext(Token::In)?;
        let line = self.ls.line_number;
        let mut e = Self::new_e();
        let n = self.explist(&mut e)?;
        self.adjust_assign(4, n, &mut e)?;
        self.adjust_local_vars(4)?;
        let fi = self.cur();
        self.mark_to_be_closed(fi);
        self.k_check_stack(3)?;
        self.forbody(base, line, nvars - 4, true)
    }

    fn forstat(&mut self, line: i32) -> R<()> {
        self.enter_block(true);
        self.next()?;
        let varname = self.str_checkname()?;
        match self.ls.t {
            Token::Char(b'=') => self.fornum(varname, line)?,
            Token::Char(b',') | Token::In => self.forlist(varname)?,
            _ => return perr(self.syntax_error("'=' or 'in' expected")),
        }
        self.check_match(Token::End, Token::For, line)?;
        self.leave_block()
    }

    fn test_then_block(&mut self, escapelist: &mut i32) -> R<()> {
        self.next()?;
        let mut v = Self::new_e();
        self.expr(&mut v)?;
        self.checknext(Token::Then)?;
        let jf = if matches!(self.ls.t, Token::Break) {
            // 'if x then break' ?
            let line = self.ls.line_number;
            self.k_go_if_false(&mut v)?;
            self.next()?;
            self.enter_block(false);
            let brkn = self.ls.brkn;
            self.new_goto_entry(brkn, line, v.t)?;
            while self.testnext_char(b';')? {}
            if self.block_follow(false) {
                return self.leave_block();
            }
            self.k_jump()?
        } else {
            self.k_go_if_true(&mut v)?;
            self.enter_block(false);
            v.f
        };
        self.statlist()?;
        self.leave_block()?;
        if matches!(self.ls.t, Token::Else | Token::Elseif) {
            let j = self.k_jump()?;
            self.k_concat(escapelist, j)?;
        }
        self.k_patch_to_here(jf)
    }

    fn ifstat(&mut self, line: i32) -> R<()> {
        let mut escapelist = NO_JUMP;
        self.test_then_block(&mut escapelist)?;
        while matches!(self.ls.t, Token::Elseif) {
            self.test_then_block(&mut escapelist)?;
        }
        if self.testnext(Token::Else)? {
            self.block()?;
        }
        self.check_match(Token::End, Token::If, line)?;
        self.k_patch_to_here(escapelist)
    }

    fn localfunc(&mut self) -> R<()> {
        let mut b = Self::new_e();
        let fvar = self.fsr().nactvar;
        let n = self.str_checkname()?;
        self.new_localvar(n)?;
        self.adjust_local_vars(1)?;
        let line = self.ls.line_number;
        self.body(&mut b, false, line)?;
        let fi = self.cur();
        let pc = self.fsr().pc() as u32;
        if let Some(i) = self.local_debug_info(fi, fvar) {
            self.fs().f.loc_vars[i].start_pc = pc;
        }
        Ok(())
    }

    fn get_local_attribute(&mut self) -> R<u8> {
        if self.testnext_char(b'<')? {
            let ts = self.str_checkname()?;
            let attr = self.ls.host.bytes(ts).to_vec();
            self.checknext_char(b'>')?;
            if attr == b"const" {
                return Ok(RDKCONST);
            } else if attr == b"close" {
                return Ok(RDKTOCLOSE);
            } else {
                let msg = format!("unknown attribute '{}'", String::from_utf8_lossy(&attr));
                return perr(self.sem_err(&msg));
            }
        }
        Ok(VDKREG)
    }

    fn check_to_close(&mut self, level: i32) -> R<()> {
        if level != -1 {
            let fi = self.cur();
            self.mark_to_be_closed(fi);
            let r = self.reglevel(fi, level);
            self.k_code_abc(OpCode::Tbc, r, 0, 0)?;
        }
        Ok(())
    }

    fn localstat(&mut self) -> R<()> {
        let mut toclose = -1;
        let mut nvars = 0;
        let mut vidx;
        let mut e = Self::new_e();
        loop {
            let vname = self.str_checkname()?;
            vidx = self.new_localvar(vname)?;
            let kind = self.get_local_attribute()?;
            let fi = self.cur();
            self.vardesc_mut(fi, vidx).kind = kind;
            if kind == RDKTOCLOSE {
                if toclose != -1 {
                    return perr(self.sem_err("multiple to-be-closed variables in local list"));
                }
                toclose = self.fsr().nactvar + nvars;
            }
            nvars += 1;
            if !self.testnext_char(b',')? {
                break;
            }
        }
        let nexps = if self.testnext_char(b'=')? {
            self.explist(&mut e)?
        } else {
            e.k = ExpKind::Void;
            0
        };
        let fi = self.cur();
        let kind_last = self.vardesc(fi, vidx).kind;
        let cv = if nvars == nexps && kind_last == RDKCONST {
            self.k_exp2const(&e)
        } else {
            None
        };
        if let Some(cv) = cv {
            let vd = self.vardesc_mut(fi, vidx);
            vd.k = cv;
            vd.kind = RDKCTC;
            self.adjust_local_vars(nvars - 1)?;
            self.fs().nactvar += 1;
        } else {
            self.adjust_assign(nvars, nexps, &mut e)?;
            self.adjust_local_vars(nvars)?;
        }
        self.check_to_close(toclose)
    }

    fn funcname(&mut self, v: &mut E<H>) -> R<bool> {
        let mut ismethod = false;
        self.single_var(v)?;
        while self.is_char(b'.') {
            self.fieldsel(v)?;
        }
        if self.is_char(b':') {
            ismethod = true;
            self.fieldsel(v)?;
        }
        Ok(ismethod)
    }

    fn funcstat(&mut self, line: i32) -> R<()> {
        let mut v = Self::new_e();
        let mut b = Self::new_e();
        self.next()?;
        let ismethod = self.funcname(&mut v)?;
        self.body(&mut b, ismethod, line)?;
        self.check_readonly(&v)?;
        self.k_storevar(&mut v, &mut b)?;
        self.k_fix_line(line)
    }

    fn exprstat(&mut self) -> R<()> {
        let mut v = Self::new_e();
        self.suffixedexp(&mut v)?;
        if self.is_char(b'=') || self.is_char(b',') {
            let mut lhs = vec![v];
            self.restassign(&mut lhs, 1)
        } else {
            if v.k != ExpKind::Call {
                return perr(self.syntax_error("syntax error"));
            }
            let pc = v.info as usize;
            let mut i = Instruction(self.fsr().f.code[pc]);
            i.set_c(1);
            self.fs().f.code[pc] = i.0;
            Ok(())
        }
    }

    fn retstat(&mut self) -> R<()> {
        let mut e = Self::new_e();
        let mut first = self.nvarstack_cur();
        let mut nret;
        if self.block_follow(true) || self.is_char(b';') {
            nret = 0;
        } else {
            nret = self.explist(&mut e)?;
            if e.k.has_multret() {
                self.k_set_multret(&mut e)?;
                let insidetbc = self.fsr().bl.last().is_some_and(|b| b.insidetbc);
                if e.k == ExpKind::Call && nret == 1 && !insidetbc {
                    let pc = e.info as usize;
                    let mut i = Instruction(self.fsr().f.code[pc]);
                    i.set_opcode(OpCode::Tailcall);
                    self.fs().f.code[pc] = i.0;
                }
                nret = LUA_MULTRET;
            } else if nret == 1 {
                first = self.k_exp2anyreg(&mut e)?;
            } else {
                self.k_exp2nextreg(&mut e)?;
            }
        }
        self.k_ret(first, nret)?;
        self.testnext_char(b';')?;
        Ok(())
    }

    fn statement(&mut self) -> R<()> {
        let line = self.ls.line_number;
        self.enter_level()?;
        match self.ls.t {
            Token::Char(b';') => self.next()?,
            Token::If => self.ifstat(line)?,
            Token::While => self.whilestat(line)?,
            Token::Do => {
                self.next()?;
                self.block()?;
                self.check_match(Token::End, Token::Do, line)?;
            }
            Token::For => self.forstat(line)?,
            Token::Repeat => self.repeatstat(line)?,
            Token::Function => self.funcstat(line)?,
            Token::Local => {
                self.next()?;
                if self.testnext(Token::Function)? {
                    self.localfunc()?;
                } else {
                    self.localstat()?;
                }
            }
            Token::DbColon => {
                self.next()?;
                let n = self.str_checkname()?;
                self.labelstat(n, line)?;
            }
            Token::Return => {
                self.next()?;
                self.retstat()?;
            }
            Token::Break => self.breakstat()?,
            Token::Goto => {
                self.next()?;
                self.gotostat()?;
            }
            _ => self.exprstat()?,
        }
        let nv = self.nvarstack_cur();
        self.fs().freereg = nv;
        self.leave_level();
        Ok(())
    }
}

struct ConsControl<H: CompilerHost> {
    v: ExpDesc<H::Str>,
    t: ExpDesc<H::Str>,
    nh: i32,
    na: i32,
    tostore: i32,
}

/// `LFIELDS_PER_FLUSH`
const LFIELDS_PER_FLUSH: i32 = 50;
