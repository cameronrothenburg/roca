pub use roca_cranelift::emit_helpers::*;

use roca_ast::Expr;
use roca_cranelift::context::EmitCtx;
use roca_cranelift::builder::{IrBuilder, Value};

/// Emit the first argument expression, or null if args is empty.
pub fn first_arg_or_null(ir: &mut IrBuilder, args: &[Expr], ctx: &mut EmitCtx) -> Value {
    let first_val = args.first().map(|a| super::expr::emit_expr(ir, a, ctx));
    roca_cranelift::emit_helpers::first_arg_or_null(ir, first_val)
}
