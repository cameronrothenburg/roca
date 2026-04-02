//! CLI entry point and subcommand dispatch for the `roca` binary.
//!
//! This is the top-level crate — it depends on every other `roca-*` crate and
//! wires them together into user-facing commands.
//!
//! # Subcommand modules
//!
//! - [`build`] — full pipeline: parse, check, native proof tests, emit JS + `.d.ts`.
//! - [`check`] — parse + static analysis only (no emission).
//! - [`repl`] — interactive REPL with optional `--native` mode.
//! - [`config`] — reads `roca.toml` project configuration.
//! - [`gen_extern`] — converts `.d.ts` files into Roca extern contracts.
//! - [`init`] — scaffold a new Roca project.
//! - [`search`] — search stdlib types and methods.
//! - [`log`] — structured build/test output formatting.

pub mod config;
pub mod build;
pub mod check;
pub mod log;
pub mod search;
pub mod repl;
pub mod gen_extern;
pub mod init;
