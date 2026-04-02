//! Body — the Roca scope manager.
//! Every Roca construct is a method. Zero IR concepts exposed.
//! All block management, memory, cleanup is internal.

use cranelift_codegen::ir::{self, types, InstBuilder, Value};
use roca_ast::{self as roca, BinOp};
use roca_ast::crash::CrashHandlerKind;
use roca_types::RocaType;
use crate::builder::{IrBuilder, VarSlot};
use crate::context::{EmitCtx, StructLayout};
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

    /// Default/zero value for a given type.
    pub fn default_for(&mut self, ty: &RocaType) -> Value {
        self.ir.default_for(ty)
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

    /// Bind an immutable variable with an explicit type hint.
    pub fn const_var_typed(&mut self, name: &str, val: Value, roca_type: RocaType) -> ConstRef {
        let cl_type = self.ir.value_ir_type(val);
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

    /// Bind a mutable variable with an explicit type hint.
    pub fn let_var_typed(&mut self, name: &str, val: Value, roca_type: RocaType) -> MutRef {
        let cl_type = self.ir.value_ir_type(val);
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

    /// Reassign a variable by name. Frees old heap value, stores new value.
    pub fn assign_name(&mut self, name: &str, val: Value) {
        if let Some(var) = self.ctx.get_var(name) {
            let slot = var.slot;
            let is_heap = var.is_heap;
            let cl_type = var.cranelift_type;
            let kind = var.kind.clone();
            if is_heap {
                let refs = FreeRefs::from_ctx(&self.ctx);
                emit_free_by_kind(self.ir, slot, cl_type, kind, &refs);
            }
            self.ir.raw().ins().stack_store(val, slot, 0);
        }
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

    /// Check if a function is registered.
    pub fn has_func(&self, name: &str) -> bool {
        self.ctx.get_func(name).is_some()
    }
}

// ─── Closures ────────────────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Get the function address of a pre-compiled closure by name.
    pub fn closure_ref(&mut self, name: &str) -> Value {
        if let Some(&func_ref) = self.ctx.get_func(name) {
            self.ir.func_addr(func_ref)
        } else {
            self.ir.null()
        }
    }

    /// Call a closure stored in a variable (indirect call through function pointer).
    pub fn call_closure(&mut self, var_name: &str, args: &[Value]) -> Value {
        if let Some(var) = self.ctx.get_var(var_name) {
            if var.cranelift_type == ir::types::I64 {
                let func_ptr = self.ir.raw().ins().stack_load(ir::types::I64, var.slot, 0);
                let sig_ref = self.ir.closure_signature(args.len());
                let results = self.ir.call_indirect(sig_ref, func_ptr, args);
                if !results.is_empty() { return results[0]; }
            }
        }
        self.ir.null()
    }

    /// Get the IR type of a value (for dispatch decisions in the emit layer).
    pub fn value_type(&self, val: Value) -> cranelift_codegen::ir::Type {
        self.ir.value_ir_type(val)
    }

    /// Convert a float value to i64 (for index operations).
    pub fn to_i64(&mut self, val: Value) -> Value {
        self.ir.to_i64(val)
    }

    /// Extend a bool (i8) to i64.
    pub fn extend_bool(&mut self, val: Value) -> Value {
        self.ir.extend_bool(val)
    }

    /// Get the function address for a named function (for emit layer dispatch).
    pub fn func_addr(&mut self, name: &str) -> Option<Value> {
        self.ctx.get_func(name).map(|&f| self.ir.func_addr(f))
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
    pub fn struct_lit(&mut self, _name: &str, fields: &[(&str, Value)]) -> Value {
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

    /// Struct field access by name — resolves through struct layout.
    pub fn field_access(&mut self, obj: Value, _field: &str) -> Value {
        // Fallback: generic access
        let idx = self.ir.const_i64(0);
        if let Some(f) = self.ctx.get_func("__struct_get_ptr") {
            self.ir.call(*f, &[obj, idx])
        } else { self.ir.null() }
    }

    /// Field access on a named variable — resolves struct layout + field type.
    pub fn field_access_on(&mut self, var_name: &str, obj: Value, field: &str) -> Value {
        if let Some(struct_name) = self.ctx.var_struct_type.get(var_name).cloned() {
            if let Some(layout) = self.ctx.struct_layouts.get(&struct_name) {
                if let Some(idx) = layout.field_index(field) {
                    let field_kind = layout.field_kind(field);
                    let idx_val = self.ir.const_i64(idx as i64);
                    return if field_kind == RocaType::Number {
                        if let Some(f) = self.ctx.get_func("__struct_get_f64") { self.ir.call(*f, &[obj, idx_val]) }
                        else { self.ir.const_number(0.0) }
                    } else {
                        if let Some(f) = self.ctx.get_func("__struct_get_ptr") { self.ir.call(*f, &[obj, idx_val]) }
                        else { self.ir.null() }
                    };
                }
            }
        }
        // Fallback: use var kind for proper length dispatch
        match field {
            "length" | "len" => {
                let kind = self.ctx.get_var(var_name)
                    .map(|v| v.kind.clone())
                    .unwrap_or(RocaType::Unknown);
                self.length_with_kind(obj, kind)
            }
            _ => obj,
        }
    }

    /// Set a struct field by name on a named variable.
    pub fn field_assign(&mut self, var_name: &str, field: &str, val: Value) {
        if let Some(struct_name) = self.ctx.var_struct_type.get(var_name).cloned() {
            if let Some(layout) = self.ctx.struct_layouts.get(&struct_name) {
                if let Some(idx) = layout.field_index(field) {
                    let obj = self.var(var_name);
                    let idx_val = self.ir.const_i64(idx as i64);
                    self.struct_set(obj, idx_val, val);
                }
            }
        }
    }

    /// Set a field on a struct pointer at the given index value.
    pub fn struct_set(&mut self, ptr: Value, idx: Value, val: Value) {
        crate::emit_helpers::emit_struct_set(self.ir, ptr, idx, val, &mut self.ctx);
    }

    /// Struct literal with layout registration.
    pub fn struct_lit_checked(
        &mut self,
        name: &str,
        fields: &[(&str, Value)],
        _field_defs: Option<&[roca::Field]>,
    ) -> Value {
        // Register layout if not present
        if !self.ctx.struct_layouts.contains_key(name) {
            let layout_fields: Vec<(String, RocaType)> = fields.iter()
                .map(|(n, v)| {
                    let kind = if self.ir.is_number(*v) { RocaType::Number }
                    else if self.ir.value_ir_type(*v) == types::I8 { RocaType::Bool }
                    else { RocaType::Unknown };
                    (n.to_string(), kind)
                })
                .collect();
            self.ctx.struct_layouts.insert(name.to_string(), StructLayout { fields: layout_fields });
        }

        let num_fields = self.ir.const_i64(fields.len() as i64);
        let ptr = if let Some(f) = self.ctx.get_func("__struct_alloc") {
            self.ir.call(*f, &[num_fields])
        } else { return self.ir.null(); };

        let indices: Vec<usize> = {
            let layout = self.ctx.struct_layouts.get(name).unwrap();
            fields.iter().map(|(n, _)| layout.field_index(n).unwrap_or(0)).collect()
        };

        for (i, &(_, val)) in fields.iter().enumerate() {
            let idx_val = self.ir.const_i64(indices[i] as i64);
            crate::emit_helpers::emit_struct_set(self.ir, ptr, idx_val, val, &mut self.ctx);
        }

        ptr
    }

    /// Pop last element from an array.
    pub fn array_pop(&mut self, arr: Value) -> Value {
        if let Some(&get) = self.ctx.get_func("__array_get_f64") {
            if let Some(&len_fn) = self.ctx.get_func("__array_len") {
                let len = self.ir.call(len_fn, &[arr]);
                let one = self.ir.const_i64(1);
                let last_idx = self.ir.isub(len, one);
                return self.ir.call(get, &[arr, last_idx]);
            }
        }
        self.ir.const_number(0.0)
    }

    /// Release a reference-counted value (for temp string cleanup).
    pub fn release_rc(&mut self, val: Value) {
        if let Some(&f) = self.ctx.get_func("__rc_release") {
            self.ir.call_void(f, &[val]);
        }
    }

    /// Check if a value is a number type.
    pub fn is_number(&self, val: Value) -> bool {
        self.ir.is_number(val)
    }

    /// Check if a value is a boolean type (i8).
    pub fn is_bool(&self, val: Value) -> bool {
        self.ir.value_ir_type(val) == types::I8
    }

    /// Get length of array or string.
    pub fn length(&mut self, obj: Value) -> Value {
        let kind = if self.ir.is_number(obj) { RocaType::Number } else { RocaType::Unknown };
        crate::emit_helpers::emit_length(self.ir, obj, kind, &mut self.ctx)
    }

    /// Get length with explicit kind hint.
    pub fn length_with_kind(&mut self, obj: Value, kind: RocaType) -> Value {
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
                Some(acc) => {
                    if let Some(f) = concat {
                        let new_val = self.ir.call(f, &[acc, val]);
                        // Release the old intermediate string
                        if let Some(&release) = self.ctx.get_func("__rc_release") {
                            self.ir.call_void(release, &[acc]);
                        }
                        new_val
                    } else { val }
                }
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
                let kind = if self.ir.is_number(val) {
                    RocaType::Number
                } else if let Some(k) = self.ctx.func_return_kinds.get(fn_name) {
                    k.clone()
                } else {
                    RocaType::Unknown
                };
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

    /// Get struct field definitions (for constraint validation).
    pub fn struct_defs(&self, name: &str) -> Option<Vec<roca::Field>> {
        self.ctx.struct_defs.get(name).cloned()
    }

    /// Check if a name.variant is a known enum variant.
    pub fn is_enum_variant(&self, type_name: &str, variant: &str) -> bool {
        self.ctx.enum_variants.get(type_name)
            .map_or(false, |vs| vs.contains(&variant.to_string()))
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

    /// Wait for a single async expression with type hint.
    pub fn wait_single_typed(&mut self, name: &str, val: Value, kind: RocaType) {
        let cl_type = self.ir.value_ir_type(val);
        let slot = self.ir.alloc_var(val);
        self.ctx.set_var_kind(name.to_string(), slot.0, cl_type, kind);
    }

    /// Bind a failed_name variable to false (used for wait statements).
    pub fn bind_failed(&mut self, failed_name: &str) {
        let false_val = self.ir.const_bool(false);
        let err_slot = self.ir.alloc_var(false_val);
        self.ctx.set_var_kind(failed_name.to_string(), err_slot.0, types::I8, RocaType::Bool);
    }

    /// Build a function-pointer array from pre-compiled wait function names.
    fn build_wait_fn_array(&mut self, fn_names: &[String]) -> (Value, Value) {
        let arr = if let Some(&arr_new) = self.ctx.get_func("__array_new") {
            self.ir.call(arr_new, &[])
        } else {
            self.ir.null()
        };
        for name in fn_names {
            if let Some(&func_ref) = self.ctx.get_func(name) {
                let ptr = self.ir.func_addr(func_ref);
                if let Some(&push) = self.ctx.get_func("__array_push_str") {
                    self.ir.call_void(push, &[arr, ptr]);
                }
            }
        }
        let count = self.ir.const_i64(fn_names.len() as i64);
        (arr, count)
    }

    /// Wait for all async functions, bind results to names.
    pub fn wait_all(&mut self, names: &[String], failed_name: &str, fn_names: &[String]) {
        let (arr, count) = self.build_wait_fn_array(fn_names);
        if let Some(&wait_all) = self.ctx.get_func("__wait_all") {
            let results = self.ir.call(wait_all, &[arr, count]);
            for (i, name) in names.iter().enumerate() {
                if let Some(&get) = self.ctx.get_func("__array_get_f64") {
                    let idx = self.ir.const_i64(i as i64);
                    let val = self.ir.call(get, &[results, idx]);
                    let slot = self.ir.alloc_var(val);
                    self.ctx.set_var_kind(name.clone(), slot.0, types::F64, RocaType::Number);
                }
            }
        }
        self.bind_failed(failed_name);
    }

    /// Wait for the first async function to complete.
    pub fn wait_first(&mut self, names: &[String], failed_name: &str, fn_names: &[String]) {
        let (arr, count) = self.build_wait_fn_array(fn_names);
        if let Some(&wait_first) = self.ctx.get_func("__wait_first") {
            let val = self.ir.call(wait_first, &[arr, count]);
            if !names.is_empty() {
                let slot = self.ir.alloc_var(val);
                self.ctx.set_var_kind(names[0].clone(), slot.0, types::F64, RocaType::Number);
            }
        }
        self.bind_failed(failed_name);
    }
}

// ─── Slot Primitives (internal) ─────────────────────

#[allow(dead_code)]
impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Allocate a stack slot and store a value. Returns a handle for load/store.
    pub(crate) fn alloc_slot(&mut self, val: Value) -> VarSlot {
        self.ir.alloc_var(val)
    }

    /// Load a pointer/i64 value from a slot.
    pub(crate) fn load_slot(&mut self, slot: VarSlot) -> Value {
        self.ir.load_var(slot, &RocaType::Unknown)
    }

    /// Load an f64 value from a slot.
    pub(crate) fn load_slot_f64(&mut self, slot: VarSlot) -> Value {
        self.ir.load_var(slot, &RocaType::Number)
    }

    /// Load an i8 (bool/error tag) value from a slot.
    pub(crate) fn load_slot_bool(&mut self, slot: VarSlot) -> Value {
        self.ir.load_var(slot, &RocaType::Bool)
    }

    /// Store a value to a slot.
    pub(crate) fn store_slot(&mut self, slot: VarSlot, val: Value) {
        self.ir.store_var(slot, val);
    }

    /// Load from a slot as f64 if is_number, otherwise as i64 pointer.
    pub(crate) fn load_slot_if_number(&mut self, slot: VarSlot, is_number: bool) -> Value {
        if is_number {
            self.ir.load_var(slot, &RocaType::Number)
        } else {
            self.ir.load_var(slot, &RocaType::Unknown)
        }
    }
}

// ─── Integer Operations ─────────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Integer constant (for struct field indices, loop counters, etc.).
    pub fn int(&mut self, n: i64) -> Value {
        self.ir.const_i64(n)
    }

    /// Integer addition.
    pub fn int_add(&mut self, a: Value, b: Value) -> Value {
        self.ir.iadd(a, b)
    }

    /// Integer subtraction.
    pub fn int_sub(&mut self, a: Value, b: Value) -> Value {
        self.ir.isub(a, b)
    }

    /// Signed integer less-than comparison. Returns I64 0/1.
    pub fn int_lt(&mut self, a: Value, b: Value) -> Value {
        self.ir.i_slt(a, b)
    }

    /// Signed integer greater-than comparison. Returns I64 0/1.
    pub fn int_gt(&mut self, a: Value, b: Value) -> Value {
        self.ir.i_sgt(a, b)
    }

    /// Float less-than comparison. Returns I64 0/1.
    pub fn float_lt(&mut self, a: Value, b: Value) -> Value {
        self.ir.f_lt(a, b)
    }

    /// Float greater-than comparison. Returns I64 0/1.
    pub fn float_gt(&mut self, a: Value, b: Value) -> Value {
        self.ir.f_gt(a, b)
    }

    /// Non-RC C string pointer (for constraint/error messages).
    pub fn cstr(&mut self, s: &str) -> Value {
        self.ir.leak_cstr(s)
    }
}

// ─── Crash / Error Context ──────────────────────────

impl<'a, 'b: 'a, 'c> Body<'a, 'b, 'c> {
    /// Get crash handler for a function name (if registered).
    pub fn get_crash_handler(&self, name: &str) -> Option<CrashHandlerKind> {
        self.ctx.crash_handlers.get(name).cloned()
    }

    /// Whether the current function returns an error tuple.
    pub fn returns_err(&self) -> bool {
        self.ctx.returns_err
    }

    /// Get the default value for the current function's return type.
    pub fn return_default(&mut self) -> Value {
        default_for_ir_type(self.ir.raw(), self.ctx.return_type)
    }

    /// Emit scope cleanup (free heap vars) before an early return.
    pub fn scope_cleanup(&mut self) {
        emit_scope_cleanup(self.ir, &self.ctx, None);
    }

    /// Return a default value with an error tag (for constraint/crash errors).
    pub fn return_default_err(&mut self) {
        emit_scope_cleanup(self.ir, &self.ctx, None);
        let default = default_for_ir_type(self.ir.raw(), self.ctx.return_type);
        if self.ctx.returns_err {
            let err_tag = self.ir.raw().ins().iconst(types::I8, 1);
            self.ir.ret_with_err(default, err_tag);
        } else {
            self.ir.ret(default);
        }
        self.returned = true;
    }

    /// Panic — abort the process. Used by crash Panic step.
    pub fn panic(&mut self) {
        self.ir.trap(1);
        self.returned = true;
    }

    /// Emit a trap (process abort). Internal — use `panic()` instead.
    #[allow(dead_code)]
    pub(crate) fn trap(&mut self, code: u8) {
        self.ir.trap(code);
        self.returned = true;
    }

    /// Bind a named variable to a value with a given type in the current scope.
    pub fn bind_var(&mut self, name: &str, val: Value, kind: RocaType) {
        let cl_type = kind.to_cranelift();
        let slot = self.ir.alloc_var(val);
        self.ctx.set_var_kind(name.to_string(), slot.0, cl_type, kind);
    }

    /// Get the struct field index for a named field.
    pub fn struct_field_index(&self, struct_name: &str, field_name: &str) -> Option<usize> {
        self.ctx.struct_layouts.get(struct_name)
            .and_then(|l| l.field_index(field_name))
    }
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

    /// Match expression with lazy result evaluation.
    /// Arms provide pattern data but result expressions are evaluated inside branches
    /// via the emit_fn callback, allowing bindings to be available.
    pub fn match_lazy<Arm>(
        &mut self,
        scrutinee: Value,
        arms: &[Arm],
        default_expr: &Option<roca::Expr>,
        emit_fn: fn(&mut Body, &roca::Expr) -> Value,
    ) -> Value
    where
        Arm: MatchArmLazy,
    {
        let is_float = self.ir.is_number(scrutinee);

        // Infer result type: check default arm first (always concrete), then first arm
        let result_type = if let Some(def) = default_expr {
            let kind = self.infer_type(def);
            if kind == RocaType::Number { RocaType::Number } else { RocaType::Unknown }
        } else if let Some(first) = arms.first() {
            let kind = self.infer_type(first.kind().value_expr());
            if kind == RocaType::Number { RocaType::Number } else { RocaType::Unknown }
        } else if is_float { RocaType::Number }
        else { RocaType::Unknown };

        let merge = self.ir.create_block();
        self.ir.append_block_param(merge, &result_type);
        let scrutinee_slot = self.ir.alloc_var(scrutinee);
        let scr_type = if is_float { RocaType::Number } else { RocaType::Unknown };

        for arm in arms {
            match arm.kind() {
                LazyArmKind::Value { pattern, value_expr } => {
                    let scr = self.ir.load_var(scrutinee_slot, &scr_type);
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
                    let result = emit_fn(self, value_expr);
                    self.ir.jump_with(merge, result);

                    self.ir.switch_to(next_block);
                    self.ir.seal(next_block);
                }
                LazyArmKind::Variant { variant, bindings, value_expr } => {
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

                    // Bind variant data fields
                    let scr2 = self.ir.load_var(scrutinee_slot, &RocaType::Unknown);
                    for (i, binding) in bindings.iter().enumerate() {
                        let field_idx = self.ir.const_i64((i + 1) as i64);
                        let val = if let Some(&f) = self.ctx.get_func("__struct_get_f64") {
                            self.ir.call(f, &[scr2, field_idx])
                        } else { self.ir.const_number(0.0) };
                        let slot = self.ir.alloc_var(val);
                        self.ctx.set_var_kind(binding.clone(), slot.0, types::F64, RocaType::Number);
                    }

                    let result = emit_fn(self, value_expr);
                    self.ir.jump_with(merge, result);

                    self.ir.switch_to(next_block);
                    self.ir.seal(next_block);
                }
            }
        }

        let default_val = if let Some(expr) = default_expr {
            emit_fn(self, expr)
        } else {
            self.ir.default_for(&result_type)
        };
        self.ir.jump_with(merge, default_val);

        self.ir.switch_to(merge);
        self.ir.seal(merge);
        self.ir.block_param(merge, 0)
    }
}

/// Kind of a lazy match arm.
pub enum LazyArmKind<'a> {
    Value { pattern: &'a Value, value_expr: &'a roca::Expr },
    Variant { variant: &'a str, bindings: &'a [String], value_expr: &'a roca::Expr },
}

impl<'a> LazyArmKind<'a> {
    /// Get the value expression for type inference.
    pub fn value_expr(&self) -> &'a roca::Expr {
        match self {
            LazyArmKind::Value { value_expr, .. } => value_expr,
            LazyArmKind::Variant { value_expr, .. } => value_expr,
        }
    }
}

/// Trait for accessing match arm data lazily.
pub trait MatchArmLazy {
    fn kind(&self) -> LazyArmKind<'_>;
}
