use crate::{
    dependencies::pool::{Client as PoolClient, Positions, Request},
    errors::FeeVaultError,
    reserve::Reserve,
    storage,
    user::User,
};
use soroban_sdk::{panic_with_error, token::TokenClient, vec, Address, Env, Vec};

/// Executes a deposit of a specific reserve into the underlying pool on behalf of the fee vault
pub fn deposit(e: &Env, from: &Address, amount: i128, reserve_id: u32) -> i128 {
    let mut reserve = Reserve::load(&e, reserve_id);
    let pool_address = storage::get_pool(&e);

    let pool = PoolClient::new(&e, &pool_address);
    // Get deposit amount pre-supply
    let pre_supply = pool
        .get_positions(&e.current_contract_address())
        .supply
        .get(reserve_id)
        .unwrap_or(0);
    // Execute the deposit - the tokens are transferred from the user to the pool
    let new_positions = pool.submit(
        &e.current_contract_address(),
        &from,
        &from,
        &vec![
            &e,
            Request {
                address: reserve.address.clone(),
                amount,
                request_type: 0,
            },
        ],
    );
    // Calculate the amount of bTokens received
    let b_tokens_amount = new_positions.supply.get_unchecked(reserve_id) - pre_supply;
    // Update the reserve's bRate and deposit the tokens
    reserve.update_rate(e, amount, b_tokens_amount);
    let share_amount = reserve.deposit(b_tokens_amount);
    let mut user = User::load(e, from.clone());
    user.deposit(reserve_id, share_amount);
    user.store(e);
    reserve.store(e);
    b_tokens_amount
}

/// Executes a user withdrawal of a specific reserve from the underlying pool on behalf of the fee vault
pub fn withdraw(e: &Env, from: &Address, amount: i128, reserve_id: u32) -> i128 {
    // REVIEW: if we intake shares here, we should validate they don't over withdraw before we even invoke
    // the pool.
    let mut reserve = Reserve::load(&e, reserve_id);
    let pool_address = storage::get_pool(&e);
    let pool = PoolClient::new(&e, &pool_address);
    // Get deposit amount pre-supply
    let pre_supply = pool
        .get_positions(&e.current_contract_address())
        .supply
        .get(reserve_id)
        .unwrap_or_else(|| panic_with_error!(e, FeeVaultError::ReserveNotFound));

    // Execute the withdrawal - the tokens are transferred from the pool to the user
    let pre_withdrawal_balance = TokenClient::new(&e, &reserve.address).balance(&from);
    let new_positions = pool.submit(
        &e.current_contract_address(),
        &from,
        &from,
        &vec![
            &e,
            Request {
                address: reserve.address.clone(),
                amount,
                request_type: 1,
            },
        ],
    );
    let post_withdrawal_balance = TokenClient::new(&e, &reserve.address).balance(&from);
    let real_amount = post_withdrawal_balance - pre_withdrawal_balance;
    // Calculate the amount of bTokens withdrawn
    let b_tokens_amount = pre_supply - new_positions.supply.get_unchecked(reserve_id);
    // Update the reserve's bRate and withdraw the tokens
    reserve.update_rate(e, real_amount, b_tokens_amount);
    let share_amount = reserve.withdraw(e, b_tokens_amount);
    let mut user = User::load(e, from.clone());
    user.withdraw(e, reserve_id, share_amount);
    user.store(e);
    reserve.store(e);
    b_tokens_amount
}

/// Executes a claim of BLND emissions from the pool on behalf of the fee vault
pub fn claim(e: &Env, admin: &Address, reserve_ids: Vec<u32>) -> i128 {
    let pool_address = storage::get_pool(&e);
    let pool = PoolClient::new(&e, &pool_address);
    // Claim the emissions - they are transferred to the admin address
    pool.claim(&e.current_contract_address(), &reserve_ids, &admin)
}

pub fn claim_fee(e: &Env, admin: &Address, claims: Vec<(u32, i128)>) -> Positions {
    let pool_client = PoolClient::new(&e, &storage::get_pool(&e));
    let mut requests: Vec<Request> = Vec::new(&e);
    for (reserve_id, amount) in claims.clone() {
        let reserve = Reserve::load(&e, reserve_id);
        requests.push_front(Request {
            address: reserve.address.clone(),
            amount,
            request_type: 1,
        });
    }
    let pre_positions = pool_client.get_positions(&e.current_contract_address());
    let new_positions =
        pool_client.submit(&e.current_contract_address(), &admin, &admin, &requests);
    for (reserve_id, _) in claims {
        let supply_change = pre_positions.supply.get_unchecked(reserve_id)
            - new_positions.supply.get_unchecked(reserve_id);
        let mut reserve = Reserve::load(&e, reserve_id);
        if reserve.accrued_fees < supply_change {
            panic_with_error!(e, FeeVaultError::InsufficientAccruedFees);
        }
        reserve.accrued_fees = reserve.accrued_fees - supply_change;
        reserve.store(e);
    }
    new_positions
}
