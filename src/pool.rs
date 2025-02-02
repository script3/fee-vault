use crate::{errors::FeeVaultError, reserve_vault::ReserveVault, storage};
use blend_contract_sdk::pool::{Client as PoolClient, Request, Reserve as BlendReserve};
use soroban_sdk::{panic_with_error, token::TokenClient, vec, Address, Env, Vec};

/// Executes a supply of a specific reserve into the underlying pool on behalf of the fee vault
///
/// ### Arguments
/// * `vault` - The reserve vault
/// * `from` - The address of the user
/// * `amount` - The amount of tokens to deposit
///
/// ### Returns
/// * `i128` - The amount of bTokens received from the supply
pub fn supply(e: &Env, vault: &ReserveVault, from: &Address, amount: i128) -> i128 {
    let pool = get_pool_client(&e);

    // Get deposit amount pre-supply
    let pre_supply = pool
        .get_positions(&e.current_contract_address())
        .supply
        .get(vault.reserve_id)
        .unwrap_or(0);
    // Execute the deposit - the tokens are transferred from the user to the pool
    let new_positions = pool.submit(
        &e.current_contract_address(),
        &from,
        &from,
        &vec![
            &e,
            Request {
                address: vault.address.clone(),
                amount,
                request_type: 0,
            },
        ],
    );
    // Calculate the amount of bTokens received
    let b_tokens_amount = new_positions.supply.get_unchecked(vault.reserve_id) - pre_supply;
    b_tokens_amount
}

/// Executes a user withdrawal of a specific reserve from the underlying pool on behalf of the fee vault
///
/// ### Arguments
/// * `vault` - The reserve vault
/// * `to` - The destination of the withdrawal
/// * `amount` - The amount of tokens to withdraw
///
/// ### Returns
/// * `(i128, i128)` - (The amount of underyling tokens withdrawn, the amount of bTokens burnt)
pub fn withdraw(e: &Env, vault: &ReserveVault, to: &Address, amount: i128) -> (i128, i128) {
    let pool = get_pool_client(&e);

    // Get bTokens pre-withdraw
    let pre_supply = pool
        .get_positions(&e.current_contract_address())
        .supply
        .get(vault.reserve_id)
        .unwrap_or_else(|| panic_with_error!(e, FeeVaultError::ReserveNotFound));
    // Get balance pre-withdraw, as the pool can modify the withdrawal amount
    let pre_withdrawal_balance = TokenClient::new(&e, &vault.address).balance(&to);

    // Execute the withdrawal - the tokens are transferred from the pool to the user
    let new_positions = pool.submit(
        &e.current_contract_address(),
        &e.current_contract_address(),
        &to,
        &vec![
            &e,
            Request {
                address: vault.address.clone(),
                amount,
                request_type: 1,
            },
        ],
    );

    // Calculate the amount of tokens withdrawn and bTokens burnt
    let post_withdrawal_balance = TokenClient::new(&e, &vault.address).balance(&to);
    let real_amount = post_withdrawal_balance - pre_withdrawal_balance;
    // position entry is deleted if the position is cleared
    let b_tokens_amount = pre_supply - new_positions.supply.get(vault.reserve_id).unwrap_or(0);
    (real_amount, b_tokens_amount)
}

/// Executes a claim of BLND emissions from the pool on behalf of the fee vault
///
/// ### Arguments
/// * `reserve_token_ids` - The reserve token ids to claim emissions for
/// * `to` - The address to send the emissions to
///
/// ### Returns
/// * `i128` - The amount of emissions claimed
pub fn claim(e: &Env, reserve_token_ids: &Vec<u32>, to: &Address) -> i128 {
    let pool_address = storage::get_pool(&e);
    let pool = PoolClient::new(&e, &pool_address);
    // Claim the emissions - they are transferred to the admin address
    pool.claim(&e.current_contract_address(), reserve_token_ids, to)
}

/// Fetches the reserve's b_rate from the pool
///
/// ### Arguments
/// * `reserve` - The reserve to fetch the b_rate for
///
/// ### Returns
/// * `i128` - The b_rate of the reserve
pub fn reserve_b_rate(e: &Env, reserve: &Address) -> i128 {
    reserve_info(e, reserve).b_rate
}

/// Fetches the reserve's id from the pool
///
/// ### Arguments
/// * `reserve` - The reserve to fetch the id for
///
/// ### Returns
/// * `i128` - The id of the reserve
pub fn reserve_id(e: &Env, reserve: &Address) -> u32 {
    reserve_info(e, reserve).index
}

fn reserve_info(e: &Env, reserve: &Address) -> BlendReserve {
    get_pool_client(&e).get_reserve(reserve)
}

#[inline]
fn get_pool_client(e: &Env) -> PoolClient {
    PoolClient::new(&e, &storage::get_pool(&e))
}
