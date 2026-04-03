//! From<AST> conversions — build roca-types from AST nodes.

use roca_ast::{self as ast, TypeRef};
use crate::*;

// ─── RocaType from TypeRef ────────────────────────────

impl From<&TypeRef> for RocaType {
    fn from(ty: &TypeRef) -> Self {
        match ty {
            TypeRef::Number => RocaType::Number,
            TypeRef::String => RocaType::String,
            TypeRef::Bool => RocaType::Bool,
            TypeRef::Ok => RocaType::Void,
            TypeRef::Named(name) => RocaType::Struct(name.clone()),
            TypeRef::Generic(name, args) => match name.as_str() {
                "Array" => RocaType::Array(Box::new(
                    args.first().map(RocaType::from).unwrap_or(RocaType::Unknown),
                )),
                "Map" => {
                    let k = args.first().map(RocaType::from).unwrap_or(RocaType::String);
                    let v = args.get(1).map(RocaType::from).unwrap_or(RocaType::Unknown);
                    RocaType::Map(Box::new(k), Box::new(v))
                }
                "Optional" => RocaType::Optional(Box::new(
                    args.first().map(RocaType::from).unwrap_or(RocaType::Unknown),
                )),
                _ => RocaType::Struct(name.clone()),
            },
            TypeRef::Nullable(inner) => RocaType::Optional(Box::new(RocaType::from(inner.as_ref()))),
            TypeRef::Fn(params, ret) => RocaType::Fn(
                params.iter().map(RocaType::from).collect(),
                Box::new(RocaType::from(ret.as_ref())),
            ),
        }
    }
}

// ─── Param ────────────────────────────────────────────

impl From<&ast::Param> for Param {
    fn from(p: &ast::Param) -> Self {
        Param {
            name: p.name.clone(),
            roca_type: RocaType::from(&p.type_ref),
            constraints: p.constraints.iter().map(Constraint::from).collect(),
        }
    }
}

// ─── Field ────────────────────────────────────────────

impl From<&ast::Field> for Field {
    fn from(f: &ast::Field) -> Self {
        Field {
            name: f.name.clone(),
            roca_type: RocaType::from(&f.type_ref),
            constraints: f.constraints.iter().map(Constraint::from).collect(),
        }
    }
}

// ─── Constraint ───────────────────────────────────────

impl From<&ast::Constraint> for Constraint {
    fn from(c: &ast::Constraint) -> Self {
        match c {
            ast::Constraint::Min(n) => Constraint::Min(*n),
            ast::Constraint::Max(n) => Constraint::Max(*n),
            ast::Constraint::MinLen(n) => Constraint::MinLen(*n),
            ast::Constraint::MaxLen(n) => Constraint::MaxLen(*n),
            ast::Constraint::Contains(s) => Constraint::Contains(s.clone()),
            ast::Constraint::Pattern(s) => Constraint::Pattern(s.clone()),
            ast::Constraint::Default(s) => Constraint::Default(s.clone()),
        }
    }
}

// ─── ErrDecl ──────────────────────────────────────────

impl From<&ast::ErrDecl> for ErrDecl {
    fn from(e: &ast::ErrDecl) -> Self {
        ErrDecl { name: e.name.clone(), message: e.message.clone() }
    }
}

// ─── FnSignature ──────────────────────────────────────

impl From<&ast::FnSignature> for FnSignature {
    fn from(s: &ast::FnSignature) -> Self {
        FnSignature {
            name: s.name.clone(),
            is_pub: s.is_pub,
            params: s.params.iter().map(Param::from).collect(),
            return_type: RocaType::from(&s.return_type),
            returns_err: s.returns_err,
            errors: s.errors.iter().map(ErrDecl::from).collect(),
        }
    }
}

// ─── CrashBlock ───────────────────────────────────────

impl From<&ast::CrashBlock> for CrashBlock {
    fn from(cb: &ast::CrashBlock) -> Self {
        CrashBlock {
            handlers: cb.handlers.iter().map(CrashHandler::from).collect(),
        }
    }
}

impl From<&ast::CrashHandler> for CrashHandler {
    fn from(h: &ast::CrashHandler) -> Self {
        CrashHandler {
            call: h.call.clone(),
            strategy: CrashHandlerKind::from(&h.strategy),
        }
    }
}

impl From<&ast::CrashHandlerKind> for CrashHandlerKind {
    fn from(k: &ast::CrashHandlerKind) -> Self {
        match k {
            ast::CrashHandlerKind::Simple(chain) => {
                CrashHandlerKind::Simple(chain.iter().map(CrashStep::from).collect())
            }
            ast::CrashHandlerKind::Detailed { arms, default } => {
                CrashHandlerKind::Detailed {
                    arms: arms.iter().map(CrashArm::from).collect(),
                    default: default.as_ref().map(|c| c.iter().map(CrashStep::from).collect()),
                }
            }
        }
    }
}

impl From<&ast::CrashArm> for CrashArm {
    fn from(a: &ast::CrashArm) -> Self {
        CrashArm {
            err_name: a.err_name.clone(),
            chain: a.chain.iter().map(CrashStep::from).collect(),
        }
    }
}

impl From<&ast::CrashStep> for CrashStep {
    fn from(s: &ast::CrashStep) -> Self {
        match s {
            ast::CrashStep::Log => CrashStep::Log,
            ast::CrashStep::Panic => CrashStep::Panic,
            ast::CrashStep::Halt => CrashStep::Halt,
            ast::CrashStep::Skip => CrashStep::Skip,
            ast::CrashStep::Retry { attempts, delay_ms } => CrashStep::Retry { attempts: *attempts, delay_ms: *delay_ms },
            ast::CrashStep::Fallback(expr) => CrashStep::Fallback(expr.clone()),
        }
    }
}

// ─── TestBlock ────────────────────────────────────────

impl From<&ast::TestBlock> for TestBlock {
    fn from(tb: &ast::TestBlock) -> Self {
        TestBlock {
            cases: tb.cases.iter().map(TestCase::from).collect(),
        }
    }
}

impl From<&ast::TestCase> for TestCase {
    fn from(tc: &ast::TestCase) -> Self {
        match tc {
            ast::TestCase::Equals { args, expected } => TestCase::Equals {
                args: args.clone(),
                expected: expected.clone(),
            },
            ast::TestCase::IsOk { args } => TestCase::IsOk { args: args.clone() },
            ast::TestCase::IsErr { args, err_name } => TestCase::IsErr {
                args: args.clone(),
                err_name: err_name.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_ref_to_roca_type() {
        assert_eq!(RocaType::from(&TypeRef::Number), RocaType::Number);
        assert_eq!(RocaType::from(&TypeRef::String), RocaType::String);
        assert_eq!(RocaType::from(&TypeRef::Ok), RocaType::Void);
        assert_eq!(RocaType::from(&TypeRef::Named("Email".into())), RocaType::Struct("Email".into()));
    }

    #[test]
    fn param_from_ast() {
        let ast_param = ast::Param {
            name: "age".into(),
            type_ref: TypeRef::Number,
            constraints: vec![ast::Constraint::Min(0.0), ast::Constraint::Max(150.0)],
        };
        let param = Param::from(&ast_param);
        assert_eq!(param.name, "age");
        assert_eq!(param.roca_type, RocaType::Number);
        assert_eq!(param.constraints.len(), 2);
    }

    #[test]
    fn field_from_ast() {
        let ast_field = ast::Field {
            name: "value".into(),
            type_ref: TypeRef::String,
            constraints: vec![],
        };
        let field = Field::from(&ast_field);
        assert_eq!(field.name, "value");
        assert_eq!(field.roca_type, RocaType::String);
    }
}
