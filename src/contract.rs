use crate::{
    dependencies::pool::Positions, errors::FeeVaultError, reserve::Reserve, storage,
    types::ReserveData, user::User, vault,
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
    /// * `Map<Address, i128>` - A map of underlying addresses and underlying deposit amounts in shares
    pub fn get_deposits(e: Env, ids: Vec<u32>, user: Address) -> Map<Address, i128> {
        let mut result = Map::new(&e);
        let user = User::load(&e, user);
        for id in ids.iter() {
            let reserve = Reserve::load(&e, id);
            result.set(reserve.address.clone(), user.deposits.get(id).unwrap_or(0));
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

    /// Get the shares_to_b_token_rate for a set of reserves
    ///
    /// ### Arguments
    /// * `id` - The id of the reserve
    /// * `share_amount` - The amount of shares to convert to b_tokens
    ///
    /// ### Returns
    /// * `i128` - The number of b_tokens the shares are worth
    pub fn shares_to_b_token(e: Env, id: u32, share_amount: i128) -> i128 {
        Reserve::load(&e, id).shares_to_b_tokens_down(share_amount)
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

    // REVIEW: I think this would be better taken in as shares, to avoid any issues
    // with dust. We could expose a "share_to_b_token_rate" function for wallets to consume.
    //
    // As it sits currently, these is also some concern that dust shares could brick the vault.
    // Token vaults as it stands have an expectation that if 0 tokens are in the vault, there
    // are also 0 shares. Since we calculate the share value from the token removal, this might not
    // be the case.
    //
    // Please see comment on get_deposits_in_underlying

    /// Withdraws tokens from the fee vault for a specific reserve
    ///
    /// ### Arguments
    /// * `from` - The address of the user making the withdrawal
    /// * `amount` - The amount of tokens to withdraw
    /// * `reserve_id` - The ID of the reserve to withdraw from
    ///
    /// ### Returns
    /// * `i128` - The amount of b_tokens withdrawn
    pub fn withdraw(e: &Env, from: Address, amount: i128, reserve_id: u32) -> i128 {
        from.require_auth();
        vault::withdraw(e, &from, amount, reserve_id)
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
    /// * `claims` - The ids of the reserves we're claiming fees for and the amount of underlying to withdraw from the blend pool
    ///
    /// ### Note
    /// The amount of underlying to withdraw must be less than or equal to the amount of underlying that the accrued bTokens from fees are worth.
    ///
    /// ### Returns
    /// * `Positions` - The new vault positions
    pub fn claim_fees(e: &Env, claims: Vec<(u32, i128)>) -> Positions {
        let admin = storage::get_admin(&e);
        admin.require_auth();
        vault::claim_fee(e, &admin, claims)
    }
}
