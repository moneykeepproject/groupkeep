//! GroupKeep core — part of the MoneyKeep family.
//!
//! UI-free so it can be reused by the Slint desktop/mobile app today and a PWA (WASM) later.

pub mod calc;
pub mod model;
pub mod money;
#[cfg(feature = "pdf")]
pub mod pdf;
pub mod samples;

pub use calc::{compute, Report};
pub use model::*;
pub use money::Currency;

#[cfg(test)]
mod tests;
