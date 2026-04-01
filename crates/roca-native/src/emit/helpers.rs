pub use roca_cranelift::emit_helpers::*;

use cranelift_codegen::ir::{types, Value, InstBuilder};
use cranelift_frontend::FunctionBuilder;
use roca_ast::Expr;
use roca_cranelift::context::EmitCtx;

/// Emit the first argument expression, or null if args is empty.
/// This wrapper calls emit_expr (in roca-native) then delegates to roca-cranelift.
pub fn first_arg_or_null(b: &mut FunctionBuilder, args: &[Expr], ctx: &mut EmitCtx) -> Value {
    let first_val = args.first().map(|a| super::expr::emit_expr(b, a, ctx));
    roca_cranelift::emit_helpers::first_arg_or_null(b, first_val)
}
