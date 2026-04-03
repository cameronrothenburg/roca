//! TDD Red — 13 tests defining the parser's contract.
//! Each parses real Roca syntax and asserts the AST structure.
//! All fail until the parser is implemented.

use crate::{parse, parse_project};
use roca_lang::*;

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
    let Stmt::Return(Expr::Lit(Lit::String(s))) = &f.body[0] else { panic!("expected return string") };
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
    // First stmt: const x = 1 + (2 * 3)
    let Stmt::Let { name, value, .. } = &f.body[0] else { panic!("expected let") };
    assert_eq!(name, "x");
    // Outer: Add, inner right: Mul
    let Expr::BinOp { op, left, right } = value else { panic!("expected binop") };
    assert_eq!(*op, BinOp::Add);
    let Expr::Lit(Lit::Int(1)) = left.as_ref() else { panic!("expected 1") };
    let Expr::BinOp { op: inner_op, .. } = right.as_ref() else { panic!("expected mul") };
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
    // Method body returns a struct literal
    let Stmt::Return(Expr::StructLit { name, fields }) = &s.methods[0].body[0] else { panic!("expected return struct lit") };
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
    // First stmt: let destructure
    let Stmt::Let { name, value, .. } = &f.body[0] else { panic!("expected let") };
    assert_eq!(name, "result");
    // Second stmt: if err
    let Stmt::If { cond, then, else_ } = &f.body[1] else { panic!("expected if") };
    let Expr::Ident(err_name) = cond else { panic!("expected ident") };
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
    let Expr::Match { value: _, arms } = value else { panic!("expected match") };
    assert_eq!(arms.len(), 3);
    // First arm: Result.Ok(val)
    let Pattern::Variant { name, variant, bindings } = &arms[0].pattern else { panic!("expected variant") };
    assert_eq!(name, "Result");
    assert_eq!(variant, "Ok");
    assert_eq!(bindings, &["val"]);
    // Last arm: wildcard
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
    // items.map(closure) is a Call where target is GetField
    let Expr::Call { target, args } = value else { panic!("expected call") };
    let Expr::GetField { field, .. } = target.as_ref() else { panic!("expected field access") };
    assert_eq!(field, "map");
    assert_eq!(args.len(), 1);
    let Expr::MakeClosure { params, body } = &args[0] else { panic!("expected closure") };
    assert_eq!(params, &["x"]);
    let Expr::BinOp { op, .. } = body.as_ref() else { panic!("expected mul") };
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
    let Expr::Ident(iter_name) = iter else { panic!("expected ident") };
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
    // First stmt: self.count = self.count + 1
    let Stmt::SetField { target, field, value } = &m.body[0] else { panic!("expected set field") };
    assert_eq!(*target, Expr::SelfRef);
    assert_eq!(field, "count");
    // RHS: self.count + 1
    let Expr::BinOp { op, left, .. } = value else { panic!("expected binop") };
    assert_eq!(*op, BinOp::Add);
    let Expr::GetField { target: inner, field: f2 } = left.as_ref() else { panic!("expected get field") };
    assert_eq!(inner.as_ref(), &Expr::SelfRef);
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
    let Expr::Wait(inner) = value else { panic!("expected wait") };
    let Expr::Call { target, args } = inner.as_ref() else { panic!("expected call") };
    let Expr::Ident(name) = target.as_ref() else { panic!("expected ident") };
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
    assert_eq!(*expected, Expr::Lit(Lit::Int(3)));
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
    // Dep has one function
    assert_eq!(files[0].items.len(), 1);
    let Item::Function(f) = &files[0].items[0] else { panic!("expected function") };
    assert_eq!(f.name, "helper");
    // Main has import + function
    assert_eq!(files[1].items.len(), 2);
    let Item::Import { names, .. } = &files[1].items[0] else { panic!("expected import") };
    assert_eq!(names, &["helper"]);
}
