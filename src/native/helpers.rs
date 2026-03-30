//! Cranelift IR builder helpers — wraps verbose Cranelift API into clean one-liners.
//! Same philosophy as src/emit/ast_helpers.rs but for native code generation.

use cranelift_codegen::ir::{self, types, InstBuilder, Value, Block, FuncRef};
use cranelift_frontend::FunctionBuilder;

// ─── Constants ─────────────────────────────────────────

pub fn const_i64(b: &mut FunctionBuilder, val: i64) -> Value {
    b.ins().iconst(types::I64, val)
}

pub fn const_i8(b: &mut FunctionBuilder, val: i8) -> Value {
    b.ins().iconst(types::I8, val as i64)
}

pub fn const_f64(b: &mut FunctionBuilder, val: f64) -> Value {
    b.ins().f64const(val)
}

pub fn const_bool(b: &mut FunctionBuilder, val: bool) -> Value {
    b.ins().iconst(types::I8, if val { 1 } else { 0 })
}

pub fn null_ptr(b: &mut FunctionBuilder) -> Value {
    b.ins().iconst(types::I64, 0)
}

// ─── Arithmetic ────────────────────────────────────────

pub fn add_f64(b: &mut FunctionBuilder, a: Value, b_val: Value) -> Value {
    b.ins().fadd(a, b_val)
}

pub fn sub_f64(b: &mut FunctionBuilder, a: Value, b_val: Value) -> Value {
    b.ins().fsub(a, b_val)
}

pub fn mul_f64(b: &mut FunctionBuilder, a: Value, b_val: Value) -> Value {
    b.ins().fmul(a, b_val)
}

pub fn div_f64(b: &mut FunctionBuilder, a: Value, b_val: Value) -> Value {
    b.ins().fdiv(a, b_val)
}

// ─── Comparison ────────────────────────────────────────

pub fn eq_f64(b: &mut FunctionBuilder, a: Value, b_val: Value) -> Value {
    b.ins().fcmp(ir::condcodes::FloatCC::Equal, a, b_val)
}

pub fn lt_f64(b: &mut FunctionBuilder, a: Value, b_val: Value) -> Value {
    b.ins().fcmp(ir::condcodes::FloatCC::LessThan, a, b_val)
}

pub fn gt_f64(b: &mut FunctionBuilder, a: Value, b_val: Value) -> Value {
    b.ins().fcmp(ir::condcodes::FloatCC::GreaterThan, a, b_val)
}

pub fn eq_i64(b: &mut FunctionBuilder, a: Value, b_val: Value) -> Value {
    b.ins().icmp(ir::condcodes::IntCC::Equal, a, b_val)
}

pub fn eq_i8(b: &mut FunctionBuilder, a: Value, b_val: Value) -> Value {
    b.ins().icmp(ir::condcodes::IntCC::Equal, a, b_val)
}

// ─── Control flow ──────────────────────────────────────

pub fn create_block(b: &mut FunctionBuilder) -> Block {
    b.create_block()
}

pub fn switch_to(b: &mut FunctionBuilder, block: Block) {
    b.switch_to_block(block);
}

pub fn seal(b: &mut FunctionBuilder, block: Block) {
    b.seal_block(block);
}

pub fn jump(b: &mut FunctionBuilder, target: Block) {
    b.ins().jump(target, &[]);
}

pub fn branch_if(b: &mut FunctionBuilder, cond: Value, then_block: Block, else_block: Block) {
    b.ins().brif(cond, then_block, &[], else_block, &[]);
}

pub fn return_val(b: &mut FunctionBuilder, vals: &[Value]) {
    b.ins().return_(vals);
}

// ─── Function calls ────────────────────────────────────

pub fn call_fn(b: &mut FunctionBuilder, func: FuncRef, args: &[Value]) -> Value {
    let call = b.ins().call(func, args);
    b.inst_results(call)[0]
}

pub fn call_fn_void(b: &mut FunctionBuilder, func: FuncRef, args: &[Value]) {
    b.ins().call(func, args);
}

/// Call a function that returns two values (value, err_tag)
pub fn call_fn_result(b: &mut FunctionBuilder, func: FuncRef, args: &[Value]) -> (Value, Value) {
    let call = b.ins().call(func, args);
    let results = b.inst_results(call);
    (results[0], results[1])
}

// ─── Memory ────────────────────────────────────────────

pub fn load_i64(b: &mut FunctionBuilder, addr: Value, offset: i32) -> Value {
    b.ins().load(types::I64, ir::MemFlags::new(), addr, offset)
}

pub fn store_i64(b: &mut FunctionBuilder, val: Value, addr: Value, offset: i32) {
    b.ins().store(ir::MemFlags::new(), val, addr, offset);
}
