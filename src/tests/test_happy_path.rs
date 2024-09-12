#![cfg(test)]

use std::println;

use crate::constants::SCALAR_7;
use crate::dependencies::pool::{Client as PoolClient, Request};
use crate::storage::ONE_DAY_LEDGERS;
use crate::testutils::{self, create_fee_vault, EnvTestUtils};
use crate::FeeVaultClient;
use blend_contract_sdk::testutils::BlendFixture;
use soroban_sdk::testutils::{Address as _, BytesN as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{vec, Address, BytesN, Env, Error, String};

#[test]
fn test_happy_path() {
    println!("test_deposit");
    let e = Env::default();
    e.budget().reset_unlimited();
    e.mock_all_auths();
    e.set_default_info();

    let bombadil = Address::generate(&e);
    let frodo = Address::generate(&e);

    let blnd = e.register_stellar_asset_contract(bombadil.clone());
    let usdc = e.register_stellar_asset_contract(bombadil.clone());
    let blnd_client = StellarAssetClient::new(&e, &blnd);
    let blnd_token = TokenClient::new(&e, &blnd);
    let usdc_client = StellarAssetClient::new(&e, &usdc);
    let usdc_token = TokenClient::new(&e, &usdc);

    let blend_fixture = BlendFixture::deploy(&e, &bombadil, &blnd, &usdc);

    let fee_vault = create_fee_vault(&e, &blend_fixture, bombadil, &usdc_client);
    println!("fee vault created");
    let fee_vault_client = FeeVaultClient::new(&e, &fee_vault);
    let pool_address = fee_vault_client.get_pool();
    let pool_client = PoolClient::new(&e, &pool_address);
    // mint frodo usdc
    usdc_client.mint(&frodo, &100_0000_0000000);
    println!("frodo usdc minted");
    // deposit usdc in fee vault
    let b_tokens_received =
        fee_vault_client
            .mock_all_auths()
            .deposit(&frodo, &100_0000_0000000, &0);
    let shares_received = fee_vault_client
        .get_deposits(&vec![&e, 0], &frodo)
        .get(usdc.clone())
        .unwrap();
    assert_eq!(shares_received, b_tokens_received);
    let vault_balance = pool_client
        .get_positions(&fee_vault)
        .supply
        .get_unchecked(0);
    assert_eq!(vault_balance, b_tokens_received);
    // withdraw some usdc from fee vault
    let pre_withdrawal_balance = usdc_token.balance(&frodo);
    let pre_vault_balance = pool_client
        .get_positions(&fee_vault)
        .supply
        .get_unchecked(0);
    let withdrawn_amount = fee_vault_client.withdraw(&frodo, &0, &50_0000_0000000);
    let post_withdrawal_balance = usdc_token.balance(&frodo);
    let post_vault_balance = pool_client
        .get_positions(&fee_vault)
        .supply
        .get_unchecked(0);
    assert_eq!(
        post_withdrawal_balance,
        pre_withdrawal_balance + withdrawn_amount
    );
    assert_eq!(post_vault_balance, pre_vault_balance / 2);
    let vault_balance = pool_client
        .get_positions(&fee_vault)
        .supply
        .get_unchecked(0);
    assert_eq!(vault_balance, b_tokens_received / 2);

    // fund merry
    let merry = Address::generate(&e);
    usdc_client.mint(&merry, &101_0000_0000000);

    // utilization rate
    // in total 250_000e7 was deposited and 100_000e7 was borrowed
    // utilization rate is 100_000e7 / 250_000e7 = 0.4
    // IR is roughly 0.4 * .5 = 0.2

    // let interest accrue
    e.jump(ONE_DAY_LEDGERS * 10);

    // check bRate
    let b_tokens = pool_client
        .submit(
            &merry,
            &merry,
            &merry,
            &vec![
                &e,
                Request {
                    address: usdc.clone(),
                    amount: 1_000_000_000,
                    request_type: 0,
                },
            ],
        )
        .supply
        .get_unchecked(0);
    println!("b_tokens: {}", b_tokens);
    let b_rate = 1_000_000_000 * 1_000_000_000 / b_tokens;
    println!("b_rate: {}", b_rate);
    pool_client.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &e,
            Request {
                address: usdc.clone(),
                amount: 1_000_000_000,
                request_type: 1,
            },
        ],
    );

    // merry deposits 100e7
    let pre_vault_b_tokens = pool_client.get_positions(&fee_vault).supply.get(0).unwrap();
    let pre_merry_balance = usdc_token.balance(&merry);
    let b_tokens_received = fee_vault_client.deposit(&merry, &100_0000_0000000, &0);
    let post_merry_balance = usdc_token.balance(&merry);
    assert_eq!(post_merry_balance, pre_merry_balance - 100_0000_0000000);
    let post_vault_b_tokens = pool_client.get_positions(&fee_vault).supply.get(0).unwrap();
    assert_eq!(pre_vault_b_tokens + b_tokens_received, post_vault_b_tokens);
    let deposit_amount = fee_vault_client
        .get_deposits(&vec![&e, 0], &merry)
        .get(usdc.clone())
        .unwrap();
    let withdraw_amount = fee_vault_client.withdraw(&merry, &0, &100_0000_0000000);
    assert_eq!(withdraw_amount, 100_0000_0000000);
    assert_eq!(usdc_token.balance(&merry), 100_0000_0000000 + 99999999999);
    assert_eq!(deposit_amount, 100_0000_0000000);

    // check frodo deposit
    let frodo_deposit_amount = fee_vault_client
        .get_deposits(&vec![&e, 0], &frodo)
        .get(usdc)
        .unwrap();
    let frodo_b_rate = 10612834 * 800_000_000 / 1_000_000_000 + 1_000_000_000;
    assert_eq!(
        frodo_deposit_amount,
        50_0000_0000000 * frodo_b_rate / 1_000_000_000
    )
}
