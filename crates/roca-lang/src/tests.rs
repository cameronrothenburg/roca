//! AST node construction and equality tests.

use crate::ast::*;

#[test]
fn expr_untyped_defaults_to_unit() {
    let e = Expr::untyped(ExprKind::Lit(Lit::Int(42)));
    assert_eq!(e.ty, Type::Unit);
    assert_eq!(e.kind, ExprKind::Lit(Lit::Int(42)));
}

#[test]
fn expr_typed_carries_type() {
    let e = Expr::typed(ExprKind::Lit(Lit::Float(3.14)), Type::Float);
    assert_eq!(e.ty, Type::Float);
}

#[test]
fn type_named_equality() {
    assert_eq!(Type::Named("User".into()), Type::Named("User".into()));
    assert_ne!(Type::Named("User".into()), Type::Named("Point".into()));
}

#[test]
fn own_variants() {
    assert_ne!(Own::O, Own::B);
    let p = Param { own: Some(Own::O), name: "x".into(), ty: Type::Int };
    assert_eq!(p.own, Some(Own::O));
}

#[test]
fn source_file_with_multiple_items() {
    let f1 = FuncDef { name: "a".into(), is_pub: true, params: vec![], ret: Type::Int, body: vec![], test: None, doc: None };
    let f2 = FuncDef { name: "b".into(), is_pub: false, params: vec![], ret: Type::String, body: vec![], test: None, doc: None };
    let sf = SourceFile { items: vec![Item::Function(f1), Item::Function(f2)] };
    assert_eq!(sf.items.len(), 2);
}

#[test]
fn struct_def_with_fields_and_methods() {
    let s = StructDef {
        name: "Point".into(),
        is_pub: true,
        fields: vec![
            Field { name: "x".into(), ty: Type::Int },
            Field { name: "y".into(), ty: Type::Int },
        ],
        methods: vec![],
        doc: Some("A point".into()),
    };
    assert_eq!(s.fields.len(), 2);
    assert_eq!(s.doc, Some("A point".into()));
}

#[test]
fn enum_def_with_variants() {
    let e = EnumDef {
        name: "Color".into(),
        is_pub: true,
        variants: vec![
            Variant::Unit("Red".into()),
            Variant::Unit("Green".into()),
            Variant::Data("Custom".into(), vec![Type::Int, Type::Int, Type::Int]),
        ],
        doc: None,
    };
    assert_eq!(e.variants.len(), 3);
}

#[test]
fn match_arm_with_wildcard() {
    let arm = MatchArm {
        pattern: Pattern::Wildcard,
        body: Expr::untyped(ExprKind::Lit(Lit::Int(0))),
    };
    assert_eq!(arm.pattern, Pattern::Wildcard);
}

#[test]
fn test_block_with_cases() {
    let tb = TestBlock {
        cases: vec![
            TestCase::Equals {
                args: vec![Expr::untyped(ExprKind::Lit(Lit::Int(1)))],
                expected: Expr::untyped(ExprKind::Lit(Lit::Int(2))),
            },
        ],
    };
    assert_eq!(tb.cases.len(), 1);
}
