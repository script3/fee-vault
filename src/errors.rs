use soroban_sdk::contracterror;

/// The error codes for the contract.
#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum FeeVaultError {
    // Default errors to align with built-in contract
    BalanceError = 10,

    ReserveNotFound = 100,
    ReserveAlreadyExists = 101,
    InvalidAmount = 102,
    InsufficientAccruedFees = 103,
    InvalidTakeRate = 104,
    InsufficientReserves = 105,
    InvalidBTokensMinted = 106,
    InvalidBTokensBurnt = 107,
}
