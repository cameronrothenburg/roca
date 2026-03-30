use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

pub(crate) fn emit_fuzz_tests<'a>(
    ast: &AstBuilder<'a>,
    fn_name: &str,
    params: &[roca::Param],
    _returns_err: bool,
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) -> usize {
    // Skip fuzz for functions with non-primitive params (structs, extern contracts)
    let all_primitive = params.iter().all(|p| matches!(p.type_ref,
        roca::TypeRef::String | roca::TypeRef::Number | roca::TypeRef::Bool
    ));
    if !all_primitive { return 0; }

    let fuzz_inputs = generate_fuzz_inputs(params);
    let mut count = 0;

    for (i, inputs) in fuzz_inputs.iter().enumerate() {
        let label = format!("{}[fuzz:{}]", fn_name, i);

        let mut call_args = ast.vec();
        for input in inputs {
            call_args.push(Argument::from(build_fuzz_value(ast, input)));
        }
        let n = ast.str(fn_name);
        let call = ast.expression_call(SPAN, ast.expression_identifier(SPAN, n), NONE, call_args, false);

        let mut try_stmts = ast.vec();
        try_stmts.push(ast.statement_expression(SPAN, call));
        let pass_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_passed")));
        try_stmts.push(ast.statement_expression(SPAN, ast.expression_update(SPAN, UpdateOperator::Increment, false, pass_target)));
        let try_block = ast.block_statement(SPAN, try_stmts);

        let fail_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_failed")));
        let fail_inc = ast.expression_update(SPAN, UpdateOperator::Increment, false, fail_target);
        // Build the failed input description
        let input_desc: Vec<String> = inputs.iter().map(|v| match v {
            FuzzValue::Str(s) => format!("\"{}\"", s.chars().take(50).collect::<String>()),
            FuzzValue::Num(n) => format!("{}", n),
            FuzzValue::Bool(b) => format!("{}", b),
        }).collect();
        let fail_msg = ast.str(&format!("FAIL: {} with ({}) — missing error path. Add crash block or declare -> Type, err", label, input_desc.join(", ")));
        let mut log_args = ast.vec();
        log_args.push(Argument::from(ast.expression_string_literal(SPAN, fail_msg, None)));
        // Also log the error
        log_args.push(Argument::from(ast.expression_identifier(SPAN, "_e")));
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
    let string_cases = vec![
        FuzzValue::Str(String::new()),
        FuzzValue::Str(" ".to_string()),
        FuzzValue::Str("a".repeat(1000)),
        FuzzValue::Str("<script>".to_string()),
        FuzzValue::Str("null".to_string()),
        FuzzValue::Str("\n\t\r".to_string()),
    ];
    let number_cases = vec![
        FuzzValue::Num(0.0),
        FuzzValue::Num(-1.0),
        FuzzValue::Num(f64::MAX),
        FuzzValue::Num(f64::MIN),
        FuzzValue::Num(0.1 + 0.2),
    ];
    let bool_cases = vec![
        FuzzValue::Bool(true),
        FuzzValue::Bool(false),
    ];

    let cases_per_param: Vec<&Vec<FuzzValue>> = params.iter().map(|p| {
        match &p.type_ref {
            roca::TypeRef::String => &string_cases,
            roca::TypeRef::Number => &number_cases,
            roca::TypeRef::Bool => &bool_cases,
            _ => &string_cases,
        }
    }).collect();

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
