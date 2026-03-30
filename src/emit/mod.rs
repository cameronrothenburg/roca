pub(crate) mod ast_helpers;
mod helpers;
mod expressions;
mod statements;
mod contracts;
pub(crate) mod functions;
pub(crate) mod structs;
mod crash;
pub(crate) mod dts;
pub mod test_harness;

use std::collections::HashMap;
use crate::ast::{self, Item, FnDef};

/// Load stdlib JS runtime snippet dynamically by module name.
/// Looks for stdlib/{name}.js alongside the roca binary.
fn stdlib_runtime(module: &str) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;

    for base in &[
        exe_dir.join("../packages/stdlib"),
        exe_dir.join("../../packages/stdlib"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/stdlib"),
    ] {
        let path = base.join(format!("{}.js", module));
        if let Ok(source) = std::fs::read_to_string(&path) {
            return Some(source);
        }
    }
    None
}
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
    // std:: imports are resolved by the compiler — not emitted as JS imports
    let mut import_lines = Vec::new();
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
                ast::ImportSource::Std(Some(module)) => {
                    if let Some(js) = stdlib_runtime(module) {
                        import_lines.push(js.to_string());
                    }
                }
                ast::ImportSource::Std(None) => {
                    // import from std — primitives, no JS needed
                }
            }
        }
    }

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
                // Extern declarations produce no JS — they exist at runtime
            }
        }
    }

    let program = ast.program(SPAN, SourceType::mjs(), source_text, ast.vec(), None, ast.vec(), body);
    let code = Codegen::new().build(&program).code;

    if import_lines.is_empty() {
        code
    } else {
        format!("{}\n{}", import_lines.join("\n"), code)
    }
}

/// Generate TypeScript declaration file (.d.ts) content
pub fn emit_dts(file: &ast::SourceFile) -> String {
    dts::emit_dts(file)
}

/// Emit enum as: const Name = { key: "value", ... };
fn build_enum<'a>(ast: &AstBuilder<'a>, e: &ast::EnumDef) -> Statement<'a> {
    use ast_helpers::{string_lit, number_lit, prop, object_expr, const_decl};

    let mut props = ast.vec();
    for v in &e.variants {
        let value = match &v.value {
            ast::EnumValue::String(s) => string_lit(ast, s),
            ast::EnumValue::Number(n) => number_lit(ast, *n),
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
}
