use soroban_sdk::{panic_with_error, Env};

use crate::errors::FeeVaultError;

/// Require that an incoming amount is not negative
///
/// ### Arguments
/// * `amount` - The amount to check
/// * `err` - The error to panic with if the amount is non-positive
///
/// ### Panics
/// If the number is negative or zero
pub fn require_positive(e: &Env, amount: i128, err: FeeVaultError) {
    if amount <= 0 {
        panic_with_error!(e, err);
    }
}
