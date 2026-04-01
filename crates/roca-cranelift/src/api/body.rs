//! Body — the Roca scope manager.
//! Every Roca statement and expression is a method on Body.
//! Owns IrBuilder + EmitCtx. Cleanup is automatic.

use std::collections::HashMap;
use cranelift_codegen::ir::{self, types, InstBuilder, Value};
use cranelift_frontend::FunctionBuilder;

use roca_ast::{self as roca, BinOp, crash::CrashHandlerKind};
use roca_types::RocaType;
use crate::builder::{IrBuilder, BlockId, VarSlot};
use crate::context::{EmitCtx, StructLayout, VarInfo};
use crate::cranelift_type::CraneliftType;
use crate::emit_helpers::{
    FreeRefs, emit_scope_cleanup, emit_free_by_kind, emit_loop_body_cleanup,
    infer_kind as ast_infer_kind,
};
use crate::helpers::default_for_ir_type;

// ─── Variable References ──────────────────────────────

/// Immutable variable reference. Cannot be reassigned.
#[derive(Clone)]
pub struct ConstRef {
    pub(crate) name: String,
    pub(crate) slot: VarSlot,
    pub(crate) roca_type: RocaType,
}

/// Mutable variable reference. Can be reassigned via `body.assign()`.
#[derive(Clone)]
pub struct MutRef {
    pub(crate) name: String,
    pub(crate) slot: VarSlot,
    pub(crate) roca_type: RocaType,
}

/// Trait for reading any variable (const or let).
pub trait VarRef {
    fn name(&self) -> &str;
    fn slot(&self) -> VarSlot;
    fn roca_type(&self) -> &RocaType;
}

impl VarRef for ConstRef {
    fn name(&self) -> &str { &self.name }
    fn slot(&self) -> VarSlot { self.slot }
    fn roca_type(&self) -> &RocaType { &self.roca_type }
}

impl VarRef for MutRef {
    fn name(&self) -> &str { &self.name }
    fn slot(&self) -> VarSlot { self.slot }
    fn roca_type(&self) -> &RocaType { &self.roca_type }
}

/// String interpolation part.
pub enum StringPart {
    Lit(String),
    Expr(Value),
}

// ─── Body ─────────────────────────────────────────────

/// Roca scope — wraps IR builder + context. Every Roca construct is a method.
pub struct Body<'a, 'b: 'a, 'c> {
    pub ir: &'c mut IrBuilder<'a, 'b>,
    pub ctx: EmitCtx,
    pub returned: bool,
}

// ─── Expressions ──────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Number literal.
    pub fn number(&mut self, n: f64) -> Value {
        self.ir.const_number(n)
    }

    /// Bool literal.
    pub fn bool_val(&mut self, v: bool) -> Value {
        self.ir.const_bool(v)
    }

    /// String literal (RC-allocated).
    pub fn string(&mut self, s: &str) -> Value {
        let static_ptr = self.ir.leak_cstr(s);
        if let Some(&f) = self.ctx.get_func("__string_new") {
            self.ir.call(f, &[static_ptr])
        } else {
            static_ptr
        }
    }

    /// Null pointer.
    pub fn null(&mut self) -> Value {
        self.ir.null()
    }

    /// Load self reference (in struct methods).
    pub fn self_ref(&mut self) -> Value {
        if let Some(var) = self.ctx.get_var("self") {
            self.ir.raw().ins().stack_load(var.cranelift_type, var.slot, 0)
        } else {
            self.ir.null()
        }
    }

    /// Load a variable by name.
    pub fn var(&mut self, name: &str) -> Value {
        if let Some(var) = self.ctx.get_var(name) {
            self.ir.raw().ins().stack_load(var.cranelift_type, var.slot, 0)
        } else {
            self.ir.null()
        }
    }

    /// Read a typed variable reference (ConstRef or MutRef).
    pub fn read(&mut self, var: &impl VarRef) -> Value {
        self.ir.load_var(var.slot(), var.roca_type())
    }

    /// Boolean NOT.
    pub fn not(&mut self, val: Value) -> Value {
        let zero = self.ir.null();
        self.ir.i_eq(val, zero)
    }
}

// ─── Binary Operations ────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Generic binary operation — dispatches based on operator and value types.
    pub fn binop(&mut self, op: &BinOp, l: Value, r: Value) -> Value {
        let is_float = self.ir.is_number(l);
        match op {
            BinOp::Add if is_float => self.ir.add(l, r),
            BinOp::Add => {
                if let Some(f) = self.ctx.get_func("__string_concat") { self.ir.call(*f, &[l, r]) }
                else { self.ir.iadd(l, r) }
            }
            BinOp::Sub => self.ir.sub(l, r),
            BinOp::Mul => self.ir.mul(l, r),
            BinOp::Div => self.ir.div(l, r),
            BinOp::Eq if is_float => self.ir.f_eq(l, r),
            BinOp::Eq => {
                if let Some(f) = self.ctx.get_func("__string_eq") {
                    let result = self.ir.call(*f, &[l, r]);
                    self.ir.extend_bool(result)
                } else {
                    self.ir.i_eq(l, r)
                }
            }
            BinOp::Neq if is_float => self.ir.f_ne(l, r),
            BinOp::Neq => {
                if let Some(f) = self.ctx.get_func("__string_eq") {
                    let eq = self.ir.call(*f, &[l, r]);
                    let ext = self.ir.extend_bool(eq);
                    let one = self.ir.const_i64(1);
                    self.ir.isub(one, ext)
                } else {
                    self.ir.i_ne(l, r)
                }
            }
            BinOp::Lt => self.ir.f_lt(l, r),
            BinOp::Gt => self.ir.f_gt(l, r),
            BinOp::Lte => self.ir.f_le(l, r),
            BinOp::Gte => self.ir.f_ge(l, r),
            BinOp::And => self.ir.bool_and(l, r),
            BinOp::Or => self.ir.bool_or(l, r),
        }
    }

    /// Number addition.
    pub fn add(&mut self, a: Value, b: Value) -> Value { self.ir.add(a, b) }
    pub fn sub(&mut self, a: Value, b: Value) -> Value { self.ir.sub(a, b) }
    pub fn mul(&mut self, a: Value, b: Value) -> Value { self.ir.mul(a, b) }
    pub fn div(&mut self, a: Value, b: Value) -> Value { self.ir.div(a, b) }

    /// String concatenation.
    pub fn string_concat(&mut self, a: Value, b: Value) -> Value {
        if let Some(f) = self.ctx.get_func("__string_concat") {
            self.ir.call(*f, &[a, b])
        } else {
            a
        }
    }

    /// Equality (works for number, string, bool).
    pub fn eq(&mut self, a: Value, b: Value) -> Value {
        self.binop(&BinOp::Eq, a, b)
    }

    pub fn neq(&mut self, a: Value, b: Value) -> Value { self.binop(&BinOp::Neq, a, b) }
    pub fn lt(&mut self, a: Value, b: Value) -> Value { self.binop(&BinOp::Lt, a, b) }
    pub fn gt(&mut self, a: Value, b: Value) -> Value { self.binop(&BinOp::Gt, a, b) }
    pub fn lte(&mut self, a: Value, b: Value) -> Value { self.binop(&BinOp::Lte, a, b) }
    pub fn gte(&mut self, a: Value, b: Value) -> Value { self.binop(&BinOp::Gte, a, b) }
    pub fn and(&mut self, a: Value, b: Value) -> Value { self.ir.bool_and(a, b) }
    pub fn or(&mut self, a: Value, b: Value) -> Value { self.ir.bool_or(a, b) }
}

// ─── Variables ────────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Bind an immutable variable. Returns a ConstRef that can only be read.
    pub fn const_var(&mut self, name: &str, val: Value) -> ConstRef {
        let roca_type = self.infer_roca_type(val);
        let cl_type = roca_type.to_cranelift();
        let slot = self.ir.alloc_var(val);
        self.ctx.set_var_kind(name.to_string(), slot.0, cl_type, roca_type.clone());
        ConstRef { name: name.to_string(), slot, roca_type }
    }

    /// Bind a mutable variable. Returns a MutRef that can be reassigned.
    pub fn let_var(&mut self, name: &str, val: Value) -> MutRef {
        let roca_type = self.infer_roca_type(val);
        let cl_type = roca_type.to_cranelift();
        let slot = self.ir.alloc_var(val);
        self.ctx.set_var_kind(name.to_string(), slot.0, cl_type, roca_type.clone());
        MutRef { name: name.to_string(), slot, roca_type }
    }

    /// Reassign a mutable variable. Frees the old heap value if needed.
    pub fn assign(&mut self, var: &MutRef, val: Value) {
        if let Some(old) = self.ctx.get_var(&var.name) {
            if old.is_heap {
                let slot = old.slot;
                let cl_type = old.cranelift_type;
                let kind = old.kind.clone();
                let refs = FreeRefs::from_ctx(&self.ctx);
                emit_free_by_kind(&mut self.ir, slot, cl_type, kind, &refs);
            }
        }
        self.ir.store_var(var.slot, val);
    }

    /// Infer RocaType from a Cranelift Value's IR type.
    fn infer_roca_type(&self, val: Value) -> RocaType {
        if self.ir.is_number(val) {
            RocaType::Number
        } else if self.ir.value_ir_type(val) == types::I8 {
            RocaType::Bool
        } else {
            RocaType::Unknown
        }
    }
}

// ─── Collections ──────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Create an array from elements.
    pub fn array(&mut self, elements: &[Value]) -> Value {
        let arr = if let Some(f) = self.ctx.get_func("__array_new") {
            self.ir.call(*f, &[])
        } else {
            return self.ir.null();
        };
        for &elem in elements {
            crate::emit_helpers::emit_array_push(&mut self.ir, arr, elem, &mut self.ctx);
        }
        arr
    }

    /// Array index access.
    pub fn index(&mut self, arr: Value, idx: Value) -> Value {
        let idx_i64 = self.ir.to_i64(idx);
        if let Some(f) = self.ctx.get_func("__array_get_f64") {
            self.ir.call(*f, &[arr, idx_i64])
        } else {
            self.ir.const_number(0.0)
        }
    }

    /// Struct field access.
    pub fn field_access(&mut self, obj: Value, field: &str) -> Value {
        // For now, delegate to struct_get_ptr. Full field resolution comes with Struct builder.
        let idx = self.ir.const_i64(0); // placeholder
        if let Some(f) = self.ctx.get_func("__struct_get_ptr") {
            self.ir.call(*f, &[obj, idx])
        } else {
            self.ir.null()
        }
    }

    /// Struct literal construction.
    pub fn struct_lit(&mut self, name: &str, fields: &[(&str, Value)]) -> Value {
        let num_fields = self.ir.const_i64(fields.len() as i64);
        let ptr = if let Some(f) = self.ctx.get_func("__struct_alloc") {
            self.ir.call(*f, &[num_fields])
        } else {
            return self.ir.null();
        };
        for (i, &(_, val)) in fields.iter().enumerate() {
            let idx = self.ir.const_i64(i as i64);
            crate::emit_helpers::emit_struct_set(&mut self.ir, ptr, idx, val, &mut self.ctx);
        }
        ptr
    }

    /// Enum variant construction.
    pub fn enum_variant(&mut self, _enum_name: &str, variant: &str, args: &[Value]) -> Value {
        let num_fields = self.ir.const_i64((1 + args.len()) as i64);
        let ptr = if let Some(f) = self.ctx.get_func("__struct_alloc") {
            self.ir.call(*f, &[num_fields])
        } else {
            return self.ir.null();
        };
        // Field 0: variant name tag
        let tag = self.string(variant);
        let zero = self.ir.const_i64(0);
        if let Some(&f) = self.ctx.get_func("__struct_set_ptr") {
            self.ir.call_void(f, &[ptr, zero, tag]);
        }
        // Fields 1..n: args
        for (i, &arg) in args.iter().enumerate() {
            let idx = self.ir.const_i64((i + 1) as i64);
            crate::emit_helpers::emit_struct_set(&mut self.ir, ptr, idx, arg, &mut self.ctx);
        }
        ptr
    }

    /// String interpolation from parts.
    pub fn string_interp(&mut self, parts: &[StringPart]) -> Value {
        let concat = self.ctx.get_func("__string_concat").copied();
        let to_str = self.ctx.get_func("__string_from_f64").copied();
        let string_new = self.ctx.get_func("__string_new").copied();

        let mut result: Option<Value> = None;
        for part in parts {
            let val = match part {
                StringPart::Lit(s) => {
                    let static_ptr = self.ir.leak_cstr(s);
                    if let Some(f) = string_new { self.ir.call(f, &[static_ptr]) } else { static_ptr }
                }
                StringPart::Expr(v) => {
                    if self.ir.is_number(*v) {
                        if let Some(f) = to_str { self.ir.call(f, &[*v]) } else { *v }
                    } else {
                        *v
                    }
                }
            };
            result = Some(match result {
                None => val,
                Some(acc) => {
                    if let Some(f) = concat { self.ir.call(f, &[acc, val]) } else { val }
                }
            });
        }
        result.unwrap_or_else(|| self.ir.null())
    }
}

// ─── Function Calls ───────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Call a function by name.
    pub fn call(&mut self, name: &str, args: &[Value]) -> Value {
        if let Some(&func_ref) = self.ctx.get_func(name) {
            let results = self.ir.call_multi(func_ref, args);
            if !results.is_empty() { results[0] } else { self.ir.null() }
        } else {
            self.ir.null()
        }
    }

    /// Call a static/contract method: Type.method(args).
    pub fn static_call(&mut self, type_name: &str, method: &str, args: &[Value]) -> Value {
        let qualified = format!("{}.{}", type_name, method);
        self.call(&qualified, args)
    }

    /// Call a method on a variable: var.method(args).
    pub fn method_call(&mut self, var_name: &str, method: &str, args: &[Value]) -> Value {
        let obj = self.var(var_name);
        // Dispatch to the appropriate runtime function
        let func_key = format!("__{}", method);
        if let Some(&f) = self.ctx.get_func(&func_key) {
            let mut all_args = vec![obj];
            all_args.extend_from_slice(args);
            self.ir.call(f, &all_args)
        } else {
            obj
        }
    }

    /// Log a value (dispatches based on type).
    pub fn log(&mut self, val: Value) {
        if self.ir.is_number(val) {
            if let Some(&f) = self.ctx.get_func("__print_f64") { self.ir.call_void(f, &[val]); }
        } else if self.ir.value_ir_type(val) == types::I8 {
            if let Some(&f) = self.ctx.get_func("__print_bool") { self.ir.call_void(f, &[val]); }
        } else {
            if let Some(&f) = self.ctx.get_func("__print") { self.ir.call_void(f, &[val]); }
        }
    }
}

// ─── Control Flow ─────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// If-else statement. Both branches get their own Body scope.
    pub fn if_else(
        &mut self,
        cond: Value,
        then_fn: impl FnOnce(&mut Body),
        else_fn: impl FnOnce(&mut Body),
    ) {
        let then_block = self.ir.create_block();
        let else_block = self.ir.create_block();
        let merge_block = self.ir.create_block();
        self.ir.brif(cond, then_block, else_block);

        let heap_base = self.ctx.live_heap_vars.len();
        let saved_vars = self.ctx.vars.clone();
        let saved_struct_types = self.ctx.var_struct_type.clone();

        // Then branch
        self.ir.switch_to(then_block);
        self.ir.seal(then_block);
        let mut then_returned = false;
        then_fn(self);
        then_returned = self.returned;
        self.returned = false;
        if !then_returned { self.ir.jump(merge_block); }

        self.ctx.live_heap_vars.truncate(heap_base);
        self.ctx.vars = saved_vars.clone();
        self.ctx.var_struct_type = saved_struct_types.clone();

        // Else branch
        self.ir.switch_to(else_block);
        self.ir.seal(else_block);
        let mut else_returned = false;
        else_fn(self);
        else_returned = self.returned;
        self.returned = false;
        if !else_returned { self.ir.jump(merge_block); }

        self.ctx.live_heap_vars.truncate(heap_base);
        self.ctx.vars = saved_vars;
        self.ctx.var_struct_type = saved_struct_types;

        self.ir.switch_to(merge_block);
        self.ir.seal(merge_block);
    }

    /// If without else.
    pub fn if_then(&mut self, cond: Value, then_fn: impl FnOnce(&mut Body)) {
        self.if_else(cond, then_fn, |_| {});
    }

    /// While loop. Condition and body both get closures.
    pub fn while_loop(
        &mut self,
        cond_fn: impl FnOnce(&mut Body) -> Value,
        body_fn: impl FnOnce(&mut Body),
    ) {
        let header = self.ir.create_block();
        let body_block = self.ir.create_block();
        let exit = self.ir.create_block();

        let prev_exit = self.ctx.loop_exit.replace(exit.0);
        let prev_header = self.ctx.loop_header.replace(header.0);
        let prev_heap_base = self.ctx.loop_heap_base;
        self.ctx.loop_heap_base = self.ctx.live_heap_vars.len();

        self.ir.jump(header);
        self.ir.switch_to(header);
        let cond = cond_fn(self);
        self.ir.brif(cond, body_block, exit);

        self.ir.switch_to(body_block);
        self.ir.seal(body_block);
        body_fn(self);
        if !self.returned {
            emit_loop_body_cleanup(&mut self.ir, &self.ctx);
            self.ir.jump(header);
        }
        self.returned = false;
        self.ir.seal(header);

        self.ir.switch_to(exit);
        self.ir.seal(exit);

        self.ctx.live_heap_vars.truncate(self.ctx.loop_heap_base);
        self.ctx.loop_heap_base = prev_heap_base;
        self.ctx.loop_exit = prev_exit;
        self.ctx.loop_header = prev_header;
    }

    /// For-each loop over an array.
    pub fn for_each(
        &mut self,
        binding: &str,
        arr: Value,
        body_fn: impl FnOnce(&mut Body),
    ) {
        let len = if let Some(f) = self.ctx.get_func("__array_len").copied() {
            self.ir.call(f, &[arr])
        } else {
            if self.ir.is_number(arr) { self.ir.f64_to_i64(arr) } else { arr }
        };

        let arr_slot = self.ir.alloc_var(arr);
        let len_slot = self.ir.alloc_var(len);
        let zero = self.ir.const_i64(0);
        let idx_slot = self.ir.alloc_var(zero);

        let header = self.ir.create_block();
        let body_block = self.ir.create_block();
        let exit = self.ir.create_block();

        let prev_exit = self.ctx.loop_exit.replace(exit.0);
        let prev_header = self.ctx.loop_header.replace(header.0);
        let prev_heap_base = self.ctx.loop_heap_base;
        self.ctx.loop_heap_base = self.ctx.live_heap_vars.len();

        self.ir.jump(header);
        self.ir.switch_to(header);

        let idx = self.ir.load_var(idx_slot, &RocaType::Unknown);
        let len_val = self.ir.load_var(len_slot, &RocaType::Unknown);
        let cond = self.ir.i_slt(idx, len_val);
        self.ir.brif(cond, body_block, exit);

        self.ir.switch_to(body_block);
        self.ir.seal(body_block);

        let idx_val = self.ir.load_var(idx_slot, &RocaType::Unknown);
        let cur_arr = self.ir.load_var(arr_slot, &RocaType::Unknown);
        if let Some(f) = self.ctx.get_func("__array_get_f64") {
            let elem = self.ir.call(*f, &[cur_arr, idx_val]);
            let elem_slot = self.ir.alloc_var(elem);
            self.ctx.set_var_kind(binding.to_string(), elem_slot.0, types::F64, RocaType::Number);
        }

        body_fn(self);

        if !self.returned {
            emit_loop_body_cleanup(&mut self.ir, &self.ctx);
            let cur = self.ir.load_var(idx_slot, &RocaType::Unknown);
            let one = self.ir.const_i64(1);
            let next = self.ir.iadd(cur, one);
            self.ir.store_var(idx_slot, next);
            self.ir.jump(header);
        }
        self.returned = false;
        self.ir.seal(header);

        self.ir.switch_to(exit);
        self.ir.seal(exit);

        self.ctx.live_heap_vars.truncate(self.ctx.loop_heap_base);
        self.ctx.loop_heap_base = prev_heap_base;
        self.ctx.loop_exit = prev_exit;
        self.ctx.loop_header = prev_header;
    }

    /// Break out of the current loop.
    pub fn break_loop(&mut self) {
        if let Some(exit) = self.ctx.loop_exit {
            emit_loop_body_cleanup(&mut self.ir, &self.ctx);
            self.ir.raw().ins().jump(exit, &[]);
            self.returned = true;
        }
    }

    /// Continue to next iteration of the current loop.
    pub fn continue_loop(&mut self) {
        if let Some(header) = self.ctx.loop_header {
            emit_loop_body_cleanup(&mut self.ir, &self.ctx);
            self.ir.raw().ins().jump(header, &[]);
            self.returned = true;
        }
    }
}

// ─── Returns ──────────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Return a value from the function.
    pub fn return_val(&mut self, val: Value) {
        emit_scope_cleanup(&mut self.ir, &self.ctx, None);
        if self.ctx.returns_err {
            let no_err = self.ir.const_bool(false);
            self.ir.ret_with_err(val, no_err);
        } else {
            self.ir.ret(val);
        }
        self.returned = true;
    }

    /// Return an error by name.
    pub fn return_err(&mut self, err_name: &str) {
        if self.ctx.returns_err {
            emit_scope_cleanup(&mut self.ir, &self.ctx, None);
            let default_val = default_for_ir_type(self.ir.raw(), self.ctx.return_type);
            let tag = (err_name.bytes().fold(1u8, |a, c| a.wrapping_add(c))).max(1);
            let err_tag = self.ir.raw().ins().iconst(types::I8, tag as i64);
            self.ir.ret_with_err(default_val, err_tag);
            self.returned = true;
        }
    }

    /// Destructure a call that returns (value, err): let {name, err_name} = call(...)
    pub fn let_result(&mut self, name: &str, err_name: &str, func_name: &str, args: &[Value]) -> (Value, Value) {
        if let Some(func_ref) = self.ctx.get_func(func_name).copied() {
            let results = self.ir.call_multi(func_ref, args);
            if results.len() >= 2 {
                let val = results[0];
                let err = results[1];
                let cl_type = self.ir.value_ir_type(val);
                let val_slot = self.ir.alloc_var(val);
                let kind = if self.ir.is_number(val) { RocaType::Number } else { RocaType::Unknown };
                self.ctx.set_var_kind(name.to_string(), val_slot.0, cl_type, kind);
                let err_slot = self.ir.alloc_var(err);
                self.ctx.set_var_kind(err_name.to_string(), err_slot.0, types::I8, RocaType::Bool);
                return (val, err);
            }
        }
        (self.ir.null(), self.ir.const_bool(false))
    }

    /// Check if the body has returned (useful for knowing when to emit default return).
    pub fn has_returned(&self) -> bool {
        self.returned
    }
}
