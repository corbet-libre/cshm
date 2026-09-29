//! Community profile schemas, validation and per-field version changes.
//!
//! Schemas are data: deserialize with serde, call [`Schema::compile`], then reuse
//! the compiled validator. Validation never stores profiles or signs anything.
//! See the README and `docs/CONTRACT.md` for the wire format and change rules.
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod changes;
mod schema;
mod validation;

pub use changes::*;
pub use rust_decimal::Decimal;
pub use schema::*;
pub use validation::*;
