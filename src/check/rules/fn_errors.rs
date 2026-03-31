//! Rule: no-fn-error-def
//! Standalone functions cannot define new errors — only struct methods and extern fn can.
//! Functions may only propagate errors from callees via crash halt.

use crate::ast::*;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::FnCheckContext;

pub struct FnErrorsRule;


impl Rule for FnErrorsRule {
    fn name(&self) -> &'static str { "fn-errors" }

    fn check_function(&self, ctx: &FnCheckContext) -> Vec<RuleError> {
        // Only applies to standalone functions (not struct methods, not extern fn)
        if ctx.func.parent_struct.is_some() {
            return vec![];
        }

        let returned_errors = collect_returned_error_names(&ctx.func.def.body);
        if returned_errors.is_empty() {
            return vec![];
        }

        // Collect allowed error names from crash halt entries
        let allowed = self.allowed_error_names(ctx);

        let mut errors = Vec::new();
        for err_name in &returned_errors {
            if !allowed.contains(err_name) {
                errors.push(RuleError::new(
                    errors::FN_ERROR_DEF,
                    format!(
                        "standalone function '{}' cannot define error '{}' — only struct methods and extern fn can define errors",
                        ctx.func.def.name, err_name
                    ),
                    Some(ctx.func.qualified_name.clone()),
                ));
            }
        }

        errors
    }
}

impl FnErrorsRule {
    /// Collect all error names that are allowed in this function via crash halt propagation.
    fn allowed_error_names(&self, ctx: &FnCheckContext) -> Vec<String> {
        let crash = match &ctx.func.def.crash {
            Some(c) => c,
            None => return vec![],
        };

        let mut allowed = Vec::new();

        for handler in &crash.handlers {
            let callee_errors = self.lookup_callee_errors(&handler.call, ctx);

            match &handler.strategy {
                CrashHandlerKind::Simple(chain) => {
                    if chain_ends_in_halt(chain) {
                        for e in callee_errors {
                            if !allowed.contains(&e) {
                                allowed.push(e);
                            }
                        }
                    }
                }
                CrashHandlerKind::Detailed { arms, default } => {
                    let mut handled_names = Vec::new();
                    for arm in arms {
                        if chain_ends_in_halt(&arm.chain) {
                            if !allowed.contains(&arm.err_name) {
                                allowed.push(arm.err_name.clone());
                            }
                        }
                        handled_names.push(arm.err_name.clone());
                    }
                    if let Some(def_chain) = default {
                        if chain_ends_in_halt(def_chain) {
                            for e in &callee_errors {
                                if !handled_names.contains(e) && !allowed.contains(e) {
                                    allowed.push(e.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        allowed
    }

    /// Look up declared errors of the callee (function/method being called).
    fn lookup_callee_errors(&self, call: &str, ctx: &FnCheckContext) -> Vec<String> {
        let parts: Vec<&str> = call.split('.').collect();

        if parts.len() == 1 {
            let name = parts[0];
            for item in &ctx.check.file.items {
                match item {
                    Item::Function(f) if f.name == name => {
                        // For standalone fn callees, their errors come from return err.x
                        // (since FnDef.errors is empty for parsed standalone fns)
                        return collect_returned_error_names(&f.body);
                    }
                    Item::ExternFn(f) if f.name == name => {
                        return f.errors.iter().map(|e| e.name.clone()).collect();
                    }
                    _ => {}
                }
            }
            // Check imported functions
            if let Some(resolved) = crate::resolve::find_imported_fn(name, ctx.check.file, ctx.check.source_dir.as_deref()) {
                return resolved.errors.iter().map(|e| e.name.clone()).collect();
            }
        } else {
            let method_name = parts[parts.len() - 1];

            // Try struct methods in file
            for item in &ctx.check.file.items {
                if let Item::Struct(s) = item {
                    if let Some(sig) = s.signatures.iter().find(|sig| sig.name == method_name) {
                        return sig.errors.iter().map(|e| e.name.clone()).collect();
                    }
                }
            }

            // Try extern contracts via registry
            if let Some(contract) = ctx.check.registry.get(parts[0]) {
                if let Some(sig) = contract.functions.iter().find(|f| f.name == method_name) {
                    return sig.errors.iter().map(|e| e.name.clone()).collect();
                }
            }

            // Chained access (e.g. env.kv.get)
            if parts.len() > 2 {
                let mut current_type: Option<String> = None;
                for param in &ctx.func.def.params {
                    if param.name == parts[0] {
                        current_type = Some(type_ref_base_name(&param.type_ref));
                        break;
                    }
                }
                if let Some(mut typ) = current_type {
                    for &segment in &parts[1..parts.len() - 1] {
                        if let Some(contract) = ctx.check.registry.get(&typ) {
                            if let Some(field) = contract.fields.iter().find(|f| f.name == segment) {
                                typ = type_ref_base_name(&field.type_ref);
                                continue;
                            }
                        }
                        return vec![];
                    }
                    if let Some(contract) = ctx.check.registry.get(&typ) {
                        if let Some(sig) = contract.functions.iter().find(|f| f.name == method_name) {
                            return sig.errors.iter().map(|e| e.name.clone()).collect();
                        }
                    }
                }
            }
        }

        vec![]
    }
}

fn type_ref_base_name(t: &TypeRef) -> String {
    match t {
        TypeRef::String => "String".into(),
        TypeRef::Number => "Number".into(),
        TypeRef::Bool => "Bool".into(),
        TypeRef::Ok => "Ok".into(),
        TypeRef::Named(n) => n.clone(),
        TypeRef::Generic(n, _) => n.clone(),
        TypeRef::Nullable(inner) => type_ref_base_name(inner),
        TypeRef::Fn(_, ret) => type_ref_base_name(ret),
    }
}

fn chain_ends_in_halt(chain: &CrashChain) -> bool {
    chain.last().map_or(false, |step| matches!(step, CrashStep::Halt))
}

#[cfg(test)]
mod tests {
    use crate::check;

    fn errors(src: &str) -> Vec<crate::errors::RuleError> {
        check::check(&crate::parse::parse(src))
    }

    #[test]
    fn standalone_fn_defining_error_rejected() {
        let e = errors(r#"
            pub fn validate(s: String) -> String, err {
                if s == "" { return err.empty }
                return s
                test { self("ok") == "ok" self("") is err.empty }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "no-fn-error-def"),
            "expected no-fn-error-def for standalone fn defining errors, got: {:?}", e);
    }

    #[test]
    fn struct_method_defining_error_allowed() {
        let e = errors(r#"
            /// Email type
            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err missing = "required"
                }
            }{
                fn validate(raw: String) -> Email, err {
                    if raw == "" { return err.missing }
                    return Email { value: raw }
                    test { self("a@b.com") is Ok self("") is err.missing }
                }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "no-fn-error-def"),
            "struct methods should be allowed to define errors, got: {:?}", e);
    }

    #[test]
    fn fn_propagating_callee_errors_allowed() {
        let e = errors(r#"
            /// Email type
            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err missing = "required"
                }
            }{
                fn validate(raw: String) -> Email, err {
                    if raw == "" { return err.missing }
                    return Email { value: raw }
                    test { self("a@b.com") is Ok self("") is err.missing }
                }
            }

            /// Creates an email
            pub fn create_email(raw: String) -> Email, err {
                const email = Email.validate(raw)
                return email
                crash { Email.validate -> halt }
                test { self("a@b.com") is Ok }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "no-fn-error-def"),
            "fn propagating callee errors via halt should be allowed, got: {:?}", e);
    }

    #[test]
    fn fn_with_no_errors_clean() {
        let e = errors(r#"
            /// Adds numbers
            pub fn add(a: Number, b: Number) -> Number {
                return a + b
                test { self(1, 2) == 3 }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "no-fn-error-def"));
    }
}
