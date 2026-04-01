//! Rule: nullable-type, nullable-return, return-type-mismatch, return-null,
//! return-err-not-declared, type-annotation-mismatch, field-type-mismatch,
//! unknown-field, arg-type-mismatch
//! Validates type annotations, return types, and struct literal fields.

use crate::ast::*;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::{FnCheckContext, ItemContext, StmtContext, ExprContext};
use crate::check::walker::{resolve_type, infer_type_with_registry, type_ref_to_name};

pub struct TypeCheckRule;

impl Rule for TypeCheckRule {
    fn name(&self) -> &'static str { "types" }

    fn check_item(&self, ctx: &ItemContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        if let Item::Struct(s) = ctx.item {
            for field in &s.fields {
                if matches!(field.type_ref, TypeRef::Nullable(_)) {
                    errors.push(RuleError::new(errors::NULLABLE_TYPE, format!("field '{}.{}' is nullable — use Optional<Type> for optional fields, or a default value", s.name, field.name), None));
                }
            }
        }
        errors
    }

    fn check_function(&self, ctx: &FnCheckContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        if let TypeRef::Nullable(inner) = &ctx.func.def.return_type {
            let type_name = type_ref_to_name(inner);
            errors.push(RuleError::new(errors::NULLABLE_RETURN, format!("return {} | null is not happy path — use -> {}, err with an error for the not-found case", type_name, type_name), Some(ctx.func.qualified_name.clone())));
        }
        for param in &ctx.func.def.params {
            if matches!(param.type_ref, TypeRef::Nullable(_)) {
                errors.push(RuleError::new(errors::NULLABLE_TYPE, format!("parameter '{}' is nullable — require the value or use a default", param.name), Some(ctx.func.qualified_name.clone())));
            }
        }
        errors
    }

    fn check_stmt(&self, ctx: &StmtContext) -> Vec<RuleError> {
        let mut errors = Vec::new();

        match ctx.stmt {
            Stmt::Return(expr) => {
                let declared = &ctx.func.def.return_type;

                // return value from -> Ok function (Ok means void)
                if *declared == TypeRef::Ok {
                    if !matches!(expr, Expr::Ident(n) if n == "Ok") {
                        errors.push(RuleError::new(errors::RETURN_TYPE_MISMATCH, "function returns Ok (void) — cannot return a value", Some(ctx.func.qualified_name.clone())));
                    }
                    return errors;
                }

                // return null from non-nullable type
                if matches!(expr, Expr::Null) && !matches!(declared, TypeRef::Nullable(_)) {
                    errors.push(RuleError::new(errors::RETURN_NULL, format!("cannot return null from function returning {} — use {} | null if nullable", type_ref_to_name(declared), type_ref_to_name(declared)), Some(ctx.func.qualified_name.clone())));
                    return errors;
                }

                // return type mismatch
                if let Some(actual) = infer_type_with_registry(expr, ctx.scope, Some(ctx.check.registry)) {
                    let expected = type_ref_to_name(declared);
                    if !types_compatible(&expected, &actual) {
                        errors.push(RuleError::new(errors::RETURN_TYPE_MISMATCH, format!("function returns {} but got {}", expected, actual), Some(ctx.func.qualified_name.clone())));
                    }
                }
            }
            // return err.x from function not declared as , err
            Stmt::ReturnErr { name: err_name, .. } => {
                if !ctx.func.def.returns_err {
                    errors.push(RuleError::new(errors::RETURN_ERR_NOT_DECLARED, format!("cannot return err.{} — function is not declared with ', err'", err_name), Some(ctx.func.qualified_name.clone())));
                }
            }
            // Let/Const with type annotation: check value type matches annotation
            Stmt::Const { value, type_ann: Some(ann), name, .. }
            | Stmt::Let { value, type_ann: Some(ann), name, .. } => {
                let expected = type_ref_to_name(ann);
                if let Some(actual) = infer_type_with_registry(value, ctx.scope, Some(ctx.check.registry)) {
                    if !types_compatible(&expected, &actual) {
                        errors.push(RuleError::new(errors::TYPE_ANNOTATION_MISMATCH, format!("'{}' declared as {} but assigned {}", name, expected, actual), Some(ctx.func.qualified_name.clone())));
                    }
                }
            }
            _ => {}
        }

        errors
    }

    fn check_expr(&self, ctx: &ExprContext) -> Vec<RuleError> {
        let mut errors = Vec::new();

        match ctx.expr {
            // Struct literal: check field types match struct declaration
            Expr::StructLit { name, fields } => {
                if let Some(contract) = ctx.check.registry.get(name) {
                    for (field_name, value) in fields {
                        if let Some(decl_field) = contract.fields.iter().find(|f| f.name == *field_name) {
                            let expected = type_ref_to_name(&decl_field.type_ref);
                            if let Some(actual) = resolve_type(value, ctx.scope) {
                                if !types_compatible(&expected, &actual) {
                                    errors.push(RuleError::new(errors::FIELD_TYPE_MISMATCH, format!("{}.{} expects {} but got {}", name, field_name, expected, actual), Some(ctx.func.qualified_name.clone())));
                                }
                            }
                        }
                    }
                }
            }
            // self.field access: check field exists on struct
            Expr::FieldAccess { target, field } => {
                if let Expr::SelfRef = target.as_ref() {
                    if let Some(parent) = ctx.func.parent_struct {
                        let key = format!("self.{}", field);
                        if !ctx.scope.contains_key(&key)
                            && !ctx.check.registry.has_method(parent, field)
                        {
                            // Also check struct's own methods
                            let has_own = ctx.check.file.items.iter().any(|item| {
                                if let Item::Struct(s) = item {
                                    s.name == parent && (
                                        s.signatures.iter().any(|sig| sig.name == *field) ||
                                        s.methods.iter().any(|m| m.name == *field)
                                    )
                                } else { false }
                            });
                            if !has_own {
                                errors.push(RuleError::new(errors::UNKNOWN_FIELD, format!("'{}' has no field or method '{}'", parent, field), Some(ctx.func.qualified_name.clone())));
                            }
                        }
                    }
                }
            }
            // Function call: check arg types match param types
            Expr::Call { target, args } => {
                if let Expr::Ident(fn_name) = target.as_ref() {
                    // Look up the function in the current file
                    let mut found = false;
                    for item in &ctx.check.file.items {
                        if let Item::Function(f) = item {
                            if f.name == *fn_name {
                                check_call_args(fn_name, &f.params, args, ctx, &mut errors);
                                found = true;
                                break;
                            }
                        }
                    }
                    // Check imported functions
                    if !found {
                        if let Some(resolved) = crate::resolve::find_imported_fn(fn_name, ctx.check.file, ctx.check.source_dir.as_deref()) {
                            check_call_args(fn_name, &resolved.params, args, ctx, &mut errors);
                        }
                    }
                }
                // Static method call: Type.method(args)
                if let Expr::FieldAccess { target: obj, field: method_name } = target.as_ref() {
                    if let Expr::Ident(type_name) = obj.as_ref() {
                        // Look up struct methods
                        for item in &ctx.check.file.items {
                            if let Item::Struct(s) = item {
                                if s.name == *type_name {
                                    if let Some(sig) = s.signatures.iter().find(|sig| sig.name == *method_name) {
                                        let call_name = format!("{}.{}", type_name, method_name);
                                        check_call_args(&call_name, &sig.params, args, ctx, &mut errors);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        errors
    }
}

fn check_call_args(call_name: &str, params: &[Param], args: &[Expr], ctx: &ExprContext, errors: &mut Vec<RuleError>) {
    for (i, param) in params.iter().enumerate() {
        if let Some(arg) = args.get(i) {
            let expected = type_ref_to_name(&param.type_ref);
            if let Some(actual) = resolve_type(arg, ctx.scope) {
                if !types_compatible(&expected, &actual) {
                    errors.push(RuleError::new(errors::ARG_TYPE_MISMATCH, format!("{}() param '{}' expects {} but got {}", call_name, param.name, expected, actual), Some(ctx.func.qualified_name.clone())));
                }
            }
        }
    }
}

/// Check if two type names are compatible (same type, or satisfies relationship)
fn types_compatible(expected: &str, actual: &str) -> bool {
    if expected == actual { return true; }
    // Strip generic args for base comparison — Array<String> accepts Array<String>
    let exp_base = expected.split('<').next().unwrap_or(expected);
    let act_base = actual.split('<').next().unwrap_or(actual);
    if exp_base == act_base { return true; }
    // Null is compatible with nullable types
    if expected.ends_with('?') && actual == "null" { return true; }
    false
}

#[cfg(test)]
mod tests {
    use crate::check;

    fn errors(src: &str) -> Vec<crate::errors::RuleError> {
        check::check(&crate::parse::parse(src))
    }

    #[test]
    fn return_type_mismatch_caught() {
        let e = errors(r#"pub fn foo() -> Number { return "hello" test { self() == 0 } }"#);
        assert!(e.iter().any(|e| e.code == "return-type-mismatch"),
            "expected return-type-mismatch, got: {:?}", e);
    }

    #[test]
    fn return_type_correct() {
        let e = errors(r#"pub fn foo() -> Number { return 42 test { self() == 42 } }"#);
        assert!(!e.iter().any(|e| e.code == "return-type-mismatch"));
    }

    #[test]
    fn arg_type_mismatch_caught() {
        let e = errors(r#"
            pub fn greet(name: String) -> String { return name test { self("hi") == "hi" } }
            pub fn caller() -> String { return greet(42) crash { greet -> halt } test { self() == "hi" } }
        "#);
        assert!(e.iter().any(|e| e.code == "arg-type-mismatch"),
            "expected arg-type-mismatch, got: {:?}", e);
    }

    #[test]
    fn arg_type_correct() {
        let e = errors(r#"
            pub fn greet(name: String) -> String { return name test { self("hi") == "hi" } }
            pub fn caller() -> String { return greet("world") crash { greet -> halt } test { self() == "world" } }
        "#);
        assert!(!e.iter().any(|e| e.code == "arg-type-mismatch"));
    }

    #[test]
    fn constructor_field_type_mismatch_caught() {
        let e = errors(r#"pub struct Email { value: String }{} pub fn make() -> Number { const e = Email { value: 42 } return 0 test { self() == 0 } }"#);
        assert!(e.iter().any(|e| e.code == "field-type-mismatch"),
            "expected field-type-mismatch, got: {:?}", e);
    }

    #[test]
    fn constructor_field_type_correct() {
        let e = errors(r#"pub struct Email { value: String }{} pub fn make() -> Number { const e = Email { value: "test" } return 0 test { self() == 0 } }"#);
        assert!(!e.iter().any(|e| e.code == "field-type-mismatch"));
    }

    #[test]
    fn unknown_self_field_caught() {
        let e = errors(r#"struct Point { x: Number }{ fn bad() -> Number { return self.nonexistent test { self() == 0 } } }"#);
        assert!(e.iter().any(|e| e.code == "unknown-field"),
            "expected unknown-field, got: {:?}", e);
    }

    #[test]
    fn valid_self_field() {
        let e = errors(r#"struct Point { x: Number }{ fn get_x() -> Number { return self.x test { self() == 0 } } }"#);
        assert!(!e.iter().any(|e| e.code == "unknown-field"));
    }

    #[test]
    fn type_annotation_mismatch_caught() {
        let e = errors(r#"pub fn foo() -> Number { let x: Number = "hello" return x test { self() == 0 } }"#);
        assert!(e.iter().any(|e| e.code == "type-annotation-mismatch"),
            "expected type-annotation-mismatch, got: {:?}", e);
    }

    #[test]
    fn type_annotation_correct() {
        let e = errors(r#"pub fn foo() -> Number { let x: Number = 42 return x test { self() == 42 } }"#);
        assert!(!e.iter().any(|e| e.code == "type-annotation-mismatch"));
    }

    // ─── Adversarial: return types with Ok and err ─────

    #[test]
    fn return_ok_type_no_mismatch() {
        let e = errors(r#"pub fn p() -> Ok { return Ok test { self() == Ok } }"#);
        assert!(!e.iter().any(|e| e.code == "return-type-mismatch"),
            "returning Ok from fn -> Ok should pass, got: {:?}", e);
    }

    #[test]
    fn return_err_function_with_value() {
        // fn returning String, err — returning "hello" should match the value part
        let e = errors(r#"pub fn p() -> String, err { err fail = "failed" return "hello" test { self() == "hello" } }"#);
        assert!(!e.iter().any(|e| e.code == "return-type-mismatch"),
            "returning String from fn -> String, err should pass, got: {:?}", e);
    }

    // ─── Adversarial: struct constructors ─────

    #[test]
    fn struct_constructor_partial_fields() {
        // Email has value: String and status: Number, but we only provide value.
        // Checker only validates types of provided fields, not missing ones.
        let e = errors(r#"pub struct Email { value: String status: Number }{} pub fn make() -> Number { const e = Email { value: "x" } return 0 test { self() == 0 } }"#);
        let has_missing = e.iter().any(|e| e.code == "missing-field");
        assert!(!has_missing,
            "checker does not currently catch missing fields — if this fails, detection was added: {:?}", e);
    }

    #[test]
    fn struct_constructor_extra_field() {
        // Email has only value: String, but we provide an extra field.
        let e = errors(r#"pub struct Email { value: String }{} pub fn make() -> Number { const e = Email { value: "x", extra: "y" } return 0 test { self() == 0 } }"#);
        let has_extra = e.iter().any(|e| e.code == "unknown-field" || e.code == "extra-field");
        assert!(!has_extra,
            "checker does not currently catch extra fields — if this fails, detection was added: {:?}", e);
    }

    // ─── Adversarial: nested call arg types ─────

    #[test]
    fn nested_call_arg_type() {
        // greet(n.toString()) where n: Number and greet wants String
        // toString() returns String, so this should pass
        let e = errors(r#"
            pub fn greet(name: String) -> String { return name test { self("hi") == "hi" } }
            pub fn p(n: Number) -> String { return greet(n.toString()) crash { greet -> halt n.toString -> halt } test { self(1) == "1" } }
        "#);
        assert!(!e.iter().any(|e| e.code == "arg-type-mismatch"),
            "greet(n.toString()) should pass — toString returns String, got: {:?}", e);
    }

    // ─── Adversarial: self method access ─────

    #[test]
    fn self_method_access_ok() {
        // Calling self.validate() in a struct method where validate is
        // declared in the contract (signature) block.
        let e = errors(r#"
            struct Email { value: String
                validate(input: String) -> Bool
                check() -> Bool
            }{
                fn validate(input: String) -> Bool { return input.includes("@") crash { input.includes -> halt } test { self("a@b") == true } }
                fn check() -> Bool { return self.validate(self.value) crash { self.validate -> halt } test { self() == true } }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "unknown-field"),
            "self.validate() should not trigger unknown-field when declared in contract block, got: {:?}", e);
    }

    // ─── Return null / err enforcement ─────

    #[test]
    fn return_null_from_non_nullable_caught() {
        let e = errors(r#"pub fn foo() -> String { return null test { self() == "hi" } }"#);
        assert!(e.iter().any(|e| e.code == "return-null"),
            "expected return-null, got: {:?}", e);
    }

    #[test]
    fn return_null_from_nullable_ok() {
        let e = errors(r#"pub fn foo() -> String | null { return null test { self() == null } }"#);
        assert!(!e.iter().any(|e| e.code == "return-null"),
            "return null from nullable should pass, got: {:?}", e);
    }

    #[test]
    fn return_value_from_ok_fn_caught() {
        let e = errors(r#"pub fn foo() -> Ok { return "hello" test { self() is Ok } }"#);
        assert!(e.iter().any(|e| e.code == "return-type-mismatch"),
            "expected return-type-mismatch for value from Ok fn, got: {:?}", e);
    }

    #[test]
    fn return_ok_from_ok_fn_passes() {
        let e = errors(r#"pub fn foo() -> Ok { return Ok test { self() is Ok } }"#);
        assert!(!e.iter().any(|e| e.code == "return-type-mismatch"));
    }

    #[test]
    fn return_err_from_non_err_fn_caught() {
        let e = errors(r#"pub fn foo() -> String { return err.bad test { self() == "hi" } }"#);
        assert!(e.iter().any(|e| e.code == "return-err-not-declared"),
            "expected return-err-not-declared, got: {:?}", e);
    }

    #[test]
    fn return_err_from_err_fn_ok() {
        let e = errors(r#"pub fn foo() -> String, err { return err.bad test { self("") is err.bad self("ok") == "ok" } }"#);
        assert!(!e.iter().any(|e| e.code == "return-err-not-declared"));
    }

    // ─── nullable-return ─────

    #[test]
    fn nullable_return_blocked() {
        let e = errors(r#"
            pub fn find(id: String) -> String | null {
                if id == "" { return null }
                return "found"
                test { self("a") == "found" }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "nullable-return"),
            "expected nullable-return, got: {:?}", e);
    }

    #[test]
    fn err_return_allowed() {
        let e = errors(r#"
            pub fn find(id: String) -> String, err {
                err not_found = "not found"
                if id == "" { return err.not_found }
                return "found"
                test { self("a") == "found" self("") is err.not_found }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "nullable-return"),
            "err return should not trigger nullable-return, got: {:?}", e);
    }

    #[test]
    fn nullable_param_blocked() {
        let e = errors(r#"
            pub fn greet(name: String | null) -> String {
                if name == null { return "hello" }
                return name
                test { self("cam") == "cam" }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "nullable-type"),
            "nullable param should be flagged, got: {:?}", e);
    }

    #[test]
    fn nullable_struct_field_blocked() {
        let e = errors(r#"
            pub struct Profile {
                name: String
                bio: String | null
                greet() -> String
            }{
                pub fn greet() -> String {
                    return self.name
                    test {}
                }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "nullable-type"),
            "nullable field should be flagged, got: {:?}", e);
    }

    #[test]
    fn non_nullable_types_ok() {
        let e = errors(r#"
            pub fn greet(name: String) -> String {
                return name
                test { self("cam") == "cam" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "nullable-type"));
    }
}
