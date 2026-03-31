//! Roca AST → Cranelift IR emission.

use std::collections::HashMap;
use cranelift_codegen::ir::{self, types, AbiParam, InstBuilder, StackSlotData, StackSlotKind, Value, FuncRef};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::JITModule;
use cranelift_module::{Module, Linkage, FuncId};

use crate::ast::{self as roca, Expr, Stmt, BinOp};
use super::types::roca_to_cranelift;
use super::runtime::RuntimeFuncs;

struct VarInfo {
    slot: ir::StackSlot,
    cranelift_type: ir::Type,
}

/// Tracks compiled functions and runtime refs available during emission
pub struct CompiledFuncs {
    pub funcs: HashMap<String, FuncId>,
}

impl CompiledFuncs {
    pub fn new() -> Self {
        Self { funcs: HashMap::new() }
    }
}

/// Compile a Roca function to native code. Returns the FuncId.
pub fn compile_function(
    module: &mut JITModule,
    func: &roca::FnDef,
    _rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
) -> Result<FuncId, String> {
    let mut sig = module.make_signature();
    for param in &func.params {
        sig.params.push(AbiParam::new(roca_to_cranelift(&param.type_ref)));
    }
    let ret_type = roca_to_cranelift(&func.return_type);
    sig.returns.push(AbiParam::new(ret_type));
    if func.returns_err {
        sig.returns.push(AbiParam::new(types::I8));
    }

    let func_id = module.declare_function(&func.name, Linkage::Export, &sig)
        .map_err(|e| format!("declare: {}", e))?;
    compiled.funcs.insert(func.name.clone(), func_id);

    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    let mut bc = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut bc);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    // Import compiled functions into this function's scope
    let mut func_refs: HashMap<String, FuncRef> = HashMap::new();
    for (name, fid) in &compiled.funcs {
        if *name != func.name { // don't import self
            let fref = module.declare_func_in_func(*fid, &mut builder.func);
            func_refs.insert(name.clone(), fref);
        }
    }

    // Import runtime print
    let print_ref = module.declare_func_in_func(_rt.print, &mut builder.func);
    func_refs.insert("__print".to_string(), print_ref);

    // Store params in stack slots
    let mut vars: HashMap<String, VarInfo> = HashMap::new();
    let block_params: Vec<Value> = builder.block_params(entry).to_vec();
    for (i, p) in func.params.iter().enumerate() {
        let val = block_params[i];
        let cl_type = roca_to_cranelift(&p.type_ref);
        let slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 0));
        builder.ins().stack_store(val, slot, 0);
        vars.insert(p.name.clone(), VarInfo { slot, cranelift_type: cl_type });
    }

    let mut returned = false;
    for stmt in &func.body {
        if returned { break; }
        emit_stmt(&mut builder, stmt, &mut vars, &func_refs, func.returns_err, &mut returned);
    }

    if !returned {
        let default_val = default_value_for_type(&mut builder, &func.return_type);
        if func.returns_err {
            let no_err = builder.ins().iconst(types::I8, 0);
            builder.ins().return_(&[default_val, no_err]);
        } else {
            builder.ins().return_(&[default_val]);
        }
    }

    builder.finalize();

    module.define_function(func_id, &mut ctx)
        .map_err(|e| format!("compile error in {}: {}", func.name, e))?;
    module.clear_context(&mut ctx);
    Ok(func_id)
}

fn default_value_for_type(b: &mut FunctionBuilder, ty: &roca::TypeRef) -> Value {
    match ty {
        roca::TypeRef::Number => b.ins().f64const(0.0),
        roca::TypeRef::Bool => b.ins().iconst(types::I8, 0),
        _ => b.ins().iconst(types::I64, 0),
    }
}

fn emit_stmt(b: &mut FunctionBuilder, stmt: &Stmt, vars: &mut HashMap<String, VarInfo>, funcs: &HashMap<String, FuncRef>, returns_err: bool, returned: &mut bool) {
    match stmt {
        Stmt::Const { name, value, .. } | Stmt::Let { name, value, .. } => {
            let val = emit_expr(b, value, vars, funcs);
            let cl_type = b.func.dfg.value_type(val);
            let slot = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 0));
            b.ins().stack_store(val, slot, 0);
            vars.insert(name.clone(), VarInfo { slot, cranelift_type: cl_type });
        }
        Stmt::Return(expr) => {
            let val = emit_expr(b, expr, vars, funcs);
            if returns_err {
                let no_err = b.ins().iconst(types::I8, 0);
                b.ins().return_(&[val, no_err]);
            } else {
                b.ins().return_(&[val]);
            }
            *returned = true;
        }
        Stmt::Expr(expr) => { emit_expr(b, expr, vars, funcs); }
        Stmt::If { condition, then_body, else_body, .. } => {
            let cond = emit_expr(b, condition, vars, funcs);
            let then_block = b.create_block();
            let else_block = b.create_block();
            let merge_block = b.create_block();
            b.ins().brif(cond, then_block, &[], else_block, &[]);

            b.switch_to_block(then_block);
            b.seal_block(then_block);
            let mut then_ret = false;
            for s in then_body { if then_ret { break; } emit_stmt(b, s, vars, funcs, returns_err, &mut then_ret); }
            if !then_ret { b.ins().jump(merge_block, &[]); }

            b.switch_to_block(else_block);
            b.seal_block(else_block);
            let mut else_ret = false;
            if let Some(body) = else_body {
                for s in body { if else_ret { break; } emit_stmt(b, s, vars, funcs, returns_err, &mut else_ret); }
            }
            if !else_ret { b.ins().jump(merge_block, &[]); }

            b.switch_to_block(merge_block);
            b.seal_block(merge_block);
        }
        _ => {}
    }
}

fn emit_expr(b: &mut FunctionBuilder, expr: &Expr, vars: &HashMap<String, VarInfo>, funcs: &HashMap<String, FuncRef>) -> Value {
    match expr {
        Expr::Number(n) => b.ins().f64const(*n),
        Expr::Bool(v) => b.ins().iconst(types::I8, if *v { 1 } else { 0 }),
        Expr::String(s) => {
            let leaked = Box::into_raw(format!("{}\0", s).into_boxed_str());
            b.ins().iconst(types::I64, leaked as *const u8 as i64)
        }
        Expr::Ident(name) => {
            if let Some(var) = vars.get(name) {
                b.ins().stack_load(var.cranelift_type, var.slot, 0)
            } else {
                b.ins().iconst(types::I64, 0)
            }
        }
        Expr::BinOp { left, op, right } => {
            let l = emit_expr(b, left, vars, funcs);
            let r = emit_expr(b, right, vars, funcs);
            match op {
                BinOp::Add => b.ins().fadd(l, r),
                BinOp::Sub => b.ins().fsub(l, r),
                BinOp::Mul => b.ins().fmul(l, r),
                BinOp::Div => b.ins().fdiv(l, r),
                BinOp::Eq => { let c = b.ins().fcmp(ir::condcodes::FloatCC::Equal, l, r); b.ins().uextend(types::I64, c) }
                BinOp::Lt => { let c = b.ins().fcmp(ir::condcodes::FloatCC::LessThan, l, r); b.ins().uextend(types::I64, c) }
                BinOp::Gt => { let c = b.ins().fcmp(ir::condcodes::FloatCC::GreaterThan, l, r); b.ins().uextend(types::I64, c) }
                _ => b.ins().iconst(types::I64, 0),
            }
        }
        Expr::Call { target, args } => {
            if let Expr::Ident(name) = target.as_ref() {
                // Check for built-in log
                if name == "log" {
                    if let Some(print_ref) = funcs.get("__print") {
                        if let Some(arg) = args.first() {
                            let val = emit_expr(b, arg, vars, funcs);
                            b.ins().call(*print_ref, &[val]);
                        }
                    }
                    return b.ins().iconst(types::I8, 0);
                }
                // Call a compiled Roca function
                if let Some(func_ref) = funcs.get(name) {
                    let arg_vals: Vec<Value> = args.iter()
                        .map(|a| emit_expr(b, a, vars, funcs))
                        .collect();
                    let call = b.ins().call(*func_ref, &arg_vals);
                    let results = b.inst_results(call);
                    if !results.is_empty() {
                        return results[0];
                    }
                }
            }
            b.ins().iconst(types::I64, 0)
        }
        _ => b.ins().iconst(types::I64, 0),
    }
}
