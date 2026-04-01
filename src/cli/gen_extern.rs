//! Generate Roca extern contracts from TypeScript .d.ts declaration files.

use std::collections::{HashMap, HashSet};
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
    fields: Vec<(String, String)>, // (name, roca_type)
}

pub fn generate(dts_path: &Path) -> Result<String, String> {
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
    let known_types: HashSet<String> = interfaces.iter().map(|i| i.name.clone()).collect();

    let file_stem = dts_path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("generated")
        .trim_end_matches(".d");

    let mut out = format!("/**\n * Generated from {} — edit as needed.\n */\n\n",
        dts_path.file_name().and_then(|s| s.to_str()).unwrap_or("unknown"));

    let mut count = 0;
    for iface in &interfaces {
        if iface.methods.is_empty() && iface.fields.is_empty() {
            continue;
        }
        out.push_str(&generate_contract(iface, &known_types));
        out.push('\n');
        count += 1;
    }

    eprintln!("{} extern contract(s) from {}", count, file_stem);
    Ok(out)
}

fn extract_interfaces(program: &Program) -> Vec<TsInterface> {
    let mut interfaces = Vec::new();
    let mut seen_methods: HashMap<String, HashSet<String>> = HashMap::new();

    for stmt in &program.body {
        if let Statement::TSInterfaceDeclaration(decl) = stmt {
            let iface_name = decl.id.name.to_string();
            let method_set = seen_methods.entry(iface_name.clone()).or_default();
            let mut methods = Vec::new();
            let mut fields = Vec::new();

            for sig in &decl.body.body {
                match sig {
                    TSSignature::TSMethodSignature(method) => {
                        if let Some(method_name) = method.key.name() {
                            let name_str = method_name.to_string();
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
                    TSSignature::TSPropertySignature(prop) => {
                        if let Some(prop_name) = prop.key.name() {
                            if let Some(ann) = &prop.type_annotation {
                                let roca_type = ts_type_to_roca(&ann.type_annotation);
                                if roca_type != "__skip__" {
                                    fields.push((prop_name.to_string(), roca_type));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            interfaces.push(TsInterface { name: iface_name, methods, fields });
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
        TSType::TSArrayType(arr) => {
            let inner = ts_type_to_roca(&arr.element_type);
            if inner == "__skip__" { return "__skip__".to_string(); }
            format!("Array<{}>", inner)
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
        "boolean" | "bool" => "Bool".to_string(),
        "void" => "Ok".to_string(),
        "ArrayBuffer" | "Uint8Array" | "BufferSource" | "ArrayBufferView" => "Bytes".to_string(),
        "ReadableStream" | "WritableStream" => "Bytes".to_string(),
        "Response" | "Request" | "object" | "Object" | "any" | "unknown" => "String".to_string(),
        // Generic type params — resolve to base types
        "Key" | "T" | "U" | "V" | "ExpectedValue" | "Metadata" | "Body" => "String".to_string(),
        "Iterable" | "Iterator" => "Array<String>".to_string(),
        "Map" => "Map<String>".to_string(),
        other => other.to_string(),
    }
}

/// Resolve a type name: if it's a known interface, keep it; otherwise map to a primitive
fn resolve_type(name: &str, known: &HashSet<String>) -> String {
    let mapped = map_named_type(name);
    // If map_named_type returned it unchanged, check if it's a known interface
    if mapped == name {
        if known.contains(name) {
            return name.to_string();
        }
        // Unknown type — fall back to String
        return "String".to_string();
    }
    mapped
}

/// Resolve all type references in a method's return type and params
fn resolve_method_types(method: &TsMethod, known: &HashSet<String>) -> TsMethod {
    TsMethod {
        name: method.name.clone(),
        params: method.params.iter()
            .map(|(n, t)| (n.clone(), resolve_type(t, known)))
            .collect(),
        return_type: resolve_type(&method.return_type, known),
        is_async: method.is_async,
        is_nullable: method.is_nullable,
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

fn generate_contract(iface: &TsInterface, known: &HashSet<String>) -> String {
    let mut out = String::new();
    // Collect imports needed
    let mut imports: HashSet<String> = HashSet::new();
    for method in &iface.methods {
        let resolved = resolve_method_types(method, known);
        collect_type_refs(&resolved.return_type, known, &mut imports);
        for (_, t) in &resolved.params {
            collect_type_refs(t, known, &mut imports);
        }
    }
    for (_, t) in &iface.fields {
        let resolved = resolve_type(t, known);
        collect_type_refs(&resolved, known, &mut imports);
    }
    imports.remove(&iface.name); // don't self-import

    out.push_str(&format!("pub extern contract {} {{\n", iface.name));

    // Fields
    for (name, ty) in &iface.fields {
        let resolved = resolve_type(ty, known);
        out.push_str(&format!("    {}: {}\n", name, resolved));
    }

    // Methods
    for method in &iface.methods {
        let resolved = resolve_method_types(method, known);
        let params: Vec<String> = resolved.params.iter()
            .map(|(name, ty)| format!("{}: {}", name, ty))
            .collect();

        let return_type = if resolved.is_nullable {
            format!("Optional<{}>", resolved.return_type)
        } else {
            resolved.return_type.clone()
        };

        let needs_err = resolved.is_async || resolved.is_nullable;

        out.push_str(&format!("    /// {}\n", resolved.name));

        if needs_err {
            out.push_str(&format!("    {}({}) -> {}, err {{\n", resolved.name, params.join(", "), return_type));
            if resolved.is_async {
                out.push_str(&format!("        err {} = \"{} failed\"\n",
                    infer_error_name(&resolved.name), resolved.name));
            }
            if resolved.is_nullable && infer_error_name(&resolved.name) != "not_found" {
                out.push_str("        err not_found = \"not found\"\n");
            }
            out.push_str("    }\n");
        } else {
            out.push_str(&format!("    {}({}) -> {}\n", resolved.name, params.join(", "), return_type));
        }
    }

    out.push_str("}\n");
    out
}

/// Collect type names that reference known interfaces (for imports)
fn collect_type_refs(ty: &str, known: &HashSet<String>, imports: &mut HashSet<String>) {
    // Strip Optional< > wrapper
    let inner = ty.strip_prefix("Optional<").and_then(|s| s.strip_suffix('>')).unwrap_or(ty);
    // Strip Array< > wrapper
    let inner = inner.strip_prefix("Array<").and_then(|s| s.strip_suffix('>')).unwrap_or(inner);
    // Strip Map< > wrapper
    let inner = inner.strip_prefix("Map<").and_then(|s| s.strip_suffix('>')).unwrap_or(inner);

    if known.contains(inner) {
        imports.insert(inner.to_string());
    }
}

