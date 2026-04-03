//! Cranelift IR helpers — utilities that target real repetition in emit.rs.

use cranelift_codegen::ir::{self, types, InstBuilder, Value, FuncRef, StackSlotData, StackSlotKind};
use cranelift_frontend::FunctionBuilder;

/// Compare two f64 values, return I8 boolean (1 or 0)
pub fn fcmp_to_i64(b: &mut FunctionBuilder, cc: ir::condcodes::FloatCC, a: Value, bv: Value) -> Value {
    b.ins().fcmp(cc, a, bv)
}

/// Compare two integers, return I8 boolean (1 or 0)
pub fn icmp_to_i64(b: &mut FunctionBuilder, cc: ir::condcodes::IntCC, a: Value, bv: Value) -> Value {
    b.ins().icmp(cc, a, bv)
}

/// Call a function and return the first result value
pub fn call_rt(b: &mut FunctionBuilder, func: FuncRef, args: &[Value]) -> Value {
    let call = b.ins().call(func, args);
    b.inst_results(call)[0]
}

/// Call a function with no return value
pub fn call_void(b: &mut FunctionBuilder, func: FuncRef, args: &[Value]) {
    b.ins().call(func, args);
}

/// Allocate a stack slot, store a value, return the slot
pub fn alloc_slot(b: &mut FunctionBuilder, val: Value) -> ir::StackSlot {
    let slot = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 0));
    b.ins().stack_store(val, slot, 0);
    slot
}

/// Load a value from a stack slot with the given type
pub fn load_slot(b: &mut FunctionBuilder, slot: ir::StackSlot, ty: ir::Type) -> Value {
    b.ins().stack_load(ty, slot, 0)
}

/// Boolean AND: both I8 values must be non-zero, returns I8
pub fn bool_and(b: &mut FunctionBuilder, l: Value, r: Value) -> Value {
    b.ins().band(l, r)
}

/// Boolean OR: either I8 value must be non-zero, returns I8
pub fn bool_or(b: &mut FunctionBuilder, l: Value, r: Value) -> Value {
    b.ins().bor(l, r)
}

/// Convert f64 to i64 if needed, pass through i64 unchanged
pub fn ensure_i64(b: &mut FunctionBuilder, val: Value) -> Value {
    if b.func.dfg.value_type(val) == types::F64 {
        b.ins().fcvt_to_sint(types::I64, val)
    } else {
        val
    }
}

/// Leak a string as a null-terminated C string pointer, return as Cranelift I64 constant.
/// Used for compile-time string literals embedded in generated code.
pub fn leak_cstr(b: &mut FunctionBuilder, s: &str) -> Value {
    let leaked = Box::into_raw(format!("{}\0", s).into_boxed_str());
    b.ins().iconst(types::I64, leaked as *const u8 as i64)
}

/// Produce a default/zero value for a Cranelift IR type
pub fn default_for_ir_type(b: &mut FunctionBuilder, ty: ir::Type) -> Value {
    if ty == types::F64 {
        b.ins().f64const(0.0)
    } else {
        b.ins().iconst(ty, 0)
    }
}

