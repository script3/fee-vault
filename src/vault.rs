use crate::{
    dependencies::pool::{Client as PoolClient, Request},
    errors::FeeVaultError,
    reserve::Reserve,
    storage::{self},
};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    panic_with_error, vec, Address, Env, IntoVal, Symbol, Vec,
};

pub fn deposit(e: &Env, from: &Address, amount: i128, reserve_id: u32) -> i128 {
    let mut reserve = Reserve::load(&e, reserve_id);
    let pool_address = storage::get_pool(&e);
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
        .unwrap_or_else(|| panic_with_error!(&e, FeeVaultError::ReserveNotFound));
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
    let b_tokens_amount = new_positions.supply.get_unchecked(reserve_id) - pre_supply;
    reserve.update_rate(e, amount, b_tokens_amount);
    reserve.deposit(from.clone(), amount, b_tokens_amount);
    reserve.store(e);
    b_tokens_amount
}

pub fn withdraw(e: &Env, from: &Address, amount: i128, reserve_id: u32) -> i128 {
    let mut reserve = Reserve::load(&e, reserve_id);
    let pool_address = storage::get_pool(&e);
    let pool = PoolClient::new(&e, &pool_address);
    // Get deposit amount pre-supply
    let pre_supply = pool
        .get_positions(&e.current_contract_address())
        .supply
        .get(reserve_id)
        .unwrap_or_else(|| panic_with_error!(&e, FeeVaultError::ReserveNotFound));
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
    let b_tokens_amount = pre_supply - new_positions.supply.get_unchecked(reserve_id);
    reserve.update_rate(e, amount, b_tokens_amount);
    reserve.withdraw(e, from.clone(), amount, b_tokens_amount);
    reserve.store(e);
    b_tokens_amount
}

pub fn claim(e: &Env, admin: &Address, reserve_ids: Vec<u32>) -> i128 {
    let pool_address = storage::get_pool(&e);
    let pool = PoolClient::new(&e, &pool_address);
    pool.claim(&e.current_contract_address(), &reserve_ids, &admin)
}
