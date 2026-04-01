pub use roca_cranelift::emit_helpers::*;

use roca_ast::Expr;
use roca_cranelift::api::Body;
use roca_cranelift::builder::Value;

/// Emit the first argument expression, or null if args is empty.
pub fn first_arg_or_null(body: &mut Body, args: &[Expr]) -> Value {
    let first_val = args.first().map(|a| super::expr::emit_expr(body, a));
    roca_cranelift::emit_helpers::first_arg_or_null(body.ir, first_val)
}
