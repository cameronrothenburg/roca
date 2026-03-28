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

    body.push(make_let_init(&ast, "_passed", 0.0));
    body.push(make_let_init(&ast, "_failed", 0.0));

    // Emit mock objects for contracts with mock blocks
    for item in &file.items {
        if let roca::Item::Contract(c) = item {
            if let Some(mock_def) = &c.mock {
                emit_mock_object(&ast, &c.name, mock_def, &mut body);
            }
        }
    }

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

    // Auto-generate fuzz tests for pub functions with typed params (hand-rolled edge cases)
    for item in &file.items {
        if let roca::Item::Function(f) = item {
            if f.is_pub && !f.params.is_empty() {
                test_count += emit_fuzz_tests(&ast, &f.name, &f.params, f.returns_err, &mut body);
            }
        }
    }

    // Generate fast-check battle tests (appended as raw JS)
    let battle_tests = generate_battle_tests(file);

    // Summary + exit are added as raw JS AFTER battle tests
    // so battle test results are counted

    let program = ast.program(SPAN, SourceType::mjs(), source_text, ast.vec(), None, ast.vec(), body);
    let test_code = Codegen::new().build(&program).code;

    // Generate mock patching code for struct dependencies
    // Only in multi-file mode — in single-file mode, patches would override the real
    // implementations that the test block is trying to test
    let mock_patches = if import_path != "__embed__" {
        generate_mock_patches(file)
    } else {
        String::new()
    };

    // Summary + exit appended after battle tests
    let summary_js = "console.log(_passed + \" passed, \" + _failed + \" failed\");\nif (_failed > 0) process.exit(1);";

    let full = if import_path == "__embed__" {
        let main_js = super::emit(file).replace("export ", "");
        let mut parts = vec![main_js];
        if !mock_patches.is_empty() { parts.push(mock_patches); }
        parts.push(test_code);
        if !battle_tests.is_empty() { parts.push(battle_tests); }
        parts.push(summary_js.to_string());
        parts.join("\n")
    } else {
        let mut parts = vec![import_line];
        if !mock_patches.is_empty() { parts.push(mock_patches); }
        parts.push(test_code);
        if !battle_tests.is_empty() { parts.push(battle_tests); }
        parts.push(summary_js.to_string());
        parts.join("\n")
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

// ─── Fuzz testing ───────────────────────────────────────

/// Generate fuzz test cases based on parameter types.
/// For each param type, generate edge-case values and verify the function
/// doesn't throw an uncaught exception (all errors must be contracted).
fn emit_fuzz_tests<'a>(
    ast: &AstBuilder<'a>,
    fn_name: &str,
    params: &[roca::Param],
    returns_err: bool,
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) -> usize {
    // Generate edge-case inputs per type
    let fuzz_inputs = generate_fuzz_inputs(params);
    let mut count = 0;

    for (i, inputs) in fuzz_inputs.iter().enumerate() {
        let label = format!("{}[fuzz:{}]", fn_name, i);

        // Build: try { fn(args); _passed++; } catch(_e) { _failed++; console.log("FAIL: ..."); }
        let mut call_args = ast.vec();
        for input in inputs {
            call_args.push(Argument::from(build_fuzz_value(ast, input)));
        }
        let n = ast.str(fn_name);
        let call = ast.expression_call(SPAN, ast.expression_identifier(SPAN, n), NONE, call_args, false);

        // try block: call the function, increment passed
        let mut try_stmts = ast.vec();
        try_stmts.push(ast.statement_expression(SPAN, call));
        let pass_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_passed")));
        try_stmts.push(ast.statement_expression(SPAN, ast.expression_update(SPAN, UpdateOperator::Increment, false, pass_target)));
        let try_block = ast.block_statement(SPAN, try_stmts);

        // catch block: increment failed, log
        let fail_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_failed")));
        let fail_inc = ast.expression_update(SPAN, UpdateOperator::Increment, false, fail_target);
        let fail_msg = ast.str(&format!("FAIL: {} (fuzz threw uncaught)", label));
        let mut log_args = ast.vec();
        log_args.push(Argument::from(ast.expression_string_literal(SPAN, fail_msg, None)));
        let log_call = ast.expression_call(
            SPAN,
            Expression::from(ast.member_expression_static(
                SPAN, ast.expression_identifier(SPAN, "console"), ast.identifier_name(SPAN, "log"), false,
            )),
            NONE, log_args, false,
        );
        let mut catch_stmts = ast.vec();
        catch_stmts.push(ast.statement_expression(SPAN, fail_inc));
        catch_stmts.push(ast.statement_expression(SPAN, log_call));
        let catch_body = ast.block_statement(SPAN, catch_stmts);
        let err_pattern = ast.binding_pattern_binding_identifier(SPAN, "_e");
        let catch_clause = ast.catch_clause(SPAN, Some(ast.catch_parameter(SPAN, err_pattern, NONE)), catch_body);

        body.push(ast.statement_try(SPAN, ast.alloc(try_block), Some(ast.alloc(catch_clause)), NONE));
        count += 1;
    }

    count
}

#[derive(Clone)]
enum FuzzValue {
    Str(String),
    Num(f64),
    Bool(bool),
}

fn generate_fuzz_inputs(params: &[roca::Param]) -> Vec<Vec<FuzzValue>> {
    // Edge cases per type
    let string_cases = vec![
        FuzzValue::Str(String::new()),              // empty
        FuzzValue::Str(" ".to_string()),             // whitespace
        FuzzValue::Str("a".repeat(1000)),            // long
        FuzzValue::Str("<script>".to_string()),      // XSS attempt
        FuzzValue::Str("null".to_string()),          // null string
        FuzzValue::Str("\n\t\r".to_string()),        // control chars
    ];
    let number_cases = vec![
        FuzzValue::Num(0.0),
        FuzzValue::Num(-1.0),
        FuzzValue::Num(f64::MAX),
        FuzzValue::Num(f64::MIN),
        FuzzValue::Num(0.1 + 0.2),                  // float precision
    ];
    let bool_cases = vec![
        FuzzValue::Bool(true),
        FuzzValue::Bool(false),
    ];

    // For each param, pick the right edge cases
    let cases_per_param: Vec<&Vec<FuzzValue>> = params.iter().map(|p| {
        match &p.type_ref {
            roca::TypeRef::String => &string_cases,
            roca::TypeRef::Number => &number_cases,
            roca::TypeRef::Bool => &bool_cases,
            _ => &string_cases, // default to string for unknown types
        }
    }).collect();

    // Generate combinations — take up to 10 total
    let mut results = Vec::new();
    if params.len() == 1 {
        for case in cases_per_param[0] {
            results.push(vec![case.clone()]);
        }
    } else if params.len() == 2 {
        for a in cases_per_param[0] {
            for b in cases_per_param[1] {
                results.push(vec![a.clone(), b.clone()]);
                if results.len() >= 10 { break; }
            }
            if results.len() >= 10 { break; }
        }
    } else {
        // For 3+ params, just use first edge case for each
        for case in cases_per_param[0] {
            let mut combo = vec![case.clone()];
            for other in &cases_per_param[1..] {
                combo.push(other[0].clone());
            }
            results.push(combo);
            if results.len() >= 10 { break; }
        }
    }

    results
}

fn build_fuzz_value<'a>(ast: &AstBuilder<'a>, val: &FuzzValue) -> Expression<'a> {
    match val {
        FuzzValue::Str(s) => {
            let s = ast.str(s);
            ast.expression_string_literal(SPAN, s, None)
        }
        FuzzValue::Num(n) => {
            ast.expression_numeric_literal(SPAN, *n, None, NumberBase::Decimal)
        }
        FuzzValue::Bool(b) => {
            ast.expression_boolean_literal(SPAN, *b)
        }
    }
}

/// Emit a mock object for a contract with a mock block.
/// Generates: const __mock_ContractName = { method() { return mockValue; }, ... };
fn emit_mock_object<'a>(
    ast: &AstBuilder<'a>,
    contract_name: &str,
    mock_def: &roca::MockDef,
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) {
    let mut props = ast.vec();

    for entry in &mock_def.entries {
        let value = super::expressions::build_expr(ast, &entry.value);

        // Build a method that returns the mock value
        let mut stmts = ast.vec();
        stmts.push(ast.statement_return(SPAN, Some(value)));
        let fn_body = ast.function_body(SPAN, ast.vec(), stmts);
        let formal_params = ast.formal_parameters(SPAN, FormalParameterKind::FormalParameter, ast.vec(), NONE);
        let func = ast.function(
            SPAN, FunctionType::FunctionExpression, None, false, false, false,
            NONE, NONE, formal_params, NONE, Some(fn_body),
        );

        let method_name = ast.str(&entry.method);
        let key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, method_name)));
        let method = ast.object_property_kind_object_property(
            SPAN, PropertyKind::Init, key,
            Expression::FunctionExpression(ast.alloc(func)),
            false, false, false,
        );
        props.push(method);
    }

    let obj = ast.expression_object(SPAN, props);
    let var_name = format!("__mock_{}", contract_name);
    let n = ast.str(&var_name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, pattern, NONE, Some(obj), false);
    let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
    body.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(decl))));
}

/// Generate JS code that patches struct static methods with mock implementations.
/// For each struct that has validate-style methods (returns Self, err),
/// generate a mock that returns random valid instances built from primitives.
fn generate_mock_patches(file: &roca::SourceFile) -> String {
    let mut patches = Vec::new();

    // Collect all structs and their mockable methods
    let mut structs: Vec<(&str, &[roca::Field], &[roca::FnSignature])> = Vec::new();
    for item in &file.items {
        if let roca::Item::Struct(s) = item {
            if !s.signatures.is_empty() {
                structs.push((&s.name, &s.fields, &s.signatures));
            }
        }
    }

    for (name, fields, sigs) in &structs {
        for sig in *sigs {
            if sig.returns_err && !sig.errors.is_empty() {
                // This method can fail — generate a mock that returns success
                let field_mocks: Vec<String> = fields.iter().map(|f| {
                    let mock_val = mock_value_for_type(&f.type_ref);
                    format!("{}: {}", f.name, mock_val)
                }).collect();

                let constructor_args = if field_mocks.is_empty() {
                    "{}".to_string()
                } else {
                    format!("{{ {} }}", field_mocks.join(", "))
                };

                // Save original, replace with mock
                patches.push(format!(
                    "const _save_{name}_{method} = {name}.{method};\n\
                     {name}.{method} = function() {{ return [new {name}({args}), null]; }};",
                    name = name,
                    method = sig.name,
                    args = constructor_args,
                ));
            }
        }
    }

    if patches.is_empty() {
        return String::new();
    }

    format!("// Auto-generated mock patches for dependency isolation\n{}", patches.join("\n"))
}

/// Generate fast-check battle tests as raw JS.
/// Uses the bundled roca-test.js for fast-check + helpers.
fn generate_battle_tests(file: &roca::SourceFile) -> String {
    let mut tests = Vec::new();

    // Collect all functions and struct methods eligible for battle testing
    for item in &file.items {
        match item {
            roca::Item::Function(f) if f.is_pub && !f.params.is_empty() => {
                let errors = collect_error_names(&f.body);
                if let Some(test) = generate_battle_test_for_fn(&f.name, &f.params, f.returns_err, &errors) {
                    tests.push(test);
                }
            }
            roca::Item::Struct(s) => {
                for method in &s.methods {
                    if !method.params.is_empty() {
                        let sig_errors: Vec<String> = s.signatures.iter()
                            .find(|sig| sig.name == method.name)
                            .map(|sig| sig.errors.iter().map(|e| e.name.clone()).collect())
                            .unwrap_or_default();
                        let mut errors = sig_errors;
                        let body_errors = collect_error_names(&method.body);
                        for e in body_errors { if !errors.contains(&e) { errors.push(e); } }

                        let full_name = format!("{}.{}", s.name, method.name);
                        if let Some(test) = generate_battle_test_for_method(&full_name, &s.name, &method.name, &method.params, method.returns_err, &errors) {
                            tests.push(test);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if tests.is_empty() {
        return String::new();
    }

    // Import from roca-test.js
    let mut out = String::new();
    out.push_str("// Battle tests — fast-check property-based testing\n");
    out.push_str("try {\n");
    // Use __dirname to resolve relative to the test file, not CWD
    out.push_str("const __dir = typeof __dirname !== 'undefined' ? __dirname : '.';\n");
    out.push_str("const { fc, battleTest, arb } = require(__dir + '/roca-test.js');\n");

    for test in &tests {
        out.push_str(test);
        out.push('\n');
    }

    out.push_str("} catch(_btErr) {\n");
    out.push_str("  // roca-test.js not available — skip battle tests\n");
    out.push_str("}\n");

    out
}

fn generate_battle_test_for_fn(
    name: &str,
    params: &[roca::Param],
    returns_err: bool,
    errors: &[String],
) -> Option<String> {
    let arbs = params_to_arbs(params)?;
    let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
    let error_list = format!("[{}]", errors.iter().map(|e| format!("\"{}\"", e)).collect::<Vec<_>>().join(", "));

    Some(format!(
        "{{ const _bt = battleTest({name}, [{arbs}], {errors}, 100); _passed += _bt.passed; _failed += _bt.failed; }}",
        name = name,
        arbs = arbs,
        errors = error_list,
    ))
}

fn generate_battle_test_for_method(
    full_name: &str,
    struct_name: &str,
    method_name: &str,
    params: &[roca::Param],
    returns_err: bool,
    errors: &[String],
) -> Option<String> {
    let arbs = params_to_arbs(params)?;
    let error_list = format!("[{}]", errors.iter().map(|e| format!("\"{}\"", e)).collect::<Vec<_>>().join(", "));

    Some(format!(
        "{{ const _bt = battleTest({struct_name}.{method_name}.bind({struct_name}), [{arbs}], {errors}, 100); _passed += _bt.passed; _failed += _bt.failed; }}",
        struct_name = struct_name,
        method_name = method_name,
        arbs = arbs,
        errors = error_list,
    ))
}

fn params_to_arbs(params: &[roca::Param]) -> Option<String> {
    let arbs: Vec<String> = params.iter().map(|p| {
        match &p.type_ref {
            roca::TypeRef::String => "arb.String()".to_string(),
            roca::TypeRef::Number => "arb.Number()".to_string(),
            roca::TypeRef::Bool => "arb.Bool()".to_string(),
            _ => return "null".to_string(), // unknown type — skip
        }
    }).collect();

    if arbs.iter().any(|a| a == "null") {
        return None; // Can't generate arbs for non-primitive params
    }

    Some(arbs.join(", "))
}

fn collect_error_names(stmts: &[roca::Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in stmts {
        match stmt {
            roca::Stmt::ReturnErr(name) => {
                if !names.contains(name) { names.push(name.clone()); }
            }
            roca::Stmt::If { then_body, else_body, .. } => {
                names.extend(collect_error_names(then_body));
                if let Some(body) = else_body {
                    names.extend(collect_error_names(body));
                }
            }
            roca::Stmt::For { body, .. } => {
                names.extend(collect_error_names(body));
            }
            _ => {}
        }
    }
    names
}

fn mock_value_for_type(t: &roca::TypeRef) -> String {
    match t {
        roca::TypeRef::String => "\"mock_\" + Math.random().toString(36).slice(2)".to_string(),
        roca::TypeRef::Number => "Math.floor(Math.random() * 100)".to_string(),
        roca::TypeRef::Bool => "true".to_string(),
        roca::TypeRef::Named(name) => {
            // For named types, create a minimal mock object
            format!("new {}({{}})", name)
        }
        _ => "null".to_string(),
    }
}

#[cfg(test)]
mod battle_tests {
    use super::*;

    #[test]
    fn battle_test_generated_for_pub_fn() {
        let file = crate::parse::parse(r#"
            pub fn greet(name: String) -> String {
                return "Hello " + name
                test { self("cam") == "Hello cam" }
            }
        "#);
        let battle = generate_battle_tests(&file);
        assert!(!battle.is_empty(), "should generate battle test for pub fn with String param");
        assert!(battle.contains("battleTest"), "should call battleTest");
        assert!(battle.contains("arb.String()"), "should use String arbitrary");
    }

    #[test]
    fn battle_test_for_err_function() {
        let file = crate::parse::parse(r#"
            pub fn validate(s: String) -> String, err {
                if s == "" { return err.empty }
                return s
                test {
                    self("ok") == "ok"
                    self("") is err.empty
                }
            }
        "#);
        let battle = generate_battle_tests(&file);
        assert!(battle.contains("battleTest"), "should generate battle test");
        assert!(battle.contains("\"empty\""), "should include declared error names");
    }

    #[test]
    fn no_battle_test_for_private_fn() {
        let file = crate::parse::parse(r#"
            fn helper(s: String) -> String {
                return s
                test { self("a") == "a" }
            }
        "#);
        let battle = generate_battle_tests(&file);
        assert!(battle.is_empty(), "private fn should not get battle test");
    }

    #[test]
    fn no_battle_test_for_no_params() {
        let file = crate::parse::parse(r#"
            pub fn hello() -> String {
                return "hi"
                test { self() == "hi" }
            }
        "#);
        let battle = generate_battle_tests(&file);
        assert!(battle.is_empty(), "no-param fn should not get battle test");
    }

    #[test]
    fn battle_test_for_struct_method() {
        let file = crate::parse::parse(r#"
            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err missing = "required"
                }
            }{
                fn validate(raw: String) -> Email, err {
                    if raw == "" { return err.missing }
                    return Email { value: raw }
                    test {
                        self("a@b") is Ok
                        self("") is err.missing
                    }
                }
            }
        "#);
        let battle = generate_battle_tests(&file);
        assert!(battle.contains("battleTest"), "should generate for struct method");
        assert!(battle.contains("Email.validate"), "should reference Email.validate");
        assert!(battle.contains("\"missing\""), "should include error name");
    }
}
