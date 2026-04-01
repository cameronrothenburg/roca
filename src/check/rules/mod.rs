//! All checker rules — each module enforces a category of diagnostics.

pub mod contracts;
pub mod constraints;
pub mod structs;
pub mod satisfies;
pub mod crash;
pub mod tests;
pub mod variables;
pub mod methods;
pub mod types;
pub mod unhandled;
pub mod manual_err;
pub mod docs;
pub mod ownership;
pub mod reserved;
