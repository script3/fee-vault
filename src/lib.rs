#![no_std]

#[cfg(any(test, feature = "testutils"))]
extern crate std;

pub mod constants;
pub mod contract;
pub mod dependencies;
pub mod errors;
pub mod reserve;
pub mod storage;
pub mod testutils;
pub mod types;
pub mod vault;

pub use contract::*;

#[cfg(test)]
mod tests;
