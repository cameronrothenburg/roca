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
    Struct(StructDef),
    Satisfies(SatisfiesDef),
    Function(FnDef),
}

/// import { Name1, Name2 } from "./path"
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDef {
    pub names: Vec<String>,
    pub path: String,
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
}

// ─── Contract ───────────────────────────────────────────

/// contract Name { signatures, errors, mock }
#[derive(Debug, Clone, PartialEq)]
pub struct ContractDef {
    pub name: String,
    pub is_pub: bool,
    pub functions: Vec<FnSignature>,
    pub fields: Vec<Field>,
    pub mock: Option<MockDef>,
    /// For enum-style contracts like StatusCode { 200, 201, ... }
    pub values: Vec<ContractValue>,
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
    pub methods: Vec<FnDef>,
}

// ─── Function ───────────────────────────────────────────

/// A function definition with body, crash block, and test block
#[derive(Debug, Clone, PartialEq)]
pub struct FnDef {
    pub name: String,
    pub is_pub: bool,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub returns_err: bool,
    pub errors: Vec<ErrDecl>,
    pub body: Vec<Stmt>,
    pub crash: Option<CrashBlock>,
    pub test: Option<TestBlock>,
}
