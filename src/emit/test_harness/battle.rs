//! Battle test generation — stress-tests functions with adversarial and edge-case inputs.
//! Verifies that functions never throw and always return well-formed results.

use crate::ast as roca;

pub(crate) fn generate_battle_tests(file: &roca::SourceFile) -> String {
    let mut tests = Vec::new();

    for item in &file.items {
        match item {
            roca::Item::Function(f) if f.is_pub && !f.params.is_empty() => {
                let errors = roca::collect_returned_error_names(&f.body);
                if let Some(test) = generate_battle_test_for_fn(&f.name, &f.params, &errors, file) {
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
                        let body_errors = roca::collect_returned_error_names(&method.body);
                        for e in body_errors { if !errors.contains(&e) { errors.push(e); } }

                        let full_name = format!("{}.{}", s.name, method.name);
                        if let Some(test) = generate_battle_test_for_method(&full_name, &s.name, &method.name, &method.params, &errors, file) {
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

    let mut out = String::new();
    out.push_str("// Battle tests — property-based testing\n");
    out.push_str("if (typeof battleTest !== 'undefined') {\n");

    for test in &tests {
        out.push_str(test);
        out.push('\n');
    }

    out.push_str("}\n");

    out
}

fn generate_battle_test_for_fn(
    name: &str,
    params: &[roca::Param],
    errors: &[String],
    file: &roca::SourceFile,
) -> Option<String> {
    let arbs = params_to_arbs(params, file)?;
    let error_list = format!("[{}]", errors.iter().map(|e| format!("\"{}\"", e)).collect::<Vec<_>>().join(", "));

    Some(format!(
        "{{ const _bt = battleTest({name}, [{arbs}], {errors}, 100); _passed += _bt.passed; _failed += _bt.failed; }}",
        name = name,
        arbs = arbs,
        errors = error_list,
    ))
}

fn generate_battle_test_for_method(
    _full_name: &str,
    struct_name: &str,
    method_name: &str,
    params: &[roca::Param],
    errors: &[String],
    file: &roca::SourceFile,
) -> Option<String> {
    let arbs = params_to_arbs(params, file)?;
    let error_list = format!("[{}]", errors.iter().map(|e| format!("\"{}\"", e)).collect::<Vec<_>>().join(", "));

    Some(format!(
        "{{ const _bt = battleTest({struct_name}.{method_name}.bind({struct_name}), [{arbs}], {errors}, 100); _passed += _bt.passed; _failed += _bt.failed; }}",
        struct_name = struct_name,
        method_name = method_name,
        arbs = arbs,
        errors = error_list,
    ))
}

fn params_to_arbs(params: &[roca::Param], file: &roca::SourceFile) -> Option<String> {
    let arbs: Vec<String> = params.iter().map(|p| {
        if p.constraints.is_empty() {
            type_to_arb(&p.type_ref, file)
        } else {
            constrained_arb(&p.type_ref, &p.constraints)
        }
    }).collect();
    if arbs.iter().any(|a| a == "null") {
        return None;
    }
    Some(arbs.join(", "))
}

/// Generate a constrained arbitrary that probes boundary values.
/// Produces values both inside and outside the valid range for fuzzing.
fn constrained_arb(ty: &roca::TypeRef, constraints: &[roca::Constraint]) -> String {
    match ty {
        roca::TypeRef::Number => {
            let mut min = f64::NEG_INFINITY;
            let mut max = f64::INFINITY;
            for c in constraints {
                match c {
                    roca::Constraint::Min(n) => min = *n,
                    roca::Constraint::Max(n) => max = *n,
                    _ => {}
                }
            }
            if min.is_finite() && max.is_finite() {
                // Generate values at boundaries: min-1, min, mid, max, max+1
                format!("fc.oneof(fc.constant({}), fc.constant({}), fc.constant({}), fc.constant({}), fc.constant({}))",
                    min - 1.0, min, (min + max) / 2.0, max, max + 1.0)
            } else if min.is_finite() {
                format!("fc.oneof(fc.constant({}), fc.constant({}), arb.Number())", min - 1.0, min)
            } else if max.is_finite() {
                format!("fc.oneof(fc.constant({}), fc.constant({}), arb.Number())", max, max + 1.0)
            } else {
                "arb.Number()".to_string()
            }
        }
        roca::TypeRef::String => {
            let mut min_len: Option<f64> = None;
            let mut max_len: Option<f64> = None;
            let mut must_contain: Option<&str> = None;
            for c in constraints {
                match c {
                    roca::Constraint::Min(n) | roca::Constraint::MinLen(n) => min_len = Some(*n),
                    roca::Constraint::Max(n) | roca::Constraint::MaxLen(n) => max_len = Some(*n),
                    roca::Constraint::Contains(s) => must_contain = Some(s),
                    _ => {}
                }
            }
            // Generate edge cases: empty, too short, valid, too long, with/without required content
            let mut cases = vec!["fc.constant(\"\")".to_string()];
            if let Some(min) = min_len {
                if min > 1.0 {
                    cases.push(format!("fc.constant(\"x\".repeat({}))", min as i64 - 1));
                }
                cases.push(format!("fc.constant(\"x\".repeat({}))", min as i64));
            }
            if let Some(max) = max_len {
                cases.push(format!("fc.constant(\"x\".repeat({}))", max as i64));
                cases.push(format!("fc.constant(\"x\".repeat({}))", max as i64 + 1));
            }
            if let Some(s) = must_contain {
                cases.push(format!("fc.constant(\"test{}test\")", s));
                cases.push("fc.constant(\"nope\")".to_string());
            }
            format!("fc.oneof({})", cases.join(", "))
        }
        _ => "null".to_string(),
    }
}

fn type_to_arb(t: &roca::TypeRef, file: &roca::SourceFile) -> String {
    match t {
        roca::TypeRef::String => "arb.String()".to_string(),
        roca::TypeRef::Number => "arb.Number()".to_string(),
        roca::TypeRef::Bool => "arb.Bool()".to_string(),
        roca::TypeRef::Named(name) => {
            // Check for extern contract with mock — use the mock object
            for item in &file.items {
                if let roca::Item::ExternContract(c) = item {
                    if c.name == *name && c.mock.is_some() {
                        return format!("fc.constant(__mock_{})", name);
                    }
                }
            }
            // Check for struct — generate from fields
            for item in &file.items {
                if let roca::Item::Struct(s) = item {
                    if s.name == *name && !s.fields.is_empty() {
                        return struct_to_arb(s, file);
                    }
                }
            }
            "null".to_string()
        }
        _ => "null".to_string(),
    }
}

fn struct_to_arb(s: &roca::StructDef, file: &roca::SourceFile) -> String {
    let field_arbs: Vec<String> = s.fields.iter().map(|f| {
        let arb = type_to_arb(&f.type_ref, file);
        format!("{}: {}", f.name, arb)
    }).collect();

    if field_arbs.iter().any(|a| a.contains("null")) {
        return "null".to_string();
    }

    format!(
        "fc.record({{ {} }}).map(_f => new {}(_f))",
        field_arbs.join(", "),
        s.name,
    )
}

#[cfg(test)]
mod tests {
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
        assert!(!battle.is_empty());
        assert!(battle.contains("battleTest"));
        assert!(battle.contains("arb.String()"));
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
        assert!(battle.contains("battleTest"));
        assert!(battle.contains("\"empty\""));
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
        assert!(battle.is_empty());
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
        assert!(battle.is_empty());
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
        assert!(battle.contains("battleTest"));
        assert!(battle.contains("Email.validate"));
        assert!(battle.contains("\"missing\""));
    }
}
