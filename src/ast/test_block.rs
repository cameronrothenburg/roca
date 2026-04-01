//! Test block AST nodes — inline test cases for functions (equality, Ok, error assertions).

use super::expr::Expr;

/// Inline test block inside a function
/// ```roca
/// test {
///     self(1, 2) == 3
///     self("bad") is err.invalid
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TestBlock {
    pub cases: Vec<TestCase>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestCase {
    /// self(args) == expected
    Equals {
        args: Vec<Expr>,
        expected: Expr,
    },
    /// self(args) is Ok
    IsOk {
        args: Vec<Expr>,
    },
    /// self(args) is err.name
    IsErr {
        args: Vec<Expr>,
        err_name: String,
    },
}
