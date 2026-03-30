//! Mock block AST nodes — test doubles for contract methods.

use super::expr::Expr;

/// Mock block on a contract
/// ```roca
/// mock {
///     get -> Response { status: StatusCode.200, body: Body.validate("{}") }
///     save -> Ok
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct MockDef {
    pub entries: Vec<MockEntry>,
}

/// Single mock entry: method_name -> value
#[derive(Debug, Clone, PartialEq)]
pub struct MockEntry {
    pub method: String,
    pub value: Expr,
}
