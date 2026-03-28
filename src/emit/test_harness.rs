use crate::ast as roca;
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_codegen::Codegen;
use oxc_span::{SPAN, SourceType};

/// Emit a test file that imports from the main JS module.
/// Returns (test_js_code, test_count) or None if no tests.
pub fn emit_tests(file: &roca::SourceFile, import_path: &str) -> Option<(String, usize)> {
    let has_tests = file.items.iter().any(|item| match item {
        roca::Item::Function(f) => f.test.is_some(),
        roca::Item::Struct(s) => s.methods.iter().any(|m| m.test.is_some()),
        _ => false,
    });
    if !has_tests {
        return None;
    }

    // Collect export names for the import statement
    let mut exports = Vec::new();
    for item in &file.items {
        match item {
            roca::Item::Function(f) if f.is_pub => exports.push(f.name.clone()),
            roca::Item::Struct(s) if s.is_pub => exports.push(s.name.clone()),
            _ => {}
        }
    }
    // Also collect private names — tests need access to everything
    for item in &file.items {
        match item {
            roca::Item::Function(f) if !f.is_pub => exports.push(f.name.clone()),
            roca::Item::Struct(s) if !s.is_pub => exports.push(s.name.clone()),
            _ => {}
        }
    }

    // Build the import line as raw string (OXC import API is complex)
    let import_line = if exports.is_empty() {
        String::new()
    } else {
        format!("import {{ {} }} from \"{}\";\n", exports.join(", "), import_path)
    };

    // Build test assertions via OXC
    let allocator = Allocator::default();
    let ast = AstBuilder::new(&allocator);
    let source_text = allocator.alloc_str("");

    let mut body = ast.vec();
    let mut test_count: usize = 0;

    // let _passed = 0; let _failed = 0;
    body.push(make_let_init(&ast, "_passed", 0.0));
    body.push(make_let_init(&ast, "_failed", 0.0));

    for item in &file.items {
        match item {
            roca::Item::Function(f) => {
                if let Some(test) = &f.test {
                    test_count += emit_fn_tests(&ast, &f.name, f.returns_err, test, &mut body);
                }
            }
            roca::Item::Struct(s) => {
                for method in &s.methods {
                    if let Some(test) = &method.test {
                        test_count += emit_static_method_tests(&ast, &s.name, &method.name, method.returns_err, test, &mut body);
                    }
                }
            }
            _ => {}
        }
    }

    // console.log(_passed + " passed, " + _failed + " failed")
    body.push(ast.statement_expression(SPAN, make_summary(&ast)));

    // if (_failed > 0) process.exit(1)
    let exit_check = ast.expression_binary(
        SPAN,
        ast.expression_identifier(SPAN, "_failed"),
        BinaryOperator::GreaterThan,
        ast.expression_numeric_literal(SPAN, 0.0, None, NumberBase::Decimal),
    );
    let exit_call = make_process_exit(&ast, 1);
    body.push(ast.statement_if(SPAN, exit_check, ast.statement_expression(SPAN, exit_call), None));

    let program = ast.program(SPAN, SourceType::mjs(), source_text, ast.vec(), None, ast.vec(), body);
    let test_code = Codegen::new().build(&program).code;

    // For single-file builds (no real imports), embed the code
    // For multi-file builds, use imports
    let full = if import_path == "__embed__" {
        // Single file mode — embed the main code
        let main_js = super::emit(file).replace("export ", "");
        format!("{}\n{}", main_js, test_code)
    } else {
        format!("{}{}", import_line, test_code)
    };

    Some((full, test_count))
}

fn emit_fn_tests<'a>(
    ast: &AstBuilder<'a>,
    fn_name: &str,
    returns_err: bool,
    test: &roca::TestBlock,
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) -> usize {
    let mut count = 0;
    for (i, case) in test.cases.iter().enumerate() {
        let label = format!("{}[{}]", fn_name, i);
        match case {
            roca::TestCase::Equals { args, expected } => {
                let call = build_fn_call(ast, fn_name, args);
                let result = if returns_err { index_access(ast, call, 0) } else { call };
                let expected_expr = super::expressions::build_expr(ast, expected);
                emit_assert_eq(ast, &label, result, expected_expr, body);
                count += 1;
            }
            roca::TestCase::IsOk { args } => {
                let call = build_fn_call(ast, fn_name, args);
                let err = index_access(ast, call, 1);
                emit_assert_null(ast, &label, err, body);
                count += 1;
            }
            roca::TestCase::IsErr { args, err_name } => {
                let call = build_fn_call(ast, fn_name, args);
                let err = index_access(ast, call, 1);
                let msg = Expression::from(ast.member_expression_static(
                    SPAN, err, ast.identifier_name(SPAN, "message"), false,
                ));
                let expected = ast.str(err_name);
                emit_assert_eq(ast, &label, msg, ast.expression_string_literal(SPAN, expected, None), body);
                count += 1;
            }
            _ => {}
        }
    }
    count
}

fn emit_static_method_tests<'a>(
    ast: &AstBuilder<'a>,
    struct_name: &str,
    method_name: &str,
    returns_err: bool,
    test: &roca::TestBlock,
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) -> usize {
    let full_name = format!("{}.{}", struct_name, method_name);
    let mut count = 0;
    for (i, case) in test.cases.iter().enumerate() {
        let label = format!("{}[{}]", full_name, i);
        match case {
            roca::TestCase::Equals { args, expected } => {
                let call = build_method_call(ast, struct_name, method_name, args);
                let result = if returns_err { index_access(ast, call, 0) } else { call };
                let expected_expr = super::expressions::build_expr(ast, expected);
                emit_assert_eq(ast, &label, result, expected_expr, body);
                count += 1;
            }
            roca::TestCase::IsOk { args } => {
                let call = build_method_call(ast, struct_name, method_name, args);
                let err = index_access(ast, call, 1);
                emit_assert_null(ast, &label, err, body);
                count += 1;
            }
            roca::TestCase::IsErr { args, err_name } => {
                let call = build_method_call(ast, struct_name, method_name, args);
                let err = index_access(ast, call, 1);
                let msg = Expression::from(ast.member_expression_static(
                    SPAN, err, ast.identifier_name(SPAN, "message"), false,
                ));
                let expected = ast.str(err_name);
                emit_assert_eq(ast, &label, msg, ast.expression_string_literal(SPAN, expected, None), body);
                count += 1;
            }
            _ => {}
        }
    }
    count
}

// ─── Call builders ──────────────────────────────────────

fn build_fn_call<'a>(ast: &AstBuilder<'a>, name: &str, args: &[roca::Expr]) -> Expression<'a> {
    let mut oxc_args = ast.vec();
    for a in args {
        oxc_args.push(Argument::from(super::expressions::build_expr(ast, a)));
    }
    let n = ast.str(name);
    ast.expression_call(SPAN, ast.expression_identifier(SPAN, n), NONE, oxc_args, false)
}

fn build_method_call<'a>(ast: &AstBuilder<'a>, struct_name: &str, method_name: &str, args: &[roca::Expr]) -> Expression<'a> {
    let mut oxc_args = ast.vec();
    for a in args {
        oxc_args.push(Argument::from(super::expressions::build_expr(ast, a)));
    }
    let s = ast.str(struct_name);
    let m = ast.str(method_name);
    let callee = Expression::from(ast.member_expression_static(
        SPAN, ast.expression_identifier(SPAN, s), ast.identifier_name(SPAN, m), false,
    ));
    ast.expression_call(SPAN, callee, NONE, oxc_args, false)
}

fn index_access<'a>(ast: &AstBuilder<'a>, expr: Expression<'a>, index: u32) -> Expression<'a> {
    Expression::from(ast.member_expression_computed(
        SPAN, expr, ast.expression_numeric_literal(SPAN, index as f64, None, NumberBase::Decimal), false,
    ))
}

// ─── Assert helpers ─────────────────────────────────────

fn emit_assert_eq<'a>(
    ast: &AstBuilder<'a>,
    label: &str,
    actual: Expression<'a>,
    expected: Expression<'a>,
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) {
    let test = ast.expression_binary(SPAN, actual, BinaryOperator::StrictEquality, expected);

    let pass_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_passed")));
    let pass_inc = ast.expression_update(SPAN, UpdateOperator::Increment, false, pass_target);
    let mut then_stmts = ast.vec();
    then_stmts.push(ast.statement_expression(SPAN, pass_inc));
    let consequent = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, then_stmts)));

    let fail_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_failed")));
    let fail_inc = ast.expression_update(SPAN, UpdateOperator::Increment, false, fail_target);
    let fail_msg = ast.str(&format!("FAIL: {}", label));
    let mut fail_args = ast.vec();
    fail_args.push(Argument::from(ast.expression_string_literal(SPAN, fail_msg, None)));
    let log_call = ast.expression_call(
        SPAN,
        Expression::from(ast.member_expression_static(
            SPAN, ast.expression_identifier(SPAN, "console"), ast.identifier_name(SPAN, "log"), false,
        )),
        NONE, fail_args, false,
    );
    let mut else_stmts = ast.vec();
    else_stmts.push(ast.statement_expression(SPAN, fail_inc));
    else_stmts.push(ast.statement_expression(SPAN, log_call));
    let alternate = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, else_stmts)));

    body.push(ast.statement_if(SPAN, test, consequent, Some(alternate)));
}

fn emit_assert_null<'a>(
    ast: &AstBuilder<'a>,
    label: &str,
    actual: Expression<'a>,
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) {
    emit_assert_eq(ast, label, actual, ast.expression_null_literal(SPAN), body);
}

fn make_let_init<'a>(ast: &AstBuilder<'a>, name: &str, val: f64) -> Statement<'a> {
    let n = ast.str(name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let init = ast.expression_numeric_literal(SPAN, val, None, NumberBase::Decimal);
    let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, pattern, NONE, Some(init), false);
    let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(declarator), false);
    Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))
}

fn make_summary<'a>(ast: &AstBuilder<'a>) -> Expression<'a> {
    let msg = ast.expression_binary(
        SPAN,
        ast.expression_binary(
            SPAN,
            ast.expression_binary(
                SPAN,
                ast.expression_identifier(SPAN, "_passed"),
                BinaryOperator::Addition,
                ast.expression_string_literal(SPAN, " passed, ", None),
            ),
            BinaryOperator::Addition,
            ast.expression_identifier(SPAN, "_failed"),
        ),
        BinaryOperator::Addition,
        ast.expression_string_literal(SPAN, " failed", None),
    );
    let mut args = ast.vec();
    args.push(Argument::from(msg));
    ast.expression_call(
        SPAN,
        Expression::from(ast.member_expression_static(
            SPAN, ast.expression_identifier(SPAN, "console"), ast.identifier_name(SPAN, "log"), false,
        )),
        NONE, args, false,
    )
}

fn make_process_exit<'a>(ast: &AstBuilder<'a>, code: i32) -> Expression<'a> {
    let mut args = ast.vec();
    args.push(Argument::from(ast.expression_numeric_literal(SPAN, code as f64, None, NumberBase::Decimal)));
    ast.expression_call(
        SPAN,
        Expression::from(ast.member_expression_static(
            SPAN, ast.expression_identifier(SPAN, "process"), ast.identifier_name(SPAN, "exit"), false,
        )),
        NONE, args, false,
    )
}
