//! TypeScript declaration (.d.ts) generation from a Roca source file.
//! Emits type signatures for exported functions, structs, contracts, and enums.

use crate::ast::*;

/// Generate TypeScript declaration file (.d.ts) from a Roca source file.
pub fn emit_dts(file: &SourceFile) -> String {
    let mut lines = Vec::new();

    // Emit import statements for referenced types from other files
    for item in &file.items {
        if let Item::Import(imp) = item {
            if let ImportSource::Path(path) = &imp.source {
                let dts_path = path.replace(".roca", ".js");
                lines.push(format!("import {{ {} }} from \"{}\";", imp.names.join(", "), dts_path));
            }
        }
    }
    if lines.iter().any(|l| l.starts_with("import")) {
        lines.push(String::new());
    }

    // Check if any exports use error returns — if so, emit shared types
    let has_err_returns = file.items.iter().any(|item| match item {
        Item::Function(f) if f.is_pub => f.returns_err,
        Item::Struct(s) if s.is_pub => s.methods.iter().any(|m| m.is_pub && m.returns_err),
        Item::ExternContract(c) if c.is_pub => c.functions.iter().any(|f| f.returns_err),
        _ => false,
    });

    if has_err_returns {
        lines.push("export interface RocaError {\n  name: string;\n  message: string;\n}".to_string());
        lines.push("export type RocaResult<T> = { value: T; err: null } | { value: null; err: RocaError };".to_string());
        lines.push(String::new());
    }

    // Collect satisfies methods by struct name
    let mut satisfies_map: std::collections::HashMap<&str, Vec<&FnDef>> = std::collections::HashMap::new();
    for item in &file.items {
        if let Item::Satisfies(sat) = item {
            for m in &sat.methods {
                satisfies_map.entry(&sat.struct_name).or_default().push(m);
            }
        }
    }

    for item in &file.items {
        match item {
            Item::Function(f) if f.is_pub => {
                let is_async = super::functions::body_has_wait(&f.body);
                lines.push(emit_fn_decl(f, is_async));
            }
            Item::Struct(s) if s.is_pub => {
                let sat_methods = satisfies_map.get(s.name.as_str()).map(|v| v.as_slice()).unwrap_or(&[]);
                lines.push(emit_class_decl(s, sat_methods));
            }
            Item::Enum(e) if e.is_pub => {
                lines.push(emit_enum_decl(e));
            }
            Item::ExternContract(c) if c.is_pub => {
                lines.push(emit_extern_contract_decl(c));
            }
            _ => {}
        }
    }

    lines.join("\n")
}

fn type_to_ts(t: &TypeRef) -> String {
    match t {
        TypeRef::String => "string".to_string(),
        TypeRef::Number => "number".to_string(),
        TypeRef::Bool => "boolean".to_string(),
        TypeRef::Ok => "void".to_string(),
        TypeRef::Named(n) => n.clone(),
        TypeRef::Generic(name, args) if name == "Optional" => {
            if let Some(inner) = args.first() {
                format!("{} | undefined", type_to_ts(inner))
            } else {
                "unknown | undefined".to_string()
            }
        }
        TypeRef::Generic(name, args) => {
            let ts_args: Vec<String> = args.iter().map(|a| type_to_ts(a)).collect();
            format!("{}<{}>", name, ts_args.join(", "))
        }
        TypeRef::Nullable(inner) => format!("{} | null", type_to_ts(inner)),
        TypeRef::Fn(params, ret) => {
            let p: Vec<String> = params.iter().enumerate()
                .map(|(i, t)| format!("arg{}: {}", i, type_to_ts(t))).collect();
            format!("({}) => {}", p.join(", "), type_to_ts(ret))
        }
    }
}

fn params_to_ts(params: &[Param]) -> String {
    params.iter()
        .map(|p| format!("{}: {}", p.name, type_to_ts(&p.type_ref)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// For error-returning functions, the JS runtime returns [value, err] tuples
fn return_type_to_ts(return_type: &TypeRef, returns_err: bool) -> String {
    if returns_err {
        format!("RocaResult<{}>", type_to_ts(return_type))
    } else {
        type_to_ts(return_type)
    }
}

fn emit_fn_decl(f: &FnDef, is_async: bool) -> String {
    let ret = return_type_to_ts(&f.return_type, f.returns_err);
    let ret_str = if is_async {
        format!("Promise<{}>", ret)
    } else {
        ret
    };
    let type_params = if f.type_params.is_empty() {
        String::new()
    } else {
        let params: Vec<String> = f.type_params.iter().map(|tp| {
            if let Some(constraint) = &tp.constraint {
                format!("{} extends {}", tp.name, constraint)
            } else {
                tp.name.clone()
            }
        }).collect();
        format!("<{}>", params.join(", "))
    };
    format!("export declare function {}{}({}): {};", f.name, type_params, params_to_ts(&f.params), ret_str)
}

fn emit_class_decl(s: &StructDef, sat_methods: &[&FnDef]) -> String {
    let mut lines = Vec::new();
    lines.push(format!("export declare class {} {{", s.name));

    // Fields
    for field in &s.fields {
        lines.push(format!("  {}: {};", field.name, type_to_ts(&field.type_ref)));
    }

    // Constructor
    if !s.fields.is_empty() {
        let field_types: Vec<String> = s.fields.iter()
            .map(|f| format!("{}: {}", f.name, type_to_ts(&f.type_ref)))
            .collect();
        lines.push(format!("  constructor(init: {{ {} }});", field_types.join("; ")));
    }

    // Methods from contract block (signatures) — these define the public API
    for sig in &s.signatures {
        // Determine static vs instance from the impl body
        let is_static = s.methods.iter()
            .find(|m| m.name == sig.name)
            .map(|m| !super::structs::body_uses_self(&m.body) && !m.params.iter().any(|p| p.name == "self"))
            .unwrap_or(true); // default to static if no impl found
        let is_async = s.methods.iter()
            .find(|m| m.name == sig.name)
            .map(|m| super::functions::body_has_wait(&m.body))
            .unwrap_or(false);
        let ret = return_type_to_ts(&sig.return_type, sig.returns_err);
        let ret_str = if is_async { format!("Promise<{}>", ret) } else { ret };
        let prefix = if is_static { "static " } else { "" };
        lines.push(format!("  {}{}({}): {};", prefix, sig.name, params_to_ts(&sig.params), ret_str));
    }

    // Satisfies methods — always instance
    for method in sat_methods {
        let is_async = super::functions::body_has_wait(&method.body);
        let ret = return_type_to_ts(&method.return_type, method.returns_err);
        let ret_str = if is_async { format!("Promise<{}>", ret) } else { ret };
        lines.push(format!("  {}({}): {};", method.name, params_to_ts(&method.params), ret_str));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

fn emit_extern_contract_decl(c: &ContractDef) -> String {
    let mut lines = Vec::new();
    lines.push(format!("export interface {} {{", c.name));

    // Fields
    for field in &c.fields {
        lines.push(format!("  {}: {};", field.name, type_to_ts(&field.type_ref)));
    }

    // Methods — error-returning ones use RocaResult<T>
    for sig in &c.functions {
        let ret = return_type_to_ts(&sig.return_type, sig.returns_err);
        lines.push(format!("  {}({}): {};", sig.name, params_to_ts(&sig.params), ret));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

fn emit_enum_decl(e: &EnumDef) -> String {
    use super::ast_helpers::{TAG_FIELD, positional_field};
    let mut lines = Vec::new();
    lines.push(format!("export declare const {}: {{", e.name));
    for v in &e.variants {
        match &v.value {
            EnumValue::String(s) => {
                lines.push(format!("  readonly {}: \"{}\";", v.name, s));
            }
            EnumValue::Number(n) => {
                let val = if *n == (*n as i64) as f64 { format!("{}", *n as i64) } else { format!("{}", n) };
                lines.push(format!("  readonly {}: {};", v.name, val));
            }
            EnumValue::Data(types) => {
                let params: Vec<String> = types.iter().enumerate()
                    .map(|(i, t)| format!("{}: {}", positional_field(i), type_to_ts(t)))
                    .collect();
                let fields: String = types.iter().enumerate()
                    .map(|(i, t)| format!(", {}: {}", positional_field(i), type_to_ts(t)))
                    .collect();
                lines.push(format!("  {}({}): {{ {}: \"{}\"{} }};",
                    v.name, params.join(", "), TAG_FIELD, v.name, fields,
                ));
            }
            EnumValue::Unit => {
                lines.push(format!("  readonly {}: {{ {}: \"{}\" }};", v.name, TAG_FIELD, v.name));
            }
        };
    }
    lines.push("};".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn pub_function_declaration() {
        let dts = emit_dts(&parse::parse(r#"
            pub fn greet(name: String) -> String {
                return name
                test { self("a") == "a" }
            }
        "#));
        assert!(dts.contains("export declare function greet(name: string): string;"));
    }

    #[test]
    fn private_function_excluded() {
        let dts = emit_dts(&parse::parse(r#"
            fn helper() -> String {
                return "x"
                test { self() == "x" }
            }
        "#));
        assert!(dts.is_empty());
    }

    #[test]
    fn struct_with_fields_and_methods() {
        let dts = emit_dts(&parse::parse(r#"
            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err invalid = "invalid"
                }
            }{
                pub fn validate(raw: String) -> Email, err {
                    if raw == "" { return err.invalid }
                    return Email { value: raw }
                    test { self("a@b.com") is Ok self("") is err.invalid }
                }
            }
        "#));
        assert!(dts.contains("export interface RocaError"), "should have RocaError interface, got:\n{}", dts);
        assert!(dts.contains("export type RocaResult<T>"), "should have RocaResult type, got:\n{}", dts);
        assert!(dts.contains("export declare class Email"));
        assert!(dts.contains("value: string;"));
        assert!(dts.contains("constructor(init: { value: string });"));
        assert!(dts.contains("static validate(raw: string): RocaResult<Email>;"), "got:\n{}", dts);
    }

    #[test]
    fn enum_declaration() {
        let dts = emit_dts(&parse::parse(r#"
            pub enum Status { active = "active", suspended = "suspended" }
        "#));
        assert!(dts.contains("export declare const Status"));
        assert!(dts.contains("readonly active: \"active\""));
    }

    #[test]
    fn err_returning_function_uses_roca_result() {
        let dts = emit_dts(&parse::parse(r#"
            pub fn validate(s: String) -> String, err {
                err empty = "empty"
                return s
                test { self("a") == "a" }
            }
        "#));
        assert!(dts.contains("RocaResult<string>"), "got:\n{}", dts);
        assert!(dts.contains("export interface RocaError"));
        assert!(dts.contains("export type RocaResult<T>"));
    }

    #[test]
    fn async_function_returns_promise() {
        let dts = emit_dts(&parse::parse(r#"
            extern fn fetch(url: String) -> String, err {
                err net = "net"
                mock { fetch -> "ok" }
            }
            pub fn load(url: String) -> String {
                const data = wait fetch(url)
                return data
                crash { fetch -> halt }
                test { self("x") == "ok" }
            }
        "#));
        assert!(dts.contains("Promise<string>"), "async function should return Promise, got: {}", dts);
    }
}
