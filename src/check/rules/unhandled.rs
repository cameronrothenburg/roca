use crate::ast::*;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::FnCheckContext;

pub struct UnhandledErrorsRule;

impl Rule for UnhandledErrorsRule {
    fn name(&self) -> &'static str { "unhandled-errors" }

    fn check_function(&self, ctx: &FnCheckContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        let f = ctx.func.def;

        // Only applies to pub functions or struct methods (where the struct may be pub)
        // Private standalone fns can propagate freely
        if !f.is_pub && ctx.func.parent_struct.is_none() {
            return errors;
        }

        // For struct methods, check if the parent struct is pub
        if let Some(parent) = ctx.func.parent_struct {
            let struct_is_pub = ctx.check.file.items.iter().any(|item| {
                matches!(item, Item::Struct(s) if s.name == parent && s.is_pub)
            });
            if !struct_is_pub {
                return errors;
            }
        }

        let crash = match &f.crash {
            Some(c) => c,
            None => return errors,
        };

        let own_errors = self.own_error_names(ctx);

        // For each handler that ends in halt, look up the callee's errors
        for handler in &crash.handlers {
            let halting_err_names = self.halting_error_names(handler, ctx);
            for err_name in halting_err_names {
                if !own_errors.contains(&err_name) {
                    errors.push(RuleError {
                        code: errors::UNHANDLED_ERROR.into(),
                        message: format!(
                            "error '{}' propagates via halt in '{}' but is not declared",
                            err_name, f.name,
                        ),
                        context: Some(ctx.func.qualified_name.clone()),
                    });
                }
            }
        }

        errors
    }
}

impl UnhandledErrorsRule {
    /// Get the error names declared by the current function.
    /// For struct methods: look up the parent struct's signature errors.
    /// For standalone fns: collect from ReturnErr statements in the body
    ///   (since FnDef.errors is always empty for parsed standalone fns).
    fn own_error_names(&self, ctx: &FnCheckContext) -> Vec<String> {
        if let Some(parent) = ctx.func.parent_struct {
            // Struct method — look up the signature in the parent struct
            for item in &ctx.check.file.items {
                if let Item::Struct(s) = item {
                    if s.name == parent {
                        if let Some(sig) = s.signatures.iter().find(|sig| sig.name == ctx.func.def.name) {
                            return sig.errors.iter().map(|e| e.name.clone()).collect();
                        }
                    }
                }
            }
            vec![]
        } else {
            collect_returned_error_names(&ctx.func.def.body)
        }
    }

    /// For a given crash handler, return the error names that propagate via halt.
    /// If the handler is Simple and ends in halt, return ALL errors from the callee.
    /// If Detailed, only return errors from arms that end in halt (+ default if halt).
    fn halting_error_names(&self, handler: &CrashHandler, ctx: &FnCheckContext) -> Vec<String> {
        let callee_errors = self.lookup_callee_errors(&handler.call, ctx);

        match &handler.strategy {
            CrashHandlerKind::Simple(chain) => {
                if chain_ends_in_halt(chain) {
                    callee_errors
                } else {
                    vec![]
                }
            }
            CrashHandlerKind::Detailed { arms, default } => {
                let mut propagated = Vec::new();
                let mut handled_names: Vec<String> = Vec::new();

                for arm in arms {
                    if chain_ends_in_halt(&arm.chain) {
                        propagated.push(arm.err_name.clone());
                    }
                    handled_names.push(arm.err_name.clone());
                }

                // Default chain covers any callee error not explicitly armed
                if let Some(def_chain) = default {
                    if chain_ends_in_halt(def_chain) {
                        for e in &callee_errors {
                            if !handled_names.contains(e) && !propagated.contains(e) {
                                propagated.push(e.clone());
                            }
                        }
                    }
                }

                propagated
            }
        }
    }

    /// Look up the declared errors of the function/method being called.
    fn lookup_callee_errors(&self, call: &str, ctx: &FnCheckContext) -> Vec<String> {
        // call can be "fn_name" or "receiver.method" or "a.b.method"
        let parts: Vec<&str> = call.split('.').collect();

        if parts.len() == 1 {
            // Top-level function or extern fn
            let name = parts[0];
            for item in &ctx.check.file.items {
                match item {
                    Item::Function(f) if f.name == name => {
                        return f.errors.iter().map(|e| e.name.clone()).collect();
                    }
                    Item::ExternFn(f) if f.name == name => {
                        return f.errors.iter().map(|e| e.name.clone()).collect();
                    }
                    _ => {}
                }
            }
        } else {
            // Dotted call: resolve the last segment as method name
            let method_name = parts[parts.len() - 1];

            // Try to find via struct methods in the file
            for item in &ctx.check.file.items {
                if let Item::Struct(s) = item {
                    // Check signatures for the method
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

            // For chained access like env.kv.get, walk the chain
            if parts.len() > 2 {
                // Try resolving intermediate types through the registry
                let mut current_type: Option<String> = None;

                // Check if first part is a param with a known type
                for param in &ctx.func.def.params {
                    if param.name == parts[0] {
                        current_type = Some(type_ref_base_name(&param.type_ref));
                        break;
                    }
                }

                if let Some(mut typ) = current_type {
                    // Walk the intermediate fields to resolve the final type
                    for &segment in &parts[1..parts.len() - 1] {
                        if let Some(contract) = ctx.check.registry.get(&typ) {
                            if let Some(field) = contract.fields.iter().find(|f| f.name == segment) {
                                typ = type_ref_base_name(&field.type_ref);
                                continue;
                            }
                        }
                        return vec![];
                    }
                    // Now look up the method on the resolved type
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
    fn halt_with_declared_errors_passes() {
        // Function uses return err.net — so the error is declared
        let e = errors(r#"
            extern fn fetch(url: String) -> String, err {
                err net = "network error"
                mock { fetch -> "ok" }
            }
            pub fn go(url: String) -> String, err {
                let r, e = wait fetch(url)
                if e != null { return err.net }
                return r
                crash { fetch -> halt }
                test { self("x") == "ok" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "unhandled-error"),
            "expected no unhandled-error, got: {:?}", e);
    }

    #[test]
    fn halt_without_declared_errors_fails() {
        // Function does NOT use return err.net — so the callee error is undeclared
        let e = errors(r#"
            extern fn fetch(url: String) -> String, err {
                err net = "network error"
                mock { fetch -> "ok" }
            }
            pub fn go(url: String) -> String {
                let r, e = wait fetch(url)
                if e != null { return r }
                return r
                crash { fetch -> halt }
                test { self("x") == "ok" }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "unhandled-error"),
            "expected unhandled-error, got: {:?}", e);
    }

    #[test]
    fn fallback_no_error() {
        let e = errors(r#"
            extern fn fetch(url: String) -> String, err {
                err net = "network error"
                mock { fetch -> "ok" }
            }
            pub fn go(url: String) -> String {
                let r, e = wait fetch(url)
                if e != null { return "default" }
                return r
                crash { fetch -> fallback("default") }
                test { self("x") == "ok" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "unhandled-error"),
            "expected no unhandled-error with fallback, got: {:?}", e);
    }

    #[test]
    fn skip_no_error() {
        let e = errors(r#"
            extern fn fetch(url: String) -> String, err {
                err net = "network error"
                mock { fetch -> "ok" }
            }
            pub fn go(url: String) -> String {
                let r, e = wait fetch(url)
                if e != null { return "" }
                return r
                crash { fetch -> skip }
                test { self("x") == "ok" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "unhandled-error"),
            "expected no unhandled-error with skip, got: {:?}", e);
    }

    #[test]
    fn panic_no_error() {
        let e = errors(r#"
            extern fn fetch(url: String) -> String, err {
                err net = "network error"
                mock { fetch -> "ok" }
            }
            pub fn go(url: String) -> String {
                let r, e = wait fetch(url)
                if e != null { return "" }
                return r
                crash { fetch -> panic }
                test { self("x") == "ok" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "unhandled-error"),
            "expected no unhandled-error with panic, got: {:?}", e);
    }

    #[test]
    fn struct_method_propagates_undeclared() {
        let e = errors(r#"
            extern fn fetch(url: String) -> String, err {
                err net = "network error"
                mock { fetch -> "ok" }
            }
            pub struct Api {
                fetch(url: String) -> String {
                }
            }{
                fn fetch(url: String) -> String {
                    let r, e = wait fetch(url)
                    if e != null { return "" }
                    return r
                    crash { fetch -> halt }
                    test { self("x") == "ok" }
                }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "unhandled-error"),
            "expected unhandled-error for struct method without declared errors, got: {:?}", e);
    }
}
