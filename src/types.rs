use soroban_sdk::{contracttype, Address, Map};

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
