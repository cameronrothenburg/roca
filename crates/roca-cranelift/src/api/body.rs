//! Body — the Roca scope manager.
//! Every Roca construct is a method. Zero IR concepts exposed.
//! All block management, memory, cleanup is internal.

use cranelift_codegen::ir::{self, types, InstBuilder, Value};
use roca_ast::{self as roca, BinOp};
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

/// Roca scope — every Roca construct is a method.
/// Fields are `pub(crate)` — only roca-cranelift internals can access them.
pub struct Body<'a, 'b: 'a, 'c> {
    pub(crate) ir: &'c mut IrBuilder<'a, 'b>,
    pub(crate) ctx: EmitCtx,
    pub(crate) returned: bool,
}

// ─── Literals ─────────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    pub fn number(&mut self, n: f64) -> Value {
        self.ir.const_number(n)
    }

    pub fn bool_val(&mut self, v: bool) -> Value {
        self.ir.const_bool(v)
    }

    pub fn string(&mut self, s: &str) -> Value {
        let static_ptr = self.ir.leak_cstr(s);
        if let Some(&f) = self.ctx.get_func("__string_new") {
            self.ir.call(f, &[static_ptr])
        } else {
            static_ptr
        }
    }

    pub fn null(&mut self) -> Value {
        self.ir.null()
    }

    pub fn self_ref(&mut self) -> Value {
        if let Some(var) = self.ctx.get_var("self") {
            self.ir.raw().ins().stack_load(var.cranelift_type, var.slot, 0)
        } else {
            self.ir.null()
        }
    }
}

// ─── Variables ────────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Load a variable by name.
    pub fn var(&mut self, name: &str) -> Value {
        if let Some(var) = self.ctx.get_var(name) {
            self.ir.raw().ins().stack_load(var.cranelift_type, var.slot, 0)
        } else {
            self.ir.null()
        }
    }

    /// Bind an immutable variable.
    pub fn const_var(&mut self, name: &str, val: Value) -> ConstRef {
        let roca_type = self.infer_value_type(val);
        let cl_type = roca_type.to_cranelift();
        let slot = self.ir.alloc_var(val);
        self.ctx.set_var_kind(name.to_string(), slot.0, cl_type, roca_type.clone());
        ConstRef { name: name.to_string(), slot, roca_type }
    }

    /// Bind a mutable variable.
    pub fn let_var(&mut self, name: &str, val: Value) -> MutRef {
        let roca_type = self.infer_value_type(val);
        let cl_type = roca_type.to_cranelift();
        let slot = self.ir.alloc_var(val);
        self.ctx.set_var_kind(name.to_string(), slot.0, cl_type, roca_type.clone());
        MutRef { name: name.to_string(), slot, roca_type }
    }

    /// Reassign a mutable variable. Frees the old heap value automatically.
    pub fn assign(&mut self, var: &MutRef, val: Value) {
        if let Some(old) = self.ctx.get_var(&var.name) {
            if old.is_heap {
                let slot = old.slot;
                let cl_type = old.cranelift_type;
                let kind = old.kind.clone();
                let refs = FreeRefs::from_ctx(&self.ctx);
                emit_free_by_kind(self.ir, slot, cl_type, kind, &refs);
            }
        }
        self.ir.store_var(var.slot, val);
    }

    /// Read a typed variable reference.
    pub fn read(&mut self, var: &impl VarRef) -> Value {
        self.ir.load_var(var.slot(), var.roca_type())
    }

    /// Register a variable as having a specific struct type (for field access).
    pub fn set_struct_type(&mut self, var_name: &str, struct_name: &str) {
        self.ctx.var_struct_type.insert(var_name.to_string(), struct_name.to_string());
    }

    /// Infer RocaType from an IR value's type.
    fn infer_value_type(&self, val: Value) -> RocaType {
        if self.ir.is_number(val) { RocaType::Number }
        else if self.ir.value_ir_type(val) == types::I8 { RocaType::Bool }
        else { RocaType::Unknown }
    }
}

// ─── Operators ────────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Binary operation — dispatches based on operator and value types.
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
                } else { self.ir.i_eq(l, r) }
            }
            BinOp::Neq if is_float => self.ir.f_ne(l, r),
            BinOp::Neq => {
                if let Some(f) = self.ctx.get_func("__string_eq") {
                    let eq = self.ir.call(*f, &[l, r]);
                    let ext = self.ir.extend_bool(eq);
                    let one = self.ir.const_i64(1);
                    self.ir.isub(one, ext)
                } else { self.ir.i_ne(l, r) }
            }
            BinOp::Lt => self.ir.f_lt(l, r),
            BinOp::Gt => self.ir.f_gt(l, r),
            BinOp::Lte => self.ir.f_le(l, r),
            BinOp::Gte => self.ir.f_ge(l, r),
            BinOp::And => self.ir.bool_and(l, r),
            BinOp::Or => self.ir.bool_or(l, r),
        }
    }

    /// Boolean NOT.
    pub fn not(&mut self, val: Value) -> Value {
        let zero = self.ir.null();
        self.ir.i_eq(val, zero)
    }
}

// ─── Calls ────────────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Call a function by name, return first result.
    pub fn call(&mut self, name: &str, args: &[Value]) -> Value {
        // TODO: crash handler dispatch will be absorbed in Task 3
        // For now, skip crash handling and call directly

        if let Some(&func_ref) = self.ctx.get_func(name) {
            let results = self.ir.call_multi(func_ref, args);
            if !results.is_empty() { results[0] } else { self.ir.null() }
        } else {
            self.ir.null()
        }
    }

    /// Call a function by name, discard result.
    pub fn call_void(&mut self, name: &str, args: &[Value]) {
        if let Some(&func_ref) = self.ctx.get_func(name) {
            self.ir.call_void(func_ref, args);
        }
    }

    /// Call a function by name, return all results.
    pub fn call_multi(&mut self, name: &str, args: &[Value]) -> Vec<Value> {
        if let Some(&func_ref) = self.ctx.get_func(name) {
            self.ir.call_multi(func_ref, args)
        } else {
            vec![]
        }
    }

    /// Call a method on a value: obj.method(args).
    /// Handles all stdlib method dispatch internally.
    pub fn method_call(&mut self, obj: Value, method: &str, args: &[Value]) -> Value {
        // Try qualified name first (Type.method)
        // String methods
        match method {
            "trim" => return self.call_runtime_1("__string_trim", obj),
            "toUpperCase" => return self.call_runtime_1("__string_to_upper", obj),
            "toLowerCase" => return self.call_runtime_1("__string_to_lower", obj),
            "includes" | "contains" => {
                let needle = args.first().copied().unwrap_or_else(|| self.ir.null());
                let result = self.call_runtime_2("__string_includes", obj, needle);
                return self.ir.extend_bool(result);
            }
            "startsWith" => {
                let prefix = args.first().copied().unwrap_or_else(|| self.ir.null());
                let result = self.call_runtime_2("__string_starts_with", obj, prefix);
                return self.ir.extend_bool(result);
            }
            "endsWith" => {
                let suffix = args.first().copied().unwrap_or_else(|| self.ir.null());
                let result = self.call_runtime_2("__string_ends_with", obj, suffix);
                return self.ir.extend_bool(result);
            }
            "indexOf" => {
                let needle = args.first().copied().unwrap_or_else(|| self.ir.null());
                return self.call_runtime_2("__string_index_of", obj, needle);
            }
            "charCodeAt" => {
                let idx = args.first().copied().unwrap_or_else(|| self.ir.null());
                return self.call_runtime_2("__string_char_code_at", obj, idx);
            }
            "charAt" => {
                let idx = args.first().copied().unwrap_or_else(|| self.ir.null());
                return self.call_runtime_2("__string_char_at", obj, idx);
            }
            "slice" => {
                let start = args.first().copied().unwrap_or_else(|| self.ir.null());
                let end = args.get(1).copied().unwrap_or_else(|| self.ir.null());
                if let Some(&f) = self.ctx.get_func("__string_slice") {
                    return self.ir.call(f, &[obj, start, end]);
                }
                return obj;
            }
            "split" => {
                let delim = args.first().copied().unwrap_or_else(|| self.ir.null());
                return self.call_runtime_2("__string_split", obj, delim);
            }
            "join" => {
                let sep = args.first().copied().unwrap_or_else(|| self.ir.null());
                return self.call_runtime_2("__array_join", obj, sep);
            }
            "toString" => {
                if self.ir.is_number(obj) {
                    return self.call_runtime_1("__string_from_f64", obj);
                }
                return obj;
            }
            "push" => {
                let val = if let Some(&v) = args.first() { v } else { self.ir.null() };
                crate::emit_helpers::emit_array_push(self.ir, obj, val, &mut self.ctx);
                return obj;
            }
            "length" | "len" => {
                let kind = if self.ir.is_number(obj) { RocaType::Number } else { RocaType::Unknown };
                return crate::emit_helpers::emit_length(self.ir, obj, kind, &mut self.ctx);
            }
            _ => {}
        }

        // Fallback: try as qualified function call
        obj
    }

    /// Call Type.method(args) — static method dispatch.
    pub fn static_call(&mut self, type_name: &str, method: &str, args: &[Value]) -> Value {
        let qualified = format!("{}.{}", type_name, method);
        self.call(&qualified, args)
    }

    /// Log a value (dispatches by type).
    pub fn log(&mut self, val: Value) {
        if self.ir.is_number(val) {
            if let Some(&f) = self.ctx.get_func("__print_f64") { self.ir.call_void(f, &[val]); }
        } else if self.ir.value_ir_type(val) == types::I8 {
            if let Some(&f) = self.ctx.get_func("__print_bool") { self.ir.call_void(f, &[val]); }
        } else {
            if let Some(&f) = self.ctx.get_func("__print") { self.ir.call_void(f, &[val]); }
        }
    }

    /// Internal: call a 1-arg runtime function.
    fn call_runtime_1(&mut self, name: &str, arg: Value) -> Value {
        if let Some(&f) = self.ctx.get_func(name) { self.ir.call(f, &[arg]) }
        else { arg }
    }

    /// Internal: call a 2-arg runtime function.
    fn call_runtime_2(&mut self, name: &str, a: Value, b: Value) -> Value {
        if let Some(&f) = self.ctx.get_func(name) { self.ir.call(f, &[a, b]) }
        else { a }
    }
}

// ─── Collections ──────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Create an array from elements.
    pub fn array(&mut self, elements: &[Value]) -> Value {
        let arr = if let Some(f) = self.ctx.get_func("__array_new") {
            self.ir.call(*f, &[])
        } else { return self.ir.null(); };
        for &elem in elements {
            crate::emit_helpers::emit_array_push(self.ir, arr, elem, &mut self.ctx);
        }
        arr
    }

    /// Array index access.
    pub fn index(&mut self, arr: Value, idx: Value) -> Value {
        let idx_i64 = self.ir.to_i64(idx);
        if let Some(f) = self.ctx.get_func("__array_get_f64") {
            self.ir.call(*f, &[arr, idx_i64])
        } else { self.ir.const_number(0.0) }
    }

    /// Push a value onto an array.
    pub fn array_push(&mut self, arr: Value, val: Value) {
        crate::emit_helpers::emit_array_push(self.ir, arr, val, &mut self.ctx);
    }

    /// Struct literal construction.
    pub fn struct_lit(&mut self, name: &str, fields: &[(&str, Value)]) -> Value {
        let num_fields = self.ir.const_i64(fields.len() as i64);
        let ptr = if let Some(f) = self.ctx.get_func("__struct_alloc") {
            self.ir.call(*f, &[num_fields])
        } else { return self.ir.null(); };
        for (i, &(_, val)) in fields.iter().enumerate() {
            let idx = self.ir.const_i64(i as i64);
            crate::emit_helpers::emit_struct_set(self.ir, ptr, idx, val, &mut self.ctx);
        }
        // TODO: validate constraints on fields
        ptr
    }

    /// Enum variant construction.
    pub fn enum_variant(&mut self, _enum_name: &str, variant: &str, args: &[Value]) -> Value {
        let num_fields = self.ir.const_i64((1 + args.len()) as i64);
        let ptr = if let Some(f) = self.ctx.get_func("__struct_alloc") {
            self.ir.call(*f, &[num_fields])
        } else { return self.ir.null(); };
        let tag = self.string(variant);
        let zero = self.ir.const_i64(0);
        if let Some(&f) = self.ctx.get_func("__struct_set_ptr") {
            self.ir.call_void(f, &[ptr, zero, tag]);
        }
        for (i, &arg) in args.iter().enumerate() {
            let idx = self.ir.const_i64((i + 1) as i64);
            crate::emit_helpers::emit_struct_set(self.ir, ptr, idx, arg, &mut self.ctx);
        }
        ptr
    }

    /// Struct field access.
    pub fn field_access(&mut self, obj: Value, field: &str) -> Value {
        // Check struct layout for typed field access
        // (simplified — full implementation needs var_struct_type resolution)
        let idx = self.ir.const_i64(0);
        if let Some(f) = self.ctx.get_func("__struct_get_ptr") {
            self.ir.call(*f, &[obj, idx])
        } else { self.ir.null() }
    }

    /// Get length of array or string.
    pub fn length(&mut self, obj: Value) -> Value {
        let kind = if self.ir.is_number(obj) { RocaType::Number } else { RocaType::Unknown };
        crate::emit_helpers::emit_length(self.ir, obj, kind, &mut self.ctx)
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
                    } else { *v }
                }
            };
            result = Some(match result {
                None => val,
                Some(acc) => if let Some(f) = concat { self.ir.call(f, &[acc, val]) } else { val },
            });
        }
        result.unwrap_or_else(|| self.ir.null())
    }
}

// ─── Control Flow ─────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// If-else. Both branches get the same Body (state saved/restored).
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

        self.ir.switch_to(then_block);
        self.ir.seal(then_block);
        then_fn(self);
        let then_returned = self.returned;
        self.returned = false;
        if !then_returned { self.ir.jump(merge_block); }

        self.ctx.live_heap_vars.truncate(heap_base);
        self.ctx.vars = saved_vars.clone();
        self.ctx.var_struct_type = saved_struct_types.clone();

        self.ir.switch_to(else_block);
        self.ir.seal(else_block);
        else_fn(self);
        let else_returned = self.returned;
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

    /// While loop.
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
            emit_loop_body_cleanup(self.ir, &self.ctx);
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
            emit_loop_body_cleanup(self.ir, &self.ctx);
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

    /// Break out of current loop.
    pub fn break_loop(&mut self) {
        if let Some(exit) = self.ctx.loop_exit {
            emit_loop_body_cleanup(self.ir, &self.ctx);
            self.ir.raw().ins().jump(exit, &[]);
            self.returned = true;
        }
    }

    /// Continue to next iteration.
    pub fn continue_loop(&mut self) {
        if let Some(header) = self.ctx.loop_header {
            emit_loop_body_cleanup(self.ir, &self.ctx);
            self.ir.raw().ins().jump(header, &[]);
            self.returned = true;
        }
    }
}

// ─── Returns ──────────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Return a value.
    pub fn return_val(&mut self, val: Value) {
        emit_scope_cleanup(self.ir, &self.ctx, None);
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
            emit_scope_cleanup(self.ir, &self.ctx, None);
            let default_val = default_for_ir_type(self.ir.raw(), self.ctx.return_type);
            let tag = (err_name.bytes().fold(1u8, |a, c| a.wrapping_add(c))).max(1);
            let err_tag = self.ir.raw().ins().iconst(types::I8, tag as i64);
            self.ir.ret_with_err(default_val, err_tag);
            self.returned = true;
        }
    }

    /// Destructure: let {name, err_name} = call(fn_name, args).
    pub fn let_result(&mut self, name: &str, err_name: &str, fn_name: &str, args: &[Value]) -> (Value, Value) {
        if let Some(func_ref) = self.ctx.get_func(fn_name).copied() {
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

    pub fn has_returned(&self) -> bool { self.returned }
}

// ─── Type Inference ───────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Infer the RocaType of an AST expression.
    pub fn infer_type(&self, expr: &roca::Expr) -> RocaType {
        ast_infer_kind(expr, &self.ctx)
    }
}

// ─── Async ────────────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Wait for a single async expression.
    pub fn wait_single(&mut self, name: &str, val: Value) {
        let cl_type = self.ir.value_ir_type(val);
        let slot = self.ir.alloc_var(val);
        self.ctx.set_var_kind(name.to_string(), slot.0, cl_type, RocaType::Unknown);
    }

    // TODO: wait_all, wait_first — need function pointer array building
}

// ─── Match ────────────────────────────────────────────

/// Match arm for body.match_val().
pub enum MatchArm {
    /// Match against a constant value.
    Value { pattern: Value, result: Value },
    /// Match enum variant with bindings.
    Variant { variant: String, bindings: Vec<String>, result: Value },
}

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Match expression with arms and a default.
    /// All block management is internal.
    pub fn match_val(&mut self, scrutinee: Value, arms: &[MatchArm], default: Value) -> Value {
        let is_float = self.ir.is_number(scrutinee);
        let result_type = if is_float { RocaType::Number } else { RocaType::Unknown };

        let merge = self.ir.create_block();
        self.ir.append_block_param(merge, &result_type);
        let scrutinee_slot = self.ir.alloc_var(scrutinee);

        for arm in arms {
            match arm {
                MatchArm::Value { pattern, result } => {
                    let scr = self.ir.load_var(scrutinee_slot, &result_type);
                    let cond = if is_float {
                        self.ir.f_eq(scr, *pattern)
                    } else if let Some(f) = self.ctx.get_func("__string_eq") {
                        let eq = self.ir.call(*f, &[scr, *pattern]);
                        self.ir.extend_bool(eq)
                    } else {
                        self.ir.i_eq(scr, *pattern)
                    };
                    let then_block = self.ir.create_block();
                    let next_block = self.ir.create_block();
                    self.ir.brif(cond, then_block, next_block);
                    self.ir.switch_to(then_block);
                    self.ir.seal(then_block);
                    self.ir.jump_with(merge, *result);
                    self.ir.switch_to(next_block);
                    self.ir.seal(next_block);
                }
                MatchArm::Variant { variant, bindings, result } => {
                    let scr = self.ir.load_var(scrutinee_slot, &RocaType::Unknown);
                    let zero_idx = self.ir.const_i64(0);
                    let tag_ptr = if let Some(&f) = self.ctx.get_func("__struct_get_ptr") {
                        self.ir.call(f, &[scr, zero_idx])
                    } else { self.ir.null() };
                    let variant_cstr = self.ir.leak_cstr(variant);
                    let cond = if let Some(&f) = self.ctx.get_func("__string_eq") {
                        let eq = self.ir.call(f, &[tag_ptr, variant_cstr]);
                        self.ir.extend_bool(eq)
                    } else { self.ir.null() };

                    let then_block = self.ir.create_block();
                    let next_block = self.ir.create_block();
                    self.ir.brif(cond, then_block, next_block);
                    self.ir.switch_to(then_block);
                    self.ir.seal(then_block);

                    let scr2 = self.ir.load_var(scrutinee_slot, &RocaType::Unknown);
                    for (i, binding) in bindings.iter().enumerate() {
                        let field_idx = self.ir.const_i64((i + 1) as i64);
                        let val = if let Some(&f) = self.ctx.get_func("__struct_get_f64") {
                            self.ir.call(f, &[scr2, field_idx])
                        } else { self.ir.const_number(0.0) };
                        let slot = self.ir.alloc_var(val);
                        self.ctx.set_var_kind(binding.clone(), slot.0, types::F64, RocaType::Number);
                    }

                    self.ir.jump_with(merge, *result);
                    self.ir.switch_to(next_block);
                    self.ir.seal(next_block);
                }
            }
        }

        self.ir.jump_with(merge, default);
        self.ir.switch_to(merge);
        self.ir.seal(merge);
        self.ir.block_param(merge, 0)
    }
}
