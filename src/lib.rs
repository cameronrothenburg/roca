//! Roca compiler library — a contractual language that compiles to TypeScript.
//! Provides parsing, type checking, and code emission for `.roca` source files.

pub mod ast;
pub mod constants;
pub mod parse;
pub mod check;
pub mod emit;
pub mod errors;
pub mod resolve;
