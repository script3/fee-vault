use soroban_sdk::{contracttype, Address, Map};

#[derive(Clone, Copy, PartialEq)]
#[repr(u32)]
#[contracttype]
pub enum BootstrapStatus {
    Active = 0,
    Closing = 1,
    Completed = 2,
    Cancelled = 3,
}

#[derive(Clone)]
#[contracttype]
pub struct TokenInfo {
    pub address: Address,
    pub weight: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct ReserveData {
    pub address: Address,
    /// The latest reserve bRate observed by the fee vault
    pub b_rate: i128,
    /// Total deposits associated with the reserve
    pub total_deposits: i128,
    /// Total bToken deposits associated with the reserve
    pub total_b_tokens: i128,
    /// The user deposits associated with the reserve
    pub deposits: Map<Address, i128>,
}
