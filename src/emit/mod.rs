//! JavaScript code generation from the Roca AST.
//! Converts checked Roca source files into valid JS modules with error tuples,
//! contracts, structs, and optional test harnesses.

pub(crate) mod ast_helpers;
mod helpers;
pub(crate) mod shapes;
mod expressions;
mod statements;
mod contracts;
pub(crate) mod functions;
pub(crate) mod structs;
mod crash;
pub(crate) mod dts;

use std::collections::HashMap;
use crate::ast::{self, Item, FnDef};

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast::AstBuilder;
use oxc_codegen::Codegen;
use oxc_span::{SPAN, SourceType};

/// Emit a Roca source file as JavaScript using OXC AST builder.
pub fn emit(file: &ast::SourceFile) -> String {
    let allocator = Allocator::default();
    let ast = AstBuilder::new(&allocator);
    let source_text = allocator.alloc_str("");

    // Pre-pass: collect satisfies methods grouped by struct name
    let mut satisfies_map: HashMap<&str, Vec<&FnDef>> = HashMap::new();
    for item in &file.items {
        if let Item::Satisfies(sat) = item {
            let methods: &mut Vec<&FnDef> = satisfies_map.entry(&sat.struct_name).or_default();
            for m in &sat.methods {
                methods.push(m);
            }
        }
    }

    // Collect import lines (emitted as raw string prefix)
    // Collect imports and detect stdlib usage
    let mut import_lines = Vec::new();
    let mut uses_stdlib = false;
    let mut stdlib_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for item in &file.items {
        if let Item::Import(imp) = item {
            match &imp.source {
                ast::ImportSource::Path(path) => {
                    let js_path = path.replace(".roca", ".js");
                    import_lines.push(format!(
                        "import {{ {} }} from \"{}\";",
                        imp.names.join(", "),
                        js_path,
                    ));
                }
                ast::ImportSource::Std(Some(_)) => {
                    uses_stdlib = true;
                    for name in &imp.names {
                        stdlib_names.insert(name.clone());
                    }
                }
                ast::ImportSource::Std(None) => {}
            }
        }
    }

    // Also detect stdlib contracts used without import (built-in)
    for item in &file.items {
        if let Item::ExternContract(c) = item {
            stdlib_names.insert(c.name.clone());
        }
    }
    if !stdlib_names.is_empty() { uses_stdlib = true; }

    // Emit single runtime import if any stdlib is used
    if uses_stdlib {
        import_lines.insert(0, "import roca from \"@rocalang/runtime\";".to_string());
    }

    // Register stdlib names for roca. prefixing in JS output
    shapes::set_stdlib_contracts(stdlib_names);

    let mut body = ast.vec();

    for item in &file.items {
        match item {
            Item::Import(_) => {
                // Handled above as raw string
            }
            Item::Enum(e) => {
                let stmt = build_enum(&ast, e);
                if e.is_pub {
                    body.push(wrap_export(&ast, stmt));
                } else {
                    body.push(stmt);
                }
            }
            Item::Contract(c) => {
                let stmts = contracts::build_contract_stmts(&ast, c);
                for stmt in stmts {
                    if c.is_pub {
                        body.push(wrap_export(&ast, stmt));
                    } else {
                        body.push(stmt);
                    }
                }
            }
            Item::Struct(s) => {
                let sat_methods = satisfies_map.get(s.name.as_str()).map(|v| v.as_slice()).unwrap_or(&[]);
                let class = structs::build_struct(&ast, s, sat_methods);
                let class_decl = Declaration::ClassDeclaration(ast.alloc(class));
                if s.is_pub {
                    let export = ast.export_named_declaration(
                        SPAN, Some(class_decl), ast.vec(), None, ImportOrExportKind::Value, oxc_ast::NONE,
                    );
                    body.push(Statement::from(ModuleDeclaration::ExportNamedDeclaration(ast.alloc(export))));
                } else {
                    body.push(Statement::from(class_decl));
                }
            }
            Item::Function(f) => {
                let func = functions::build_function(&ast, f);
                let func_decl = Declaration::FunctionDeclaration(ast.alloc(func));
                if f.is_pub {
                    let export = ast.export_named_declaration(
                        SPAN, Some(func_decl), ast.vec(), None, ImportOrExportKind::Value, oxc_ast::NONE,
                    );
                    body.push(Statement::from(ModuleDeclaration::ExportNamedDeclaration(ast.alloc(export))));
                } else {
                    body.push(Statement::from(func_decl));
                }
            }
            Item::Satisfies(_) => {
                // Handled in pre-pass — methods merged into struct class
            }
            Item::ExternContract(_) | Item::ExternFn(_) => {
                // Types only — runtime provides implementations
            }
        }
    }

    let program = ast.program(SPAN, SourceType::mjs(), source_text, ast.vec(), None, ast.vec(), body);
    let code = Codegen::new().build(&program).code;

    let mut parts = Vec::new();
    if !import_lines.is_empty() { parts.push(import_lines.join("\n")); }
    if !code.is_empty() { parts.push(code); }
    parts.join("\n")
}

/// Generate TypeScript declaration file (.d.ts) content
pub fn emit_dts(file: &ast::SourceFile) -> String {
    dts::emit_dts(file)
}

/// Emit enum as: const Name = { key: "value", ... };
fn build_enum<'a>(ast: &AstBuilder<'a>, e: &ast::EnumDef) -> Statement<'a> {
    use ast_helpers::{string_lit, number_lit, prop, object_expr, const_decl, function_expr, formal_params, param, function_body, return_stmt, ident, TAG_FIELD, positional_field};

    let mut props = ast.vec();
    for v in &e.variants {
        let value = match &v.value {
            ast::EnumValue::String(s) => string_lit(ast, s),
            ast::EnumValue::Number(n) => number_lit(ast, *n),
            ast::EnumValue::Unit => {
                let mut obj_props = ast.vec();
                obj_props.push(prop(ast, TAG_FIELD, string_lit(ast, &v.name)));
                object_expr(ast, obj_props)
            }
            ast::EnumValue::Data(types) => {
                let mut fn_params = ast.vec();
                let mut obj_props = ast.vec();
                obj_props.push(prop(ast, TAG_FIELD, string_lit(ast, &v.name)));
                for (i, _) in types.iter().enumerate() {
                    let pname = positional_field(i);
                    fn_params.push(param(ast, &pname));
                    obj_props.push(prop(ast, &pname, ident(ast, &pname)));
                }
                let obj = object_expr(ast, obj_props);
                let mut stmts = ast.vec();
                stmts.push(return_stmt(ast, obj));
                let body = function_body(ast, stmts);
                let params = formal_params(ast, fn_params);
                function_expr(ast, params, body, false)
            }
        };
        props.push(prop(ast, &v.name, value));
    }
    let obj = object_expr(ast, props);
    const_decl(ast, &e.name, obj)
}

fn wrap_export<'a>(ast: &AstBuilder<'a>, stmt: Statement<'a>) -> Statement<'a> {
    if let Statement::VariableDeclaration(decl) = stmt {
        let decl = Declaration::VariableDeclaration(decl);
        let export = ast.export_named_declaration(
            SPAN, Some(decl), ast.vec(), None, ImportOrExportKind::Value, oxc_ast::NONE,
        );
        Statement::from(ModuleDeclaration::ExportNamedDeclaration(ast.alloc(export)))
    } else {
        stmt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn emit_simple_function() {
        let file = parse::parse(r#"
            pub fn greet(name: String) -> String {
                return "Hello " + name
                test { self("cam") == "Hello cam" }
            }
        "#);
        let js = emit(&file);
        assert!(js.contains("function greet"));
        assert!(js.contains("export"));
        assert!(js.contains("Hello "));
    }

    #[test]
    fn emit_struct() {
        let file = parse::parse(r#"
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
                        self("a@b.com") is Ok
                        self("") is err.missing
                    }
                }
            }
        "#);
        let js = emit(&file);
        assert!(js.contains("class Email"));
        assert!(js.contains("export"));
    }

    #[test]
    fn emit_contract_errors() {
        let file = parse::parse(r#"
            contract HttpClient {
                get(url: String) -> Response, err {
                    err timeout = "request timed out"
                    err not_found = "404 not found"
                }
            }
        "#);
        let js = emit(&file);
        assert!(js.contains("HttpClientErrors"));
        assert!(js.contains("timeout"));
        assert!(js.contains("request timed out"));
    }

    #[test]
    fn emit_enum_contract() {
        let file = parse::parse(r#"
            contract StatusCode { 200 201 400 }
        "#);
        let js = emit(&file);
        assert!(js.contains("StatusCode"));
        assert!(js.contains("200"));
    }

    #[test]
    fn emit_algebraic_enum() {
        let file = parse::parse(r#"
            pub enum Token {
                Number(Number)
                Str(String)
                Plus
            }
        "#);
        let js = emit(&file);
        assert!(js.contains("_tag"), "should contain _tag: {}", js);
        assert!(js.contains("\"Number\""), "should contain Number tag: {}", js);
        assert!(js.contains("\"Plus\""), "should contain Plus tag: {}", js);
        assert!(js.contains("function"), "data variants should be functions: {}", js);
    }

    #[test]
    fn emit_algebraic_enum_usage() {
        let file = parse::parse(r#"
            pub enum Color { Red Green Blue }
            pub fn is_red(c: String) -> String {
                return match c {
                    "Red" => "yes"
                    _ => "no"
                }
            }
        "#);
        let js = emit(&file);
        assert!(js.contains("Red"), "should contain Red: {}", js);
    }
}
