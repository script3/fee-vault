use crate::{
    errors::FeeVaultError,
    events::FeeVaultEvents,
    pool,
    reserve_vault::{self, ReserveVault},
    storage,
};

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Vec};

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
    /// * `InvalidTakeRate` - If the take rate is not within 0 and 1_000_0000
    pub fn __constructor(e: Env, admin: Address, pool: Address, take_rate: i128) {
        admin.require_auth();
        if take_rate < 0 || take_rate > 1_000_0000 {
            panic_with_error!(&e, FeeVaultError::InvalidTakeRate);
        }

        storage::set_admin(&e, admin);
        storage::set_pool(&e, pool);
        storage::set_take_rate(&e, take_rate);
    }

    //********** Read-Only ***********//

    /// Fetch a user's position in shares
    ///
    /// ### Arguments
    /// * `reserve` - The asset address of the reserve
    /// * `user` - The address of the user
    ///
    /// ### Returns
    /// * `i128` - The user's position in shares, or 0 if the reserve does not have a vault or the
    ///            user has no shares
    pub fn get_shares(e: Env, reserve: Address, user: Address) -> i128 {
        storage::get_reserve_vault_shares(&e, &reserve, &user)
    }

    /// Fetch a user's position in bTokens
    ///
    /// ### Arguments
    /// * `reserve` - The asset address of the reserve
    /// * `user` - The address of the user
    ///
    /// ### Returns
    /// * `i128` - The user's position in bTokens, or 0 if they have no bTokens
    pub fn get_b_tokens(e: Env, reserve: Address, user: Address) -> i128 {
        let shares = storage::get_reserve_vault_shares(&e, &reserve, &user);
        if shares > 0 {
            let vault = storage::get_reserve_vault(&e, &reserve);
            vault.shares_to_b_tokens_down(shares)
        } else {
            0
        }
    }

    /// Fetch a user's position in underlying tokens
    ///
    /// ### Arguments
    /// * `reserve` - The asset address of the reserve
    /// * `user` - The address of the user
    ///
    /// ### Returns
    /// * `i128` - The user's position in underlying tokens, or 0 if they have no bTokens
    pub fn get_underlying_tokens(e: Env, reserve: Address, user: Address) -> i128 {
        let shares = storage::get_reserve_vault_shares(&e, &reserve, &user);
        if shares > 0 {
            let mut vault = storage::get_reserve_vault(&e, &reserve);
            let new_b_rate = pool::reserve_b_rate(&e, &reserve);

            vault.update_rate(&e, new_b_rate);
            vault.shares_to_b_tokens_down(shares)
        } else {
            0
        }
    }

    /// Fetch the accrued fees in underlying tokens
    ///
    /// ### Arguments
    /// * `reserve` - The asset address of the reserve
    ///
    /// ### Returns
    /// * `i128` - The user's position in underlying tokens, or 0 if they have no bTokens
    ///
    /// ### Panics
    /// * `ReserveNotFound` - If the reserve does not exist
    pub fn get_collected_fees(e: Env, reserve: Address) -> i128 {
        let mut vault = storage::get_reserve_vault(&e, &reserve);
        let new_b_rate = pool::reserve_b_rate(&e, &reserve);
        vault.update_rate(&e, new_b_rate);

        vault.b_tokens_to_underlying_down(vault.accrued_fees)
    }

    /// Get the blend pool address
    ///
    /// ### Returns
    /// * `Address` - The blend pool address
    pub fn get_pool(e: Env) -> Address {
        storage::get_pool(&e)
    }

    /// Get the reserve vault data
    ///
    /// ### Arguments
    /// * `reserve` - The asset address of the reserve
    ///
    /// ### Returns
    /// * `ReserveData` - The reserve data
    ///
    /// ### Panics
    /// * `ReserveNotFound` - If the reserve does not exist
    pub fn get_reserve_vault(e: Env, reserve: Address) -> ReserveVault {
        storage::get_reserve_vault(&e, &reserve)
    }

    //********** Read-Write Admin Only ***********//

    /// ADMIN ONLY
    /// Sets the take rate for the fee vault
    ///
    /// ### Arguments
    /// * `e` - The environment object
    /// * `take_rate` - The new take rate to set
    ///
    /// ### Panics
    /// * `InvalidTakeRate` - If the take rate is not within 0 and 1_000_0000
    pub fn set_take_rate(e: Env, take_rate: i128) {
        storage::get_admin(&e).require_auth();
        if take_rate < 0 || take_rate > 1_000_0000 {
            panic_with_error!(&e, FeeVaultError::InvalidTakeRate);
        }
        storage::set_take_rate(&e, take_rate);
    }

    /// ADMIN ONLY
    /// Sets the admin address for the fee vault
    ///
    /// ### Arguments
    /// * `e` - The environment object
    /// * `admin` - The new admin address to set
    pub fn set_admin(e: Env, admin: Address) {
        storage::get_admin(&e).require_auth();
        admin.require_auth();
        storage::set_admin(&e, admin);
    }

    /// ADMIN ONLY
    /// Add a new reserve vault
    ///
    /// ### Arguments
    /// * `reserve_address` - The address of the reserve to add
    ///
    /// ### Panics
    /// * `ReserveAlreadyExists` - If the reserve already has a vault
    pub fn add_reserve_vault(e: Env, reserve_address: Address) {
        storage::get_admin(&e).require_auth();
        if storage::has_reserve_vault(&e, &reserve_address) {
            panic_with_error!(&e, FeeVaultError::ReserveAlreadyExists);
        } else {
            let reserve_id = pool::reserve_id(&e, &reserve_address);
            storage::set_reserve_vault(
                &e,
                &reserve_address,
                &ReserveVault {
                    address: reserve_address.clone(),
                    reserve_id,
                    b_rate: 1_000_000_000,
                    total_shares: 0,
                    total_b_tokens: 0,
                    accrued_fees: 0,
                },
            );
            FeeVaultEvents::new_reserve_vault(&e, reserve_id, &reserve_address);
        }
    }

    /// ADMIN ONLY
    /// Claims emissions for the given reserves from the pool. This is a passthrough function
    /// that invokes the pool's "claim" function as the contract. More details can be found
    /// here: https://github.com/blend-capital/blend-contracts/blob/v1.0.0/pool/src/contract.rs#L192
    ///
    /// ### Arguments
    /// * `reserve_token_ids` - The ids of the reserves to claiming emissions for
    /// * `to` - The address to send the emissions to
    ///
    /// ### Returns
    /// * `i128` - The amount of blnd tokens claimed
    pub fn claim_emissions(e: Env, reserve_token_ids: Vec<u32>, to: Address) -> i128 {
        storage::get_admin(&e).require_auth();
        pool::claim(&e, &reserve_token_ids, &to)
    }

    /// ADMIN ONLY
    /// Claims fees for the given reserves from the vault
    ///
    /// ### Arguments
    /// * `reserve` - The address of the reserve to claim fees for
    /// * `to` - The address to send the fees to
    /// * `amount` - The amount of fees to claim
    ///
    /// ### Returns
    /// * `b_tokens` - The number of b_tokens burnt
    ///
    /// ### Panics
    /// * `InsufficientAccruedFees` - If more b_tokens are withdrawn than accrued via fees
    /// * `ReserveNotFound` - If the reserve does not have a vault
    pub fn claim_fees(e: Env, reserve: Address, to: Address, amount: i128) -> i128 {
        let admin = storage::get_admin(&e);
        admin.require_auth();

        let vault = storage::get_reserve_vault(&e, &reserve);
        let new_b_rate = pool::reserve_b_rate(&e, &reserve);

        let (tokens_withdrawn, b_tokens_burnt) = pool::withdraw(&e, &vault, &to, amount);
        reserve_vault::claim_fees(&e, vault, b_tokens_burnt, new_b_rate);
        FeeVaultEvents::vault_fee_claim(&e, &reserve, &admin, tokens_withdrawn, b_tokens_burnt);
        b_tokens_burnt
    }

    //********** Read-Write ***********//

    /// Deposits tokens into the fee vault for a specific reserve
    ///
    /// ### Arguments
    /// * `reserve` - The address of the reserve to deposit
    /// * `user` - The address of the user making the deposit
    /// * `amount` - The amount of tokens to deposit
    ///
    /// ### Returns
    /// * `i128` - The number of shares minted for the user
    ///
    /// ### Panics
    /// * `InvalidAmount` - If the amount is less than the minimum dust amount (10000)
    /// * `ReserveNotFound` - If the reserve does not have a vault
    pub fn deposit(e: Env, reserve: Address, user: Address, amount: i128) -> i128 {
        user.require_auth();

        let vault = storage::get_reserve_vault(&e, &reserve);
        let new_b_rate = pool::reserve_b_rate(&e, &reserve);

        let b_tokens_minted = pool::supply(&e, &vault, &user, amount);
        let new_shares = reserve_vault::deposit(&e, vault, &user, b_tokens_minted, new_b_rate);
        FeeVaultEvents::vault_deposit(&e, &reserve, &user, amount, new_shares, b_tokens_minted);
        new_shares
    }

    /// Withdraws tokens from the fee vault for a specific reserve
    ///
    /// ### Arguments
    /// * `reserve` - The address of the reserve to withdraw
    /// * `user` - The address of the user making the withdrawal
    /// * `amount` - The amount of tokens to withdraw
    ///
    /// ### Returns
    /// * `i128` - The number of shares burnt
    ///
    /// ### Panics
    /// * `InvalidAmount` - If the amount is less than the minimum dust amount (10000)
    /// * `BalanceError` - If the user does not have enough shares to withdraw the amount
    /// * `ReserveNotFound` - If the reserve does not have a vault
    pub fn withdraw(e: Env, reserve: Address, user: Address, amount: i128) -> i128 {
        user.require_auth();

        let vault = storage::get_reserve_vault(&e, &reserve);
        let new_b_rate = pool::reserve_b_rate(&e, &reserve);
        let (tokens_withdrawn, b_tokens_burnt) = pool::withdraw(&e, &vault, &user, amount);
        let burnt_shares = reserve_vault::withdraw(&e, vault, &user, b_tokens_burnt, new_b_rate);
        FeeVaultEvents::vault_withdraw(
            &e,
            &reserve,
            &user,
            tokens_withdrawn,
            burnt_shares,
            b_tokens_burnt,
        );
        burnt_shares
    }
}
