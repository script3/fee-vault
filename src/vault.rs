use crate::{
    dependencies::pool::{Client as PoolClient, Request},
    errors::FeeVaultError,
    reserve::Reserve,
    storage::{self},
};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    panic_with_error,
    token::TokenClient,
    vec, Address, Env, IntoVal, Symbol, Vec,
};

/// Executes a deposit of a specific reserve into the underlying pool on behalf of the fee vault
pub fn deposit(e: &Env, from: &Address, amount: i128, reserve_id: u32) -> i128 {
    let mut reserve = Reserve::load(&e, reserve_id);
    let pool_address = storage::get_pool(&e);
    // Authorize the fee vault to transfer tokens on behalf of the user.
    e.authorize_as_current_contract(vec![
        &e,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: reserve.address.clone(),
                fn_name: Symbol::new(&e, "transfer"),
                args: vec![
                    &e,
                    from.into_val(e),
                    pool_address.into_val(e),
                    amount.into_val(e),
                ],
            },
            sub_invocations: Vec::new(&e),
        }),
    ]);
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
    reserve.deposit(from.clone(), b_tokens_amount);
    reserve.store(e);
    b_tokens_amount
}

/// Executes a user withdrawal of a specific reserve from the underlying pool on behalf of the fee vault
pub fn withdraw(e: &Env, from: &Address, amount: i128, reserve_id: u32) -> i128 {
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
    reserve.withdraw(e, from.clone(), b_tokens_amount);
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
