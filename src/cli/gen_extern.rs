//! Generate Roca extern contracts from TypeScript .d.ts declaration files.

use std::path::Path;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_ast::ast::*;

struct TsMethod {
    name: String,
    params: Vec<(String, String)>,
    return_type: String,
    is_async: bool,
    is_nullable: bool,
}

struct TsInterface {
    name: String,
    methods: Vec<TsMethod>,
}

pub fn generate(dts_path: &Path) -> Result<Vec<(String, String)>, String> {
    let source = std::fs::read_to_string(dts_path)
        .map_err(|e| format!("error reading {}: {}", dts_path.display(), e))?;

    let allocator = Allocator::default();
    let source_type = SourceType::d_ts();
    let parser = Parser::new(&allocator, &source, source_type);
    let result = parser.parse();

    if !result.errors.is_empty() {
        return Err(format!("parse errors in {}", dts_path.display()));
    }

    let interfaces = extract_interfaces(&result.program);

    let mut output = Vec::new();
    for iface in &interfaces {
        if iface.methods.is_empty() {
            continue;
        }
        let filename = to_snake_case(&iface.name);
        let roca = generate_contract(iface);
        output.push((filename, roca));
    }

    Ok(output)
}

fn extract_interfaces(program: &Program) -> Vec<TsInterface> {
    let mut interfaces = Vec::new();
    let mut seen_methods: std::collections::HashMap<String, std::collections::HashSet<String>> = std::collections::HashMap::new();

    for stmt in &program.body {
        if let Statement::TSInterfaceDeclaration(decl) = stmt {
            let iface_name = decl.id.name.to_string();
            let method_set = seen_methods.entry(iface_name.clone()).or_default();
            let mut methods = Vec::new();

            for sig in &decl.body.body {
                if let TSSignature::TSMethodSignature(method) = sig {
                    if let Some(method_name) = method.key.name() {
                        let name_str = method_name.to_string();
                        // Skip overloaded methods — take only the first signature
                        if method_set.contains(&name_str) {
                            continue;
                        }
                        method_set.insert(name_str.clone());

                        let params = extract_params(method);
                        let (return_type, is_async, is_nullable) = match &method.return_type {
                            Some(ann) => extract_return_type(&ann.type_annotation),
                            None => ("Ok".to_string(), false, false),
                        };

                        methods.push(TsMethod {
                            name: name_str,
                            params,
                            return_type,
                            is_async,
                            is_nullable,
                        });
                    }
                }
            }

            interfaces.push(TsInterface { name: iface_name, methods });
        }
    }

    interfaces
}

fn extract_params(method: &TSMethodSignature) -> Vec<(String, String)> {
    let params = &method.params;
    let mut result = Vec::new();

    for param in &params.items {
        let name = match &param.pattern {
            BindingPattern::BindingIdentifier(id) => id.name.to_string(),
            _ => continue,
        };

        if param.optional || name == "options" || name == "init" {
            continue;
        }

        let roca_type = match &param.type_annotation {
            Some(ann) => ts_type_to_roca(&ann.type_annotation),
            None => "String".to_string(),
        };

        if roca_type == "__skip__" {
            continue;
        }

        result.push((name, roca_type));
    }
    result
}

fn extract_return_type(ts_type: &TSType) -> (String, bool, bool) {
    match ts_type {
        TSType::TSTypeReference(ref_type) => {
            let name = type_ref_name(ref_type);
            if name == "Promise" {
                if let Some(args) = &ref_type.type_arguments {
                    if let Some(inner) = args.params.first() {
                        let (inner_type, _, inner_nullable) = extract_return_type(inner);
                        return (inner_type, true, inner_nullable);
                    }
                }
                return ("Ok".to_string(), true, false);
            }
            (map_named_type(&name), false, false)
        }
        TSType::TSUnionType(union) => {
            let non_null: Vec<&TSType> = union.types.iter()
                .filter(|t| !matches!(t, TSType::TSNullKeyword(_) | TSType::TSUndefinedKeyword(_)))
                .collect();
            let has_null = union.types.iter()
                .any(|t| matches!(t, TSType::TSNullKeyword(_) | TSType::TSUndefinedKeyword(_)));
            if non_null.len() == 1 {
                let (inner, is_async, _) = extract_return_type(non_null[0]);
                return (inner, is_async, has_null);
            }
            ("String".to_string(), false, has_null)
        }
        TSType::TSStringKeyword(_) => ("String".to_string(), false, false),
        TSType::TSNumberKeyword(_) => ("Number".to_string(), false, false),
        TSType::TSBooleanKeyword(_) => ("Bool".to_string(), false, false),
        TSType::TSVoidKeyword(_) => ("Ok".to_string(), false, false),
        _ => ("String".to_string(), false, false),
    }
}

fn ts_type_to_roca(ts_type: &TSType) -> String {
    match ts_type {
        TSType::TSStringKeyword(_) => "String".to_string(),
        TSType::TSNumberKeyword(_) => "Number".to_string(),
        TSType::TSBooleanKeyword(_) => "Bool".to_string(),
        TSType::TSVoidKeyword(_) => "Ok".to_string(),
        TSType::TSTypeReference(ref_type) => {
            let name = type_ref_name(ref_type);
            map_named_type(&name)
        }
        TSType::TSUnionType(union) => {
            let non_null: Vec<&TSType> = union.types.iter()
                .filter(|t| !matches!(t, TSType::TSNullKeyword(_) | TSType::TSUndefinedKeyword(_)))
                .collect();
            if non_null.len() == 1 {
                return ts_type_to_roca(non_null[0]);
            }
            "String".to_string()
        }
        _ => "__skip__".to_string(),
    }
}

fn type_ref_name(ref_type: &TSTypeReference) -> String {
    match &ref_type.type_name {
        TSTypeName::IdentifierReference(id) => id.name.to_string(),
        TSTypeName::QualifiedName(q) => q.right.name.to_string(),
        _ => "Unknown".to_string(),
    }
}

fn map_named_type(name: &str) -> String {
    match name {
        "string" => "String".to_string(),
        "number" => "Number".to_string(),
        "boolean" => "Bool".to_string(),
        "void" => "Ok".to_string(),
        "ArrayBuffer" | "Uint8Array" | "BufferSource" => "Bytes".to_string(),
        "Response" | "Request" | "object" | "Object" => "String".to_string(),
        "ReadableStream" | "WritableStream" => "Bytes".to_string(),
        // Generic type params that extend string/number
        "Key" | "T" | "U" | "V" => "String".to_string(),
        "ExpectedValue" => "String".to_string(),
        "Metadata" => "String".to_string(),
        other => other.to_string(),
    }
}

fn infer_error_name(method_name: &str) -> &str {
    match method_name {
        "get" | "find" | "lookup" | "first" => "not_found",
        "parse" | "decode" => "parse_failed",
        "connect" | "fetch" => "connection_failed",
        _ => "failed",
    }
}

fn mock_value(roca_type: &str, is_nullable: bool) -> String {
    if is_nullable { return "\"mock\"".to_string(); }
    match roca_type {
        "String" => "\"mock\"".to_string(),
        "Number" => "0".to_string(),
        "Bool" => "true".to_string(),
        "Ok" => "Ok".to_string(),
        "Bytes" => "Bytes".to_string(),
        _ => "\"mock\"".to_string(),
    }
}

fn generate_contract(iface: &TsInterface) -> String {
    let mut out = String::new();
    let snake = to_snake_case(&iface.name);
    out.push_str(&format!("/**\n * Generated from {} — edit as needed.\n * Import with: import {{ {} }} from \"./{}.roca\"\n */\n", iface.name, iface.name, snake));
    out.push_str(&format!("pub extern contract {} {{\n", iface.name));

    for method in &iface.methods {
        let params: Vec<String> = method.params.iter()
            .map(|(name, ty)| format!("{}: {}", name, ty))
            .collect();

        let return_type = if method.is_nullable {
            format!("Optional<{}>", method.return_type)
        } else {
            method.return_type.clone()
        };

        let needs_err = method.is_async || method.is_nullable;

        out.push_str(&format!("    /// {}\n", method.name));

        if needs_err {
            out.push_str(&format!("    {}({}) -> {}, err {{\n", method.name, params.join(", "), return_type));
            if method.is_async {
                out.push_str(&format!("        err {} = \"{} failed\"\n",
                    infer_error_name(&method.name), method.name));
            }
            if method.is_nullable && infer_error_name(&method.name) != "not_found" {
                out.push_str("        err not_found = \"not found\"\n");
            }
            out.push_str("    }\n");
        } else {
            out.push_str(&format!("    {}({}) -> {}\n", method.name, params.join(", "), return_type));
        }
    }

    out.push_str("    mock {\n");
    for method in &iface.methods {
        let val = mock_value(&method.return_type, method.is_nullable);
        out.push_str(&format!("        {} -> {}\n", method.name, val));
    }
    out.push_str("    }\n}\n");
    out
}

fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_lowercase().next().unwrap());
    }
    result
}
