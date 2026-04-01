//! Module resolution and contract registry for the Roca compiler.
//! Handles cross-file imports and type registry building.

pub mod registry;
pub mod resolve;

pub use resolve::*;
pub use registry::ContractRegistry;
