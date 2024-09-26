use crate::{
    dependencies::pool::Positions, errors::FeeVaultError, reserve::Reserve, storage,
    types::ReserveData, vault,
};

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Map, Vec};

#[contract]
pub struct FeeVault;

#[contractimpl]
impl FeeVault {
    /// Initialize the contract
    ///
    /// ### Arguments
    /// * `admin` - The admin address
    /// * `pool` - The blend pool address
    /// * `take_rate` - The take rate for the fee vault, 7 decimal precision
    ///
    /// ### Panics
    /// * `AlreadyInitializedError` - If the contract has already been initialized
    /// * `InvalidTakeRate` - If the take rate is not within 0 and 1_000_0000
    pub fn initialize(e: Env, admin: Address, pool: Address, take_rate: i128) {
        admin.require_auth();
        if storage::get_is_init(&e) {
            panic_with_error!(&e, FeeVaultError::AlreadyInitializedError);
        }
        storage::set_is_init(&e);
        storage::set_admin(&e, admin);
        storage::set_pool(&e, pool);
        if take_rate < 0 || take_rate > 1_000_0000 {
            panic_with_error!(&e, FeeVaultError::InvalidTakeRate);
        }
        storage::set_take_rate(&e, take_rate);
    }

    //********** Read-Only ***********//

    /// Fetch a deposits for a user
    ///
    /// ### Arguments
    /// * `ids` - The ids of the reserves
    /// * `user` - The address of the user
    ///
    /// ### Returns
    /// * `Map<Address, i128>` - A map of underlying addresses and underlying deposit amounts
    pub fn get_deposits_in_underlying(e: Env, ids: Vec<u32>, user: Address) -> Map<Address, i128> {
        let mut result = Map::new(&e);
        for id in ids.iter() {
            let reserve = Reserve::load(&e, id);
            result.set(
                reserve.address.clone(),
                reserve.shares_to_underlying(reserve.deposits.get(user.clone()).unwrap_or(0)),
            );
        }
        result
    }

    /// Get the blend pool address
    ///
    /// ### Returns
    /// * `Address` - The blend pool address
    pub fn get_pool(e: Env) -> Address {
        storage::get_pool(&e)
    }

    /// Get the reserve data for a reserve
    ///
    /// ### Arguments
    /// * `id` - The id of the reserve
    ///
    /// ### Returns
    /// * `ReserveData` - The reserve data
    pub fn get_reserve_data(e: Env, id: u32) -> ReserveData {
        storage::get_reserve_data(&e, id).unwrap()
    }

    //********** Read-Write ***********//

    // ADMIN ONLY
    /// Sets the take rate for the fee vault
    ///
    /// ### Arguments
    /// * `e` - The environment object
    /// * `take_rate` - The new take rate to set
    ///
    /// ### Panics
    /// * `InvalidTakeRate` - If the take rate is not within 0 and 1_000_0000
    pub fn set_take_rate(e: Env, take_rate: i128) {
        let admin = storage::get_admin(&e);
        admin.require_auth();
        if take_rate < 0 || take_rate > 1_000_0000 {
            panic_with_error!(&e, FeeVaultError::InvalidTakeRate);
        }
        storage::set_take_rate(&e, take_rate);
    }

    // ADMIN ONLY
    /// Sets the admin address for the fee vault
    ///
    /// ### Arguments
    /// * `e` - The environment object
    /// * `admin` - The new admin address to set
    pub fn set_admin(e: Env, admin: Address) {
        admin.require_auth();
        storage::get_admin(&e).require_auth();
        storage::set_admin(&e, admin);
    }

    // ADMIN ONLY
    /// Adds a new reserve to the fee vault
    ///
    /// ### Arguments
    /// * `reserve_id` - The ID of the reserve to add,
    /// must be the same as the blend pool reserve id
    /// * `reserve_address` - The address of the reserve to add,
    /// must be the same as the blend pool reserve address
    ///
    /// ### Note
    /// DO NOT call this function without ensuring the reserve id and address
    /// correspond to the blend pool reserve id and address.
    /// Doing so will cause you to be unable to support the reserve of that id in the future.
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
                    accrued_fees: 0,
                },
            );
        }
    }

    /// Deposits tokens into the fee vault for a specific reserve
    ///
    /// ### Arguments
    /// * `from` - The address of the user making the deposit
    /// * `amount` - The amount of tokens to deposit
    /// * `reserve_id` - The ID of the reserve to deposit
    ///
    /// ### Returns
    /// * `i128` - The amount of b-tokens received in exchange for the deposited underlying tokens
    pub fn deposit(e: &Env, from: Address, amount: i128, reserve_id: u32) -> i128 {
        from.require_auth();
        vault::deposit(e, &from, amount, reserve_id)
    }

    /// Withdraws tokens from the fee vault for a specific reserve
    ///
    /// ### Arguments
    /// * `from` - The address of the user making the withdrawal
    /// * `id` - The ID of the reserve to withdraw from
    /// * `amount` - The amount of tokens to withdraw
    ///
    /// ### Returns
    /// * `i128` - The amount of b_tokens withdrawn
    pub fn withdraw(e: &Env, from: Address, id: u32, amount: i128) -> i128 {
        from.require_auth();
        vault::withdraw(e, &from, amount, id)
    }

    /// ADMIN ONLY
    /// Claims emissions for the given reserves from the pool
    ///
    /// ### Arguments
    /// * `ids` - The ids of the reserves to claiming emissions for
    ///
    /// ### Returns
    /// * `i128` - The amount of blnd tokens claimed
    pub fn claim_emissions(e: &Env, ids: Vec<u32>) -> i128 {
        let admin = storage::get_admin(&e);
        admin.require_auth();
        vault::claim(e, &admin, ids)
    }

    /// ADMIN ONLY
    /// Claims fees for the given reserves from the vault
    ///
    /// ### Arguments
    /// * `claims` - The ids of the reserves we're claiming fees for
    ///
    /// ### Returns
    /// * `Positions` - The new vault positions
    pub fn claim_fees(e: &Env, claims: Vec<(u32, i128)>) -> Positions {
        let admin = storage::get_admin(&e);
        admin.require_auth();
        vault::claim_fee(e, &admin, claims)
    }
}
