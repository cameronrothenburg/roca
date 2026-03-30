//! Test harness codegen — generates JS test files from Roca `test` and `mock` blocks.
//! Emits case-based tests, fuzz tests, battle tests, and mock object wiring.

mod cases;
mod fuzz;
mod mocks;
mod battle;
pub(crate) mod values;

use crate::ast as roca;
use oxc_allocator::Allocator;
use oxc_ast::AstBuilder;
use oxc_codegen::Codegen;
use oxc_span::{SPAN, SourceType};

use cases::{CallKind, emit_test_cases};
use fuzz::emit_fuzz_tests;
use mocks::{emit_mock_object, generate_mock_patches};
use battle::generate_battle_tests;

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

    let mut exports = Vec::new();
    for item in &file.items {
        match item {
            roca::Item::Function(f) => exports.push(f.name.clone()),
            roca::Item::Struct(s) => exports.push(s.name.clone()),
            _ => {}
        }
    }

    let import_line = if exports.is_empty() {
        String::new()
    } else {
        format!("import {{ {} }} from \"{}\";\n", exports.join(", "), import_path)
    };

    let allocator = Allocator::default();
    let ast = AstBuilder::new(&allocator);
    let source_text = allocator.alloc_str("");

    let mut body = ast.vec();
    let mut test_count: usize = 0;

    // Outside async IIFE so battle tests + summary can access them
    let counter_decls = "let _passed = 0;\nlet _failed = 0;";

    // Parse imported files once — reused for mock discovery and JS inlining
    let mut imported_files: Vec<(String, roca::SourceFile)> = Vec::new();
    for item in &file.items {
        if let roca::Item::Import(imp) = item {
            if let roca::ImportSource::Path(path) = &imp.source {
                let roca_path = std::path::Path::new(path);
                for base in &[".", "src"] {
                    let full_path = std::path::Path::new(base).join(roca_path);
                    if let Ok(source) = std::fs::read_to_string(&full_path) {
                        if let Ok(parsed) = crate::parse::try_parse(&source) {
                            imported_files.push((path.clone(), parsed));
                        }
                        break;
                    }
                }
            }
        }
    }

    // Emit mocks from current file AND imported files
    let mut mock_files: Vec<&roca::SourceFile> = vec![file];
    for (_, f) in &imported_files { mock_files.push(f); }

    for mock_file in &mock_files {
        for item in &mock_file.items {
            match item {
                roca::Item::Contract(c) => {
                    if let Some(mock_def) = &c.mock {
                        emit_mock_object(&ast, &c.name, mock_def, false, &c.functions, &mut body);
                    }
                }
                roca::Item::ExternContract(c) => {
                    if let Some(mock_def) = &c.mock {
                        emit_mock_object(&ast, &c.name, mock_def, true, &c.functions, &mut body);
                    }
                }
                _ => {}
            }
        }
    }

    let mut has_async = false;

    for item in &file.items {
        match item {
            roca::Item::Function(f) => {
                if let Some(test) = &f.test {
                    let is_async = super::functions::body_has_wait(&f.body);
                    if is_async { has_async = true; }
                    test_count += emit_test_cases(&ast, CallKind::Function(&f.name), f.returns_err, is_async, test, &mut body);
                }
            }
            roca::Item::Struct(s) => {
                for method in &s.methods {
                    if let Some(test) = &method.test {
                        let is_async = super::functions::body_has_wait(&method.body);
                        if is_async { has_async = true; }
                        test_count += emit_test_cases(&ast, CallKind::Method(&s.name, &method.name), method.returns_err, is_async, test, &mut body);
                    }
                }
            }
            _ => {}
        }
    }

    for item in &file.items {
        if let roca::Item::Function(f) = item {
            if f.is_pub && !f.params.is_empty() {
                test_count += emit_fuzz_tests(&ast, &f.name, &f.params, f.returns_err, &mut body);
            }
        }
    }

    let battle_tests = generate_battle_tests(file);

    let program = ast.program(SPAN, SourceType::mjs(), source_text, ast.vec(), None, ast.vec(), body);
    let test_code = Codegen::new().build(&program).code;

    let mock_patches = generate_mock_patches(file, import_path == "__embed__");

    let summary_js = "console.log(_passed + \" passed, \" + _failed + \" failed\");\nif (_failed > 0) process.exit(1);";

    let test_section = if has_async {
        format!("{}\nawait (async () => {{\n{}\n}})();", counter_decls, test_code)
    } else {
        format!("{}\n{}", counter_decls, test_code)
    };

    let full = if import_path == "__embed__" {
        // Inline imported files' code too (for cross-file deps)
        let mut all_code = Vec::new();
        for (_, imported) in &imported_files {
            let js = super::emit(imported)
                .replace("export ", "")
                .lines()
                .filter(|l| !l.starts_with("import "))
                .collect::<Vec<_>>()
                .join("\n");
            all_code.push(js);
        }
        let main_js = super::emit(file)
            .replace("export ", "")
            .lines()
            .filter(|l| !l.starts_with("import "))
            .collect::<Vec<_>>()
            .join("\n");
        all_code.push(main_js);
        let main_js = all_code.join("\n");
        let mut parts = vec![main_js];
        if !mock_patches.is_empty() { parts.push(mock_patches); }
        parts.push(test_section);
        if !battle_tests.is_empty() { parts.push(battle_tests); }
        parts.push(summary_js.to_string());
        parts.join("\n")
    } else {
        let mut parts = vec![import_line];
        if !mock_patches.is_empty() { parts.push(mock_patches); }
        parts.push(test_section);
        if !battle_tests.is_empty() { parts.push(battle_tests); }
        parts.push(summary_js.to_string());
        parts.join("\n")
    };

    Some((full, test_count))
}

