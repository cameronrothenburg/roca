//! Roca-level builder — wraps Cranelift FunctionBuilder into high-level ops.
//! No Cranelift types leak outside this module.

use cranelift_codegen::ir::{
    types, AbiParam, Block, BlockArg, ExtFuncData, ExternalName, FuncRef, InstBuilder,
    SigRef, Signature, Value,
};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, Variable};
use std::collections::HashMap;

/// Cranelift type tag for passing around in this module.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClifType {
    I64,
    F64,
    I8,
}

impl ClifType {
    pub fn to_clif(self) -> cranelift_codegen::ir::Type {
        match self {
            ClifType::I64 => types::I64,
            ClifType::F64 => types::F64,
            ClifType::I8 => types::I8,
        }
    }
}

/// Owns the builder state for one function compilation.
pub struct RocaBuilder<'a> {
    pub builder: FunctionBuilder<'a>,
    /// Maps variable name → (Variable, ClifType)
    vars: HashMap<String, (Variable, ClifType)>,
    /// Whether the current block has been terminated.
    terminated: bool,
    /// Stack of (loop_header, loop_exit) blocks for break
    loop_stack: Vec<(Block, Block)>,
    pub func_sigs: HashMap<String, (Vec<ClifType>, ClifType)>,
}

impl<'a> RocaBuilder<'a> {
    pub fn new(
        builder: FunctionBuilder<'a>,
        func_sigs: HashMap<String, (Vec<ClifType>, ClifType)>,
    ) -> Self {
        Self {
            builder,
            vars: HashMap::new(),
            terminated: false,
            loop_stack: Vec::new(),
            func_sigs,
        }
    }

    fn alloc_var(&mut self, ty: ClifType) -> Variable {
        self.builder.declare_var(ty.to_clif())
    }

    /// Returns true if the current block already has a terminator.
    pub fn is_terminated(&self) -> bool {
        self.terminated
    }

    fn set_terminated(&mut self) {
        self.terminated = true;
    }

    fn clear_terminated(&mut self) {
        self.terminated = false;
    }

    // ── Literals ─────────────────────────────────────────────────────────────

    pub fn int_val(&mut self, n: i64) -> Value {
        self.builder.ins().iconst(types::I64, n)
    }

    pub fn number(&mut self, n: f64) -> Value {
        self.builder.ins().f64const(n)
    }

    pub fn bool_val(&mut self, b: bool) -> Value {
        self.builder.ins().iconst(types::I8, if b { 1 } else { 0 })
    }

    // ── Variables ────────────────────────────────────────────────────────────

    pub fn var_declare(&mut self, name: &str, ty: ClifType, val: Value) {
        let v = self.alloc_var(ty);
        self.vars.insert(name.to_string(), (v, ty));
        self.builder.def_var(v, val);
    }

    pub fn var_set(&mut self, name: &str, val: Value) {
        if let Some((v, _ty)) = self.vars.get(name).copied() {
            self.builder.def_var(v, val);
        } else {
            panic!("undefined variable: {name}");
        }
    }

    pub fn var_get(&mut self, name: &str) -> Value {
        let (v, _) = *self.vars.get(name).unwrap_or_else(|| panic!("undefined variable: {name}"));
        self.builder.use_var(v)
    }

    /// Declare a function parameter (already has a Value from block params).
    pub fn param_declare(&mut self, name: &str, ty: ClifType, val: Value) {
        let v = self.alloc_var(ty);
        self.vars.insert(name.to_string(), (v, ty));
        self.builder.def_var(v, val);
    }

    // ── Binary ops ───────────────────────────────────────────────────────────

    pub fn add(&mut self, l: Value, r: Value, ty: ClifType) -> Value {
        match ty {
            ClifType::I64 => self.builder.ins().iadd(l, r),
            ClifType::F64 => self.builder.ins().fadd(l, r),
            ClifType::I8  => self.builder.ins().iadd(l, r),
        }
    }

    pub fn sub(&mut self, l: Value, r: Value, ty: ClifType) -> Value {
        match ty {
            ClifType::I64 => self.builder.ins().isub(l, r),
            ClifType::F64 => self.builder.ins().fsub(l, r),
            ClifType::I8  => self.builder.ins().isub(l, r),
        }
    }

    pub fn mul(&mut self, l: Value, r: Value, ty: ClifType) -> Value {
        match ty {
            ClifType::I64 => self.builder.ins().imul(l, r),
            ClifType::F64 => self.builder.ins().fmul(l, r),
            ClifType::I8  => self.builder.ins().imul(l, r),
        }
    }

    pub fn div(&mut self, l: Value, r: Value, ty: ClifType) -> Value {
        match ty {
            ClifType::I64 => self.builder.ins().sdiv(l, r),
            ClifType::F64 => self.builder.ins().fdiv(l, r),
            ClifType::I8  => self.builder.ins().sdiv(l, r),
        }
    }

    pub fn mod_op(&mut self, l: Value, r: Value) -> Value {
        self.builder.ins().srem(l, r)
    }

    pub fn eq(&mut self, l: Value, r: Value, ty: ClifType) -> Value {
        match ty {
            ClifType::I64 | ClifType::I8 => {
                self.builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, l, r)
            }
            ClifType::F64 => {
                self.builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::Equal, l, r)
            }
        }
    }

    pub fn ne(&mut self, l: Value, r: Value, ty: ClifType) -> Value {
        match ty {
            ClifType::I64 | ClifType::I8 => {
                self.builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, l, r)
            }
            ClifType::F64 => {
                self.builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::NotEqual, l, r)
            }
        }
    }

    pub fn lt(&mut self, l: Value, r: Value, ty: ClifType) -> Value {
        match ty {
            ClifType::I64 | ClifType::I8 => {
                self.builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedLessThan, l, r)
            }
            ClifType::F64 => {
                self.builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::LessThan, l, r)
            }
        }
    }

    pub fn gt(&mut self, l: Value, r: Value, ty: ClifType) -> Value {
        match ty {
            ClifType::I64 | ClifType::I8 => {
                self.builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThan, l, r)
            }
            ClifType::F64 => {
                self.builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::GreaterThan, l, r)
            }
        }
    }

    pub fn le(&mut self, l: Value, r: Value, ty: ClifType) -> Value {
        match ty {
            ClifType::I64 | ClifType::I8 => {
                self.builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedLessThanOrEqual, l, r)
            }
            ClifType::F64 => {
                self.builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::LessThanOrEqual, l, r)
            }
        }
    }

    pub fn ge(&mut self, l: Value, r: Value, ty: ClifType) -> Value {
        match ty {
            ClifType::I64 | ClifType::I8 => {
                self.builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThanOrEqual, l, r)
            }
            ClifType::F64 => {
                self.builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::GreaterThanOrEqual, l, r)
            }
        }
    }

    pub fn and(&mut self, l: Value, r: Value) -> Value {
        self.builder.ins().band(l, r)
    }

    pub fn or(&mut self, l: Value, r: Value) -> Value {
        self.builder.ins().bor(l, r)
    }

    pub fn neg(&mut self, v: Value, ty: ClifType) -> Value {
        match ty {
            ClifType::I64 | ClifType::I8 => self.builder.ins().ineg(v),
            ClifType::F64 => self.builder.ins().fneg(v),
        }
    }

    pub fn not(&mut self, v: Value) -> Value {
        let zero = self.builder.ins().iconst(types::I8, 0);
        self.builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, v, zero)
    }

    // ── Type conversion ───────────────────────────────────────────────────────

    pub fn widen_to_i64(&mut self, v: Value) -> Value {
        self.builder.ins().uextend(types::I64, v)
    }

    pub fn narrow_to_i8(&mut self, v: Value) -> Value {
        self.builder.ins().ireduce(types::I8, v)
    }

    pub fn i64_to_f64(&mut self, v: Value) -> Value {
        self.builder.ins().fcvt_from_sint(types::F64, v)
    }

    pub fn f64_to_i64(&mut self, v: Value) -> Value {
        self.builder.ins().fcvt_to_sint_sat(types::I64, v)
    }

    // ── Function calls ───────────────────────────────────────────────────────

    /// Call using a pre-imported FuncRef.
    pub fn call_with_ref(
        &mut self,
        func_ref: FuncRef,
        args: &[Value],
    ) -> Option<Value> {
        let inst = self.builder.ins().call(func_ref, args);
        let results = self.builder.inst_results(inst);
        results.first().copied()
    }

    // ── Control flow ─────────────────────────────────────────────────────────

    fn emit_brif(&mut self, cond_i8: Value, then_block: Block, else_block: Block) {
        let empty: &[BlockArg] = &[];
        self.builder.ins().brif(cond_i8, then_block, empty, else_block, empty);
        self.set_terminated();
    }

    fn emit_jump(&mut self, target: Block) {
        let empty: &[BlockArg] = &[];
        self.builder.ins().jump(target, empty);
        self.set_terminated();
    }

    fn switch_to(&mut self, block: Block) {
        self.builder.switch_to_block(block);
        self.clear_terminated();
    }

    /// Push a loop frame onto the stack.
    pub fn loop_stack_push(&mut self, header: Block, exit: Block) {
        self.loop_stack.push((header, exit));
    }

    /// Pop the current loop frame.
    pub fn loop_stack_pop(&mut self) {
        self.loop_stack.pop().expect("loop_stack_pop without push");
    }

    /// Jump to the loop exit (break).
    pub fn break_loop(&mut self) {
        let (_, exit) = *self.loop_stack.last().expect("break outside loop");
        self.emit_jump(exit);
    }

    /// Return from function.
    pub fn return_val(&mut self, val: Value) {
        self.builder.ins().return_(&[val]);
        self.set_terminated();
    }

    pub fn finalize(self) {
        self.builder.finalize();
    }

    // ── Helpers for compiler.rs ───────────────────────────────────────────────

    pub fn create_block(&mut self) -> Block {
        self.builder.create_block()
    }

    pub fn seal_block(&mut self, block: Block) {
        self.builder.seal_block(block);
    }

    /// Branch on i8 condition; also handles I64 by narrowing.
    pub fn brif_to(&mut self, cond_i8: Value, then_block: Block, else_block: Block) {
        self.emit_brif(cond_i8, then_block, else_block);
    }

    pub fn jump_to(&mut self, target: Block) {
        if !self.terminated {
            self.emit_jump(target);
        }
    }

    pub fn switch_block(&mut self, block: Block) {
        self.switch_to(block);
    }
}
