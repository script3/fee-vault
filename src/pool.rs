use crate::storage;
use blend_contract_sdk::pool::{Client as PoolClient, Request};
use soroban_sdk::{token::TokenClient, vec, Address, Env, Vec};

/// Executes a supply of a specific reserve into the underlying pool on behalf of the fee vault
///
/// ### Arguments
/// * `vault` - The reserve vault
/// * `from` - The address of the user
/// * `amount` - The amount of tokens to deposit
///
/// ### Returns
/// * `i128` - The amount of bTokens received from the supply
pub fn supply(e: &Env, reserve: &Address, from: &Address, amount: i128) -> i128 {
    let pool = get_pool_client(&e);

    // Execute the deposit - the tokens are transferred from the user to the pool
    pool.submit(
        &e.current_contract_address(),
        &from,
        &from,
        &vec![
            &e,
            Request {
                address: reserve.clone(),
                amount,
                request_type: 0,
            },
        ],
    );
    // NOTE: Mock this for now to avoid breaking everything
    let b_tokens_amount = 0;
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
pub fn withdraw(e: &Env, reserve: &Address, to: &Address, amount: i128) -> (i128, i128) {
    let pool = get_pool_client(&e);

    // Get balance pre-withdraw, as the pool can modify the withdrawal amount
    let pre_withdrawal_balance = TokenClient::new(&e, &reserve).balance(&to);

    // Execute the withdrawal - the tokens are transferred from the pool to the user
    pool.submit(
        &e.current_contract_address(),
        &e.current_contract_address(),
        &to,
        &vec![
            &e,
            Request {
                address: reserve.clone(),
                amount,
                request_type: 1,
            },
        ],
    );

    // Calculate the amount of tokens withdrawn and bTokens burnt
    let post_withdrawal_balance = TokenClient::new(&e, &reserve).balance(&to);
    let real_amount = post_withdrawal_balance - pre_withdrawal_balance;
    // NOTE: Mock this for now to avoid breaking everything
    let b_tokens_amount = 0;
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
    let pool = get_pool_client(e);
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
    get_pool_client(&e).get_reserve(reserve).b_rate
}

#[inline]
fn get_pool_client(e: &Env) -> PoolClient {
    PoolClient::new(&e, &storage::get_pool(&e))
}
