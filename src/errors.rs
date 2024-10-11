use soroban_sdk::contracterror;

/// The error codes for the contract.
#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum FeeVaultError {
    // Default errors to align with built-in contract
    InternalError = 1,
    AlreadyInitializedError = 3,

    UnauthorizedError = 4,

    NegativeAmountError = 8,
    AllowanceError = 9,
    BalanceError = 10,
    OverflowError = 12,

    ReserveNotFound = 100,
    ReserveAlreadyExists = 101,
    InvalidAmount = 102,
    InsufficientAccruedFees = 103,
    InvalidTakeRate = 104,
    InsufficientReserves = 105,
    InvalidBTokensMinted = 106,
    InvalidBTokensBurnt = 107,
}
