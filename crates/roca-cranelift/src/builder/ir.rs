//! IrBuilder — wraps Cranelift FunctionBuilder with Roca-level methods.
//! Callers never touch raw Cranelift types directly.

use cranelift_codegen::ir::{self, types, Value, FuncRef, InstBuilder, StackSlot, Block};
use cranelift_frontend::FunctionBuilder;
use roca_types::RocaType;
use crate::cranelift_type::CraneliftType;

/// Block handle — wraps cranelift_codegen::ir::Block.
#[derive(Clone, Copy, Debug)]
pub struct BlockId(pub Block);

/// Stack variable handle — wraps cranelift_codegen::ir::StackSlot.
#[derive(Clone, Copy, Debug)]
pub struct VarSlot(pub StackSlot);

/// Roca-level IR builder. Wraps Cranelift FunctionBuilder with domain methods.
///
/// The lifetime `'a` borrows the FunctionBuilder which itself borrows the
/// Cranelift context. FunctionCompiler creates this inside a callback so
/// lifetimes are scoped naturally.
pub struct IrBuilder<'a, 'b: 'a> {
    pub(crate) b: &'a mut FunctionBuilder<'b>,
}

// ─── Constants ────────────────────────────────────────

impl<'a, 'b: 'a> IrBuilder<'a, 'b> {
    pub fn const_number(&mut self, n: f64) -> Value {
        self.b.ins().f64const(n)
    }

    pub fn const_bool(&mut self, v: bool) -> Value {
        self.b.ins().iconst(types::I8, if v { 1 } else { 0 })
    }

    pub fn const_i64(&mut self, n: i64) -> Value {
        self.b.ins().iconst(types::I64, n)
    }

    pub fn null(&mut self) -> Value {
        self.b.ins().iconst(types::I64, 0)
    }

    pub fn leak_cstr(&mut self, s: &str) -> Value {
        crate::helpers::leak_cstr(self.b, s)
    }

    pub fn default_for(&mut self, ty: &RocaType) -> Value {
        ty.default_value(self.b)
    }
}

// ─── Arithmetic ───────────────────────────────────────

impl<'a, 'b: 'a> IrBuilder<'a, 'b> {
    pub fn add(&mut self, a: Value, bv: Value) -> Value { self.b.ins().fadd(a, bv) }
    pub fn sub(&mut self, a: Value, bv: Value) -> Value { self.b.ins().fsub(a, bv) }
    pub fn mul(&mut self, a: Value, bv: Value) -> Value { self.b.ins().fmul(a, bv) }
    pub fn div(&mut self, a: Value, bv: Value) -> Value { self.b.ins().fdiv(a, bv) }
    pub fn iadd(&mut self, a: Value, bv: Value) -> Value { self.b.ins().iadd(a, bv) }
    pub fn isub(&mut self, a: Value, bv: Value) -> Value { self.b.ins().isub(a, bv) }
}

// ─── Comparisons (return I64 0/1) ─────────────────────

impl<'a, 'b: 'a> IrBuilder<'a, 'b> {
    pub fn f_eq(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::fcmp_to_i64(self.b, ir::condcodes::FloatCC::Equal, a, bv)
    }
    pub fn f_ne(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::fcmp_to_i64(self.b, ir::condcodes::FloatCC::NotEqual, a, bv)
    }
    pub fn f_lt(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::fcmp_to_i64(self.b, ir::condcodes::FloatCC::LessThan, a, bv)
    }
    pub fn f_gt(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::fcmp_to_i64(self.b, ir::condcodes::FloatCC::GreaterThan, a, bv)
    }
    pub fn f_le(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::fcmp_to_i64(self.b, ir::condcodes::FloatCC::LessThanOrEqual, a, bv)
    }
    pub fn f_ge(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::fcmp_to_i64(self.b, ir::condcodes::FloatCC::GreaterThanOrEqual, a, bv)
    }
    pub fn i_eq(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::icmp_to_i64(self.b, ir::condcodes::IntCC::Equal, a, bv)
    }
    pub fn i_ne(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::icmp_to_i64(self.b, ir::condcodes::IntCC::NotEqual, a, bv)
    }
    pub fn i_slt(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::icmp_to_i64(self.b, ir::condcodes::IntCC::SignedLessThan, a, bv)
    }
    pub fn i_sgt(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::icmp_to_i64(self.b, ir::condcodes::IntCC::SignedGreaterThan, a, bv)
    }
}

// ─── Logic ────────────────────────────────────────────

impl<'a, 'b: 'a> IrBuilder<'a, 'b> {
    pub fn bool_and(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::bool_and(self.b, a, bv)
    }
    pub fn bool_or(&mut self, a: Value, bv: Value) -> Value {
        crate::helpers::bool_or(self.b, a, bv)
    }
    pub fn extend_bool(&mut self, val: Value) -> Value {
        self.b.ins().uextend(types::I64, val)
    }
}

// ─── Type queries + conversions ───────────────────────

impl<'a, 'b: 'a> IrBuilder<'a, 'b> {
    pub fn is_number(&self, val: Value) -> bool {
        self.b.func.dfg.value_type(val) == types::F64
    }

    pub fn value_ir_type(&self, val: Value) -> ir::Type {
        self.b.func.dfg.value_type(val)
    }

    pub fn to_i64(&mut self, val: Value) -> Value {
        crate::helpers::ensure_i64(self.b, val)
    }

    pub fn f64_to_i64(&mut self, val: Value) -> Value {
        self.b.ins().fcvt_to_sint(types::I64, val)
    }

    pub fn i64_to_f64(&mut self, val: Value) -> Value {
        self.b.ins().fcvt_from_sint(types::F64, val)
    }
}

// ─── Variable management ──────────────────────────────

impl<'a, 'b: 'a> IrBuilder<'a, 'b> {
    pub fn alloc_var(&mut self, val: Value) -> VarSlot {
        VarSlot(crate::helpers::alloc_slot(self.b, val))
    }

    pub fn load_var(&mut self, slot: VarSlot, ty: &RocaType) -> Value {
        crate::helpers::load_slot(self.b, slot.0, ty.to_cranelift())
    }

    pub fn store_var(&mut self, slot: VarSlot, val: Value) {
        self.b.ins().stack_store(val, slot.0, 0);
    }
}

// ─── Control flow ─────────────────────────────────────

impl<'a, 'b: 'a> IrBuilder<'a, 'b> {
    pub fn create_block(&mut self) -> BlockId {
        BlockId(self.b.create_block())
    }

    pub fn switch_to(&mut self, block: BlockId) {
        self.b.switch_to_block(block.0);
    }

    pub fn seal(&mut self, block: BlockId) {
        self.b.seal_block(block.0);
    }

    pub fn jump(&mut self, target: BlockId) {
        self.b.ins().jump(target.0, &[]);
    }

    pub fn jump_with(&mut self, target: BlockId, val: Value) {
        self.b.ins().jump(target.0, &[ir::BlockArg::Value(val)]);
    }

    pub fn brif(&mut self, cond: Value, then_block: BlockId, else_block: BlockId) {
        self.b.ins().brif(cond, then_block.0, &[], else_block.0, &[]);
    }

    pub fn append_block_param(&mut self, block: BlockId, ty: &RocaType) {
        self.b.append_block_param(block.0, ty.to_cranelift());
    }

    pub fn block_param(&self, block: BlockId, idx: usize) -> Value {
        self.b.block_params(block.0)[idx]
    }

    #[allow(dead_code)]
    pub fn block_params(&self, block: BlockId) -> Vec<Value> {
        self.b.block_params(block.0).to_vec()
    }
}

// ─── Function calls ───────────────────────────────────

impl<'a, 'b: 'a> IrBuilder<'a, 'b> {
    pub fn call(&mut self, func: FuncRef, args: &[Value]) -> Value {
        crate::helpers::call_rt(self.b, func, args)
    }

    pub fn call_void(&mut self, func: FuncRef, args: &[Value]) {
        crate::helpers::call_void(self.b, func, args);
    }

    pub fn call_multi(&mut self, func: FuncRef, args: &[Value]) -> Vec<Value> {
        let call = self.b.ins().call(func, args);
        self.b.inst_results(call).to_vec()
    }

    /// Get a function's address as a pointer value (for closures).
    pub fn func_addr(&mut self, func: FuncRef) -> Value {
        self.b.ins().func_addr(types::I64, func)
    }

    /// Build a closure call signature: N f64 params → f64 return.
    pub fn closure_signature(&mut self, param_count: usize) -> ir::SigRef {
        let mut sig = self.b.func.signature.clone();
        sig.params.clear();
        sig.returns.clear();
        for _ in 0..param_count {
            sig.params.push(ir::AbiParam::new(types::F64));
        }
        sig.returns.push(ir::AbiParam::new(types::F64));
        self.b.import_signature(sig)
    }

    /// Indirect call through a function pointer (for closures).
    pub fn call_indirect(&mut self, sig: ir::SigRef, ptr: Value, args: &[Value]) -> Vec<Value> {
        let call = self.b.ins().call_indirect(sig, ptr, args);
        self.b.inst_results(call).to_vec()
    }
}

// ─── Returns ──────────────────────────────────────────

impl<'a, 'b: 'a> IrBuilder<'a, 'b> {
    pub fn ret(&mut self, val: Value) {
        self.b.ins().return_(&[val]);
    }

    pub fn ret_with_err(&mut self, val: Value, err: Value) {
        self.b.ins().return_(&[val, err]);
    }

    pub fn trap(&mut self, code: u8) {
        self.b.ins().trap(ir::TrapCode::unwrap_user(code));
    }
}

// ─── Raw access (escape hatch for migration) ─────────

impl<'a, 'b: 'a> IrBuilder<'a, 'b> {
    /// Access the raw FunctionBuilder. Use sparingly — prefer typed methods.
    pub fn raw(&mut self) -> &mut FunctionBuilder<'b> {
        self.b
    }
}
