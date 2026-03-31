//! Top-level AST node definitions.
//! Source files, items (contracts, structs, functions, enums), and their components.

use super::crash::CrashBlock;
use super::err::ErrDecl;
use super::mock::MockDef;
use super::stmt::Stmt;
use super::test_block::TestBlock;
use super::types::TypeRef;

/// A complete Roca source file
#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub items: Vec<Item>,
}

/// Top-level item in a source file
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Import(ImportDef),
    Contract(ContractDef),
    Enum(EnumDef),
    Struct(StructDef),
    Satisfies(SatisfiesDef),
    Function(FnDef),
    /// extern contract — JS runtime type (no emit)
    ExternContract(ContractDef),
    /// extern fn — JS runtime function (no emit, bare call at sites)
    ExternFn(ExternFnDef),
}

/// extern fn name(params) -> ReturnType, err { err declarations, mock block }
#[derive(Debug, Clone, PartialEq)]
pub struct ExternFnDef {
    pub name: String,
    pub doc: Option<String>,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub returns_err: bool,
    pub errors: Vec<ErrDecl>,
    pub mock: Option<MockDef>,
}

/// enum Name { key = value, ... } OR enum Name { Variant(Type) | Variant | ... }
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub is_pub: bool,
    pub doc: Option<String>,
    pub variants: Vec<EnumVariant>,
    /// True if this is an algebraic enum (data variants), false for flat key=value
    pub is_algebraic: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub value: EnumValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnumValue {
    String(String),
    Number(f64),
    /// Data variant with typed fields: Variant(Type1, Type2)
    Data(Vec<TypeRef>),
    /// Unit variant (no data): Variant
    Unit,
}

/// import { Name1, Name2 } from "./path" or from std::module
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDef {
    pub names: Vec<String>,
    pub source: ImportSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportSource {
    /// import from "./file.roca"
    Path(String),
    /// import from std or std::module
    Std(Option<String>),
}

/// Function parameter
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub type_ref: TypeRef,
}

/// Function signature (used in contracts and struct contract blocks)
#[derive(Debug, Clone, PartialEq)]
pub struct FnSignature {
    pub name: String,
    pub is_pub: bool,
    pub doc: Option<String>,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    /// Whether this function can return errors
    pub returns_err: bool,
    /// Named errors this function can produce
    pub errors: Vec<ErrDecl>,
}

/// Field in a struct or contract
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub type_ref: TypeRef,
    pub constraints: Vec<Constraint>,
}

/// Constraint on a field: { min: 0, max: 255, contains: "@" }
#[derive(Debug, Clone, PartialEq)]
pub enum Constraint {
    Min(f64),
    Max(f64),
    MinLen(f64),
    MaxLen(f64),
    Contains(String),
    Pattern(String),
    Default(String),
}

// ─── Contract ───────────────────────────────────────────

/// contract Name<T, V: Constraint> { signatures, errors, mock }
#[derive(Debug, Clone, PartialEq)]
pub struct ContractDef {
    pub name: String,
    pub is_pub: bool,
    pub doc: Option<String>,
    pub type_params: Vec<TypeParam>,
    pub functions: Vec<FnSignature>,
    pub fields: Vec<Field>,
    pub mock: Option<MockDef>,
    pub values: Vec<ContractValue>,
}

/// Type parameter: T or T: ContractName (constrained)
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: String,
    /// Optional constraint — must satisfy this contract
    pub constraint: Option<String>,
}

/// Fixed value in an enum-style contract
#[derive(Debug, Clone, PartialEq)]
pub enum ContractValue {
    Number(f64),
    String(String),
}

// ─── Struct ─────────────────────────────────────────────

/// struct Name { contract_block }{ impl_block }
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub name: String,
    pub is_pub: bool,
    pub doc: Option<String>,
    /// Contract block (first {}): fields + fn signatures
    pub fields: Vec<Field>,
    pub signatures: Vec<FnSignature>,
    /// Implementation block (second {}): fn bodies
    pub methods: Vec<FnDef>,
}

// ─── Satisfies ──────────────────────────────────────────

/// Name satisfies Contract { implementations }
#[derive(Debug, Clone, PartialEq)]
pub struct SatisfiesDef {
    pub struct_name: String,
    pub contract_name: String,
    pub type_args: Vec<TypeRef>,
    pub methods: Vec<FnDef>,
}

// ─── Function ───────────────────────────────────────────

/// A function definition with body, crash block, and test block
#[derive(Debug, Clone, PartialEq)]
pub struct FnDef {
    pub name: String,
    pub is_pub: bool,
    pub doc: Option<String>,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub returns_err: bool,
    pub errors: Vec<ErrDecl>,
    pub body: Vec<Stmt>,
    pub crash: Option<CrashBlock>,
    pub test: Option<TestBlock>,
}
