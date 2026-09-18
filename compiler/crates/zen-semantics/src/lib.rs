//! Whole-program declaration collection followed by lexical, bidirectional checking.
mod checker;
pub mod types;
pub use checker::{CheckedProgram, ModuleInput, Symbol, TypedExpr, check};

pub mod analysis;
