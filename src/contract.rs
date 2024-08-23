use crate::{errors::FeeVaultError, reserve::Reserve, storage, types::ReserveData, vault};

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Map, Vec};

#[contract]
pub struct FeeVault;

#[contractimpl]
impl FeeVault {
    /// Initialize the contract
    ///
    /// ### Arguments
    /// * `backstop` - The backstop address
    /// * `admin` - The admin address
    /// * `pool` - The pool address
    ///
    /// ### Panics
    /// * `AlreadyInitializedError` - If the contract has already been initialized
    pub fn initialize(e: Env, backstop: Address, admin: Address, pool: Address, take_rate: i128) {
        if storage::get_is_init(&e) {
            panic_with_error!(&e, FeeVaultError::AlreadyInitializedError);
        }
        storage::set_is_init(&e);
        storage::set_backstop(&e, backstop);
        storage::set_admin(&e, admin);
        storage::set_pool(&e, pool);
        storage::set_take_rate(&e, take_rate);
    }

    //********** Read-Only ***********//

    /// Fetch a deposits for a user
    ///
    /// ### Arguments
    /// * `ids` - The ids of the reserves
    /// * `user` - The address of the user
    pub fn get_deposits(e: Env, ids: Vec<u32>, user: Address) -> Map<Address, i128> {
        let mut result = Map::new(&e);
        for id in ids.iter() {
            let reserve = Reserve::load(&e, id);
            result.set(
                reserve.address,
                reserve.deposits.get(user.clone()).unwrap_or(0),
            );
        }
        result
    }

    //********** Read-Write ***********//

    // ADMIN ONLY
    pub fn set_take_rate(e: Env, take_rate: i128) {
        let admin = storage::get_admin(&e);
        admin.require_auth();
        storage::set_take_rate(&e, take_rate);
    }

    // ADMIN ONLY
    pub fn set_admin(e: Env, admin: Address) {
        storage::get_admin(&e).require_auth();
        storage::set_admin(&e, admin);
    }

    // ADMIN ONLY
    pub fn add_reserve(e: Env, reserve_id: u32, reserve_address: Address) {
        let admin = storage::get_admin(&e);
        admin.require_auth();
        if let Some(_) = storage::get_reserve_data(&e, reserve_id) {
            panic_with_error!(&e, FeeVaultError::ReserveAlreadyExists);
        } else {
            storage::set_reserve_data(
                &e,
                reserve_id,
                ReserveData {
                    address: reserve_address,
                    b_rate: 1_000_000_000,
                    total_deposits: 0,
                    total_b_tokens: 0,
                    deposits: Map::new(&e),
                },
            );
        }
    }

    /// Deposits tokens into the fee vault for a specific reserve
    ///
    /// This function allows users to deposit tokens into the fee vault for a particular reserve.
    /// It requires authorization from the depositor and calls the `vault::deposit` function to handle the deposit process.
    ///
    /// ### Arguments
    /// * `e` - The environment object
    /// * `from` - The address of the user making the deposit
    /// * `amount` - The amount of tokens to deposit
    /// * `reserve_id` - The ID of the reserve to deposit into
    ///
    /// ### Returns
    /// * `i128` - The amount of b-tokens received in exchange for the deposit

    pub fn deposit(e: &Env, from: Address, amount: i128, reserve_id: u32) -> i128 {
        from.require_auth();
        vault::deposit(e, &from, amount, reserve_id)
    }

    /// Withdraws tokens from the fee vault for a specific reserve
    ///
    /// This function allows users to withdraw tokens from the fee vault for a particular reserve.
    /// It requires authorization from the withdrawer and calls the `vault::withdraw` function to handle the withdrawal process.
    ///
    /// ### Arguments
    /// * `e` - The environment object
    /// * `from` - The address of the user making the withdrawal
    /// * `id` - The ID of the reserve to withdraw from
    /// * `amount` - The amount of tokens to withdraw
    ///
    /// ### Returns
    /// * `i128` - The amount of underlying tokens received in exchange for the withdrawn b-tokens
    pub fn withdraw(e: &Env, from: Address, id: u32, amount: i128) -> i128 {
        from.require_auth();
        vault::withdraw(e, &from, amount, id)
    }

    /// Claims emissions for the given reserves from the pool
    ///
    /// Returns the amount of blnd tokens claimed
    ///
    /// ### Arguments
    /// * `id` - The ids of the reserves we're claiming emissions for
    pub fn claim(e: &Env, ids: Vec<u32>) -> i128 {
        let admin = storage::get_admin(&e);
        admin.require_auth();
        vault::claim(e, &admin, ids)
    }
}
