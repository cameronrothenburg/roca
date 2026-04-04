//! Parser tests — verify the tokenizer + parser produce correct AST.
//! These bypass the checker (use raw parser).

use crate::parser;
fn parse(s: &str) -> roca_lang::SourceFile { parser::parse(s) }
fn parse_project(files: &[(&str, &str)]) -> Vec<roca_lang::SourceFile> {
    files.iter().map(|(_, s)| parser::parse(s)).collect()
}
use roca_lang::*;
use roca_lang::ast::ExprKind;

#[test]
fn hello_world() {
    let ast = parse(r#"
        pub fn greet() -> String {
            return "hello"
        }
    "#);
    assert_eq!(ast.items.len(), 1);
    let Item::Function(f) = &ast.items[0] else { panic!("expected function") };
    assert_eq!(f.name, "greet");
    assert!(f.is_pub);
    assert_eq!(f.ret, Type::String);
    assert_eq!(f.params.len(), 0);
    assert_eq!(f.body.len(), 1);
    let Stmt::Return(ref e) = f.body[0] else { panic!("expected return") };
    let ExprKind::Lit(Lit::String(ref s)) = e.kind else { panic!("expected string lit") };
    assert_eq!(s, "hello");
}

#[test]
fn arithmetic_precedence() {
    let ast = parse(r#"
        fn math() -> Int {
            const x = 1 + 2 * 3
            return x
        }
    "#);
    assert_eq!(ast.items.len(), 1);
    let Item::Function(f) = &ast.items[0] else { panic!("expected function") };
    let Stmt::Let { name, value, .. } = &f.body[0] else { panic!("expected let") };
    assert_eq!(name, "x");
    let ExprKind::BinOp { op, left, right } = &value.kind else { panic!("expected binop") };
    assert_eq!(*op, BinOp::Add);
    let ExprKind::Lit(Lit::Int(1)) = &left.as_ref().kind else { panic!("expected 1") };
    let ExprKind::BinOp { op: inner_op, .. } = &right.as_ref().kind else { panic!("expected mul") };
    assert_eq!(*inner_op, BinOp::Mul);
}

#[test]
fn ownership_params() {
    let ast = parse(r#"
        fn process(b config: Config, o data: Data) -> Unit {
            return Unit
        }
    "#);
    let Item::Function(f) = &ast.items[0] else { panic!("expected function") };
    assert_eq!(f.params.len(), 2);
    assert_eq!(f.params[0].own, Some(Own::B));
    assert_eq!(f.params[0].name, "config");
    assert_eq!(f.params[0].ty, Type::Named("Config".into()));
    assert_eq!(f.params[1].own, Some(Own::O));
    assert_eq!(f.params[1].name, "data");
    assert_eq!(f.params[1].ty, Type::Named("Data".into()));
}

#[test]
fn struct_with_constructor() {
    let ast = parse(r#"
        pub struct User {
            name: String
            age: Int
        }{
            pub fn new(o name: String, b age: Int) -> User {
                return User { name: name, age: age }
            }
        }
    "#);
    assert_eq!(ast.items.len(), 1);
    let Item::Struct(s) = &ast.items[0] else { panic!("expected struct") };
    assert_eq!(s.name, "User");
    assert!(s.is_pub);
    assert_eq!(s.fields.len(), 2);
    assert_eq!(s.fields[0].name, "name");
    assert_eq!(s.fields[0].ty, Type::String);
    assert_eq!(s.fields[1].name, "age");
    assert_eq!(s.fields[1].ty, Type::Int);
    assert_eq!(s.methods.len(), 1);
    assert_eq!(s.methods[0].name, "new");
    let Stmt::Return(ref e) = s.methods[0].body[0] else { panic!("expected return") };
    let ExprKind::StructLit { ref name, ref fields } = e.kind else { panic!("expected struct lit") };
    assert_eq!(name, "User");
    assert_eq!(fields.len(), 2);
}

#[test]
fn error_handling_inline() {
    let ast = parse(r#"
        fn load(b path: String) -> String {
            let result, err = read_file(path)
            if err {
                return ""
            }
            return result
        }
    "#);
    let Item::Function(f) = &ast.items[0] else { panic!("expected function") };
    let Stmt::Let { name, .. } = &f.body[0] else { panic!("expected let") };
    assert_eq!(name, "result");
    let Stmt::If { cond, then, .. } = &f.body[1] else { panic!("expected if") };
    let ExprKind::Ident(ref err_name) = cond.kind else { panic!("expected ident") };
    assert_eq!(err_name, "err");
    assert!(then.len() >= 1);
}

#[test]
fn match_enum() {
    let ast = parse(r#"
        fn handle(b result: Result) -> String {
            const msg = match result {
                Result.Ok(val) => val
                Result.Err(e) => "error"
                _ => "unknown"
            }
            return msg
        }
    "#);
    let Item::Function(f) = &ast.items[0] else { panic!("expected function") };
    let Stmt::Let { value, .. } = &f.body[0] else { panic!("expected let") };
    let ExprKind::Match { value: _, ref arms } = value.kind else { panic!("expected match") };
    assert_eq!(arms.len(), 3);
    let Pattern::Variant { name, variant, bindings } = &arms[0].pattern else { panic!("expected variant") };
    assert_eq!(name, "Result");
    assert_eq!(variant, "Ok");
    assert_eq!(bindings, &["val"]);
    assert_eq!(arms[2].pattern, Pattern::Wildcard);
}

#[test]
fn closure_to_function() {
    let ast = parse(r#"
        fn double_all(b items: Array) -> Array {
            const result = items.map(fn(x) -> x * 2)
            return result
        }
    "#);
    let Item::Function(f) = &ast.items[0] else { panic!("expected function") };
    let Stmt::Let { value, .. } = &f.body[0] else { panic!("expected let") };
    let ExprKind::Call { target, args } = &value.kind else { panic!("expected call") };
    let ExprKind::GetField { ref field, .. } = target.as_ref().kind else { panic!("expected field access") };
    assert_eq!(field, "map");
    assert_eq!(args.len(), 1);
    let ExprKind::MakeClosure { ref params, ref body } = args[0].kind else { panic!("expected closure") };
    assert_eq!(params, &["x"]);
    let ExprKind::BinOp { op, .. } = &body.as_ref().kind else { panic!("expected mul") };
    assert_eq!(*op, BinOp::Mul);
}

#[test]
fn for_loop() {
    let ast = parse(r#"
        fn sum(b items: Array) -> Int {
            var total = 0
            for item in items {
                total = total + item
            }
            return total
        }
    "#);
    let Item::Function(f) = &ast.items[0] else { panic!("expected function") };
    let Stmt::For { name, iter, body } = &f.body[1] else { panic!("expected for") };
    assert_eq!(name, "item");
    let ExprKind::Ident(ref iter_name) = iter.kind else { panic!("expected ident") };
    assert_eq!(iter_name, "items");
    assert!(body.len() >= 1);
}

#[test]
fn struct_field_access_and_mutation() {
    let ast = parse(r#"
        pub struct Counter {
            count: Int
        }{
            pub fn increment() -> Int {
                self.count = self.count + 1
                return self.count
            }
        }
    "#);
    let Item::Struct(s) = &ast.items[0] else { panic!("expected struct") };
    let m = &s.methods[0];
    let Stmt::SetField { target, field, value } = &m.body[0] else { panic!("expected set field") };
    assert_eq!(target.kind, ExprKind::SelfRef);
    assert_eq!(field, "count");
    let ExprKind::BinOp { op, left, .. } = &value.kind else { panic!("expected binop") };
    assert_eq!(*op, BinOp::Add);
    let ExprKind::GetField { target: inner, field: f2 } = &left.as_ref().kind else { panic!("expected get field") };
    assert_eq!(inner.as_ref().kind, ExprKind::SelfRef);
    assert_eq!(f2, "count");
}

#[test]
fn async_wait() {
    let ast = parse(r#"
        fn fetch_data(b url: String) -> String {
            const data = wait fetch(url)
            return data
        }
    "#);
    let Item::Function(f) = &ast.items[0] else { panic!("expected function") };
    let Stmt::Let { value, .. } = &f.body[0] else { panic!("expected let") };
    let ExprKind::Wait(ref inner) = value.kind else { panic!("expected wait") };
    let ExprKind::Call { target, args } = &inner.as_ref().kind else { panic!("expected call") };
    let ExprKind::Ident(ref name) = target.as_ref().kind else { panic!("expected ident") };
    assert_eq!(name, "fetch");
    assert_eq!(args.len(), 1);
}

#[test]
fn test_block() {
    let ast = parse(r#"
        pub fn add(b a: Int, b b: Int) -> Int {
            return a + b
        test {
            self(1, 2) == 3
            self(0, 0) == 0
        }}
    "#);
    let Item::Function(f) = &ast.items[0] else { panic!("expected function") };
    let test = f.test.as_ref().expect("expected test block");
    assert_eq!(test.cases.len(), 2);
    let TestCase::Equals { args, expected } = &test.cases[0];
    assert_eq!(args.len(), 2);
    assert_eq!(expected.kind, ExprKind::Lit(Lit::Int(3)));
}

#[test]
fn import_statement() {
    let ast = parse(r#"
        import { User, Config } from "./types.roca"
    "#);
    assert_eq!(ast.items.len(), 1);
    let Item::Import { names, path } = &ast.items[0] else { panic!("expected import") };
    assert_eq!(names, &["User", "Config"]);
    assert_eq!(path, "./types.roca");
}

#[test]
fn cross_file_project() {
    let dep = r#"
        pub fn helper(b n: Int) -> Int {
            return n * 10
        }
    "#;
    let main = r#"
        import { helper } from "./dep.roca"

        pub fn use_helper(b n: Int) -> Int {
            const result = helper(n)
            return result
        }
    "#;
    let files = parse_project(&[("dep.roca", dep), ("main.roca", main)]);
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].items.len(), 1);
    let Item::Function(f) = &files[0].items[0] else { panic!("expected function") };
    assert_eq!(f.name, "helper");
    assert_eq!(files[1].items.len(), 2);
    let Item::Import { names, .. } = &files[1].items[0] else { panic!("expected import") };
    assert_eq!(names, &["helper"]);
}
