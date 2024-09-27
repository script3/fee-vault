#![cfg(test)]

use crate::constants::{SCALAR_7, SCALAR_9};
use crate::dependencies::pool::{Client as PoolClient, Request};
use crate::storage::ONE_DAY_LEDGERS;
use crate::testutils::{assert_approx_eq_abs, create_fee_vault, EnvTestUtils};
use crate::FeeVaultClient;
use blend_contract_sdk::testutils::BlendFixture;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{vec, Address, Env};

#[test]
fn test_happy_path() {
    let e = Env::default();
    e.budget().reset_unlimited();
    e.mock_all_auths();
    e.set_default_info();

    let bombadil = Address::generate(&e);
    let frodo = Address::generate(&e);

    let blnd = e.register_stellar_asset_contract(bombadil.clone());
    let usdc = e.register_stellar_asset_contract(bombadil.clone());
    let xlm = e.register_stellar_asset_contract(bombadil.clone());
    let usdc_client = StellarAssetClient::new(&e, &usdc);
    let usdc_token = TokenClient::new(&e, &usdc);
    let xlm_client = StellarAssetClient::new(&e, &xlm);
    let xlm_token = TokenClient::new(&e, &xlm);
    let blend_fixture = BlendFixture::deploy(&e, &bombadil, &blnd, &usdc);
    let blnd_token = TokenClient::new(&e, &blnd);
    let fee_vault = create_fee_vault(
        &e,
        &blend_fixture,
        bombadil.clone(),
        &usdc_client,
        &xlm_client,
    );
    let fee_vault_client = FeeVaultClient::new(&e, &fee_vault);
    let pool_address = fee_vault_client.get_pool();
    let pool_client = PoolClient::new(&e, &pool_address);
    // mint frodo usdc
    usdc_client.mint(&frodo, &100_0000_0000000);
    // mint frodo xlm
    xlm_client.mint(&frodo, &100_0000_0000000);
    // deposit usdc in fee vault
    let b_tokens_received =
        fee_vault_client
            .mock_all_auths()
            .deposit(&frodo, &1_000_000_000_0000, &0);
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

    let accrued_fees = fee_vault_client.get_reserve_data(&0).accrued_fees;
    assert_eq!(accrued_fees, 0);

    // fund merry
    let merry = Address::generate(&e);
    usdc_client.mint(&merry, &101_0000_0000000);
    xlm_client.mint(&merry, &100_0000_0000000);

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
    let b_rate = 1_000_000_000 * 1_000_000_000 / b_tokens;
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

    // merry deposits 100_000e7
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
        .unwrap()
        .fixed_mul_floor(
            fee_vault_client
                .get_shares_to_b_token_rate(&vec![&e, 0])
                .get(usdc.clone())
                .unwrap(),
            SCALAR_7,
        )
        .unwrap()
        .fixed_mul_floor(b_rate, SCALAR_9)
        .unwrap();

    let withdraw_amount = fee_vault_client.withdraw(&merry, &0, &99_9999_9999990);
    let post_withdraw_deposit_amount = fee_vault_client
        .get_deposits(&vec![&e, 0], &merry)
        .get(usdc.clone())
        .unwrap()
        .fixed_mul_floor(
            fee_vault_client
                .get_shares_to_b_token_rate(&vec![&e, 0])
                .get(usdc.clone())
                .unwrap(),
            SCALAR_7,
        )
        .unwrap()
        .fixed_mul_floor(b_rate, SCALAR_9)
        .unwrap();
    assert_eq!(post_withdraw_deposit_amount, 0);
    assert_eq!(withdraw_amount, 989_498_615_4500);
    assert_eq!(usdc_token.balance(&merry), 100_0000_0000000 + 99999999989);
    assert_eq!(deposit_amount, 999_999_999_9999);

    let reserve_data = fee_vault_client.get_reserve_data(&0);
    // check accrued fees
    let accrued_fees = reserve_data.accrued_fees;
    assert_approx_eq_abs(accrued_fees, 1050_1384549, 1000);
    // check that b_tokens are not exceeded
    let positions = pool_client.get_positions(&fee_vault);
    assert!(
        positions.supply.get(0).unwrap() >= reserve_data.total_b_tokens + reserve_data.accrued_fees
    );
    assert_eq!(
        positions.supply.get(0).unwrap(),
        reserve_data.total_b_tokens + reserve_data.accrued_fees
    );
    // check frodo deposit
    let frodo_deposit_amount = fee_vault_client
        .get_deposits(&vec![&e, 0], &frodo)
        .get(usdc.clone())
        .unwrap();
    assert_approx_eq_abs(
        frodo_deposit_amount,
        (500_000_0000000 - 1050_1384549) * b_rate / 1_000_000_000,
        10000,
    );
    blend_fixture.emitter.distribute();
    blend_fixture.backstop.gulp_emissions();
    pool_client.gulp_emissions();
    // deposit xlm in fee vault
    fee_vault_client.deposit(&frodo, &5000_0000000, &1);
    e.jump(ONE_DAY_LEDGERS * 7);
    blend_fixture.emitter.distribute();
    blend_fixture.backstop.gulp_emissions();
    pool_client.gulp_emissions();
    fee_vault_client.withdraw(&frodo, &1, &5039_6041670);
    let frodo_deposit = fee_vault_client
        .get_deposits(&vec![&e, 1], &frodo)
        .get(xlm.clone())
        .unwrap()
        .fixed_mul_floor(
            fee_vault_client
                .get_shares_to_b_token_rate(&vec![&e, 1])
                .get(xlm.clone())
                .unwrap(),
            SCALAR_7,
        )
        .unwrap();
    assert_eq!(frodo_deposit, 0);

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
    let usdc_b_rate = 1_000_000_000 * 1_000_000_000 / b_tokens;
    let b_tokens = pool_client
        .submit(
            &merry,
            &merry,
            &merry,
            &vec![
                &e,
                Request {
                    address: xlm.clone(),
                    amount: 1_000_000_000,
                    request_type: 0,
                },
            ],
        )
        .supply
        .get_unchecked(1);
    let xlm_b_rate = 1_000_000_000 * 1_000_000_000 / b_tokens;
    // try claim fees
    let pre_claim_xlm_balance = xlm_token.balance(&bombadil);
    let pre_claim_usdc_balance = usdc_token.balance(&bombadil);
    let usdc_accrued_fees = fee_vault_client.get_reserve_data(&0).accrued_fees;
    let xlm_accrued_fees = fee_vault_client.get_reserve_data(&1).accrued_fees;
    fee_vault_client.claim_fees(&vec![
        &e,
        (0, usdc_accrued_fees * usdc_b_rate / 1_000_000_000),
        (1, xlm_accrued_fees * xlm_b_rate / 1_000_000_000),
    ]);
    assert_eq!(fee_vault_client.get_reserve_data(&0).accrued_fees, 0);
    assert_eq!(fee_vault_client.get_reserve_data(&1).accrued_fees, 0);
    assert_eq!(
        xlm_token.balance(&bombadil),
        pre_claim_xlm_balance + xlm_accrued_fees * xlm_b_rate / 1_000_000_000
    );
    assert_eq!(
        usdc_token.balance(&bombadil),
        pre_claim_usdc_balance + usdc_accrued_fees * usdc_b_rate / 1_000_000_000
    );

    // test claim emissions
    let pre_blnd_balance = blnd_token.balance(&bombadil);
    let blnd_claimed = fee_vault_client.claim_emissions(&vec![&e, 1, 3]);
    assert_eq!(
        blnd_token.balance(&bombadil),
        pre_blnd_balance + blnd_claimed
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #103)")]
fn test_fee_claim_fails() {
    let e = Env::default();
    e.budget().reset_unlimited();
    e.mock_all_auths();
    e.set_default_info();

    let bombadil = Address::generate(&e);
    let frodo = Address::generate(&e);

    let blnd = e.register_stellar_asset_contract(bombadil.clone());
    let usdc = e.register_stellar_asset_contract(bombadil.clone());
    let xlm = e.register_stellar_asset_contract(bombadil.clone());
    let usdc_client = StellarAssetClient::new(&e, &usdc);
    let xlm_client = StellarAssetClient::new(&e, &xlm);
    let blend_fixture = BlendFixture::deploy(&e, &bombadil, &blnd, &usdc);
    let fee_vault = create_fee_vault(
        &e,
        &blend_fixture,
        bombadil.clone(),
        &usdc_client,
        &xlm_client,
    );
    let fee_vault_client = FeeVaultClient::new(&e, &fee_vault);
    let pool_address = fee_vault_client.get_pool();
    let pool_client = PoolClient::new(&e, &pool_address);
    // mint frodo usdc
    usdc_client.mint(&frodo, &100_0000_0000000);
    // mint frodo xlm
    xlm_client.mint(&frodo, &100_0000_0000000);
    // deposit usdc in fee vault

    fee_vault_client
        .mock_all_auths()
        .deposit(&frodo, &1_000_000_0000, &0);
    // deposit xlm in fee vault
    fee_vault_client.deposit(&frodo, &5000_0000000, &1);
    e.jump(ONE_DAY_LEDGERS * 7);

    // check bRate
    let b_tokens = pool_client
        .submit(
            &frodo,
            &frodo,
            &frodo,
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
    let usdc_b_rate = 1_000_000_000 * 1_000_000_000 / b_tokens;
    let b_tokens = pool_client
        .submit(
            &frodo,
            &frodo,
            &frodo,
            &vec![
                &e,
                Request {
                    address: xlm.clone(),
                    amount: 1_000_000_000,
                    request_type: 0,
                },
            ],
        )
        .supply
        .get_unchecked(1);
    let xlm_b_rate = 1_000_000_000 * 1_000_000_000 / b_tokens;
    // try claim fees
    let usdc_accrued_fees = fee_vault_client.get_reserve_data(&0).accrued_fees;
    let xlm_accrued_fees = fee_vault_client.get_reserve_data(&1).accrued_fees;
    fee_vault_client.claim_fees(&vec![
        &e,
        (0, usdc_accrued_fees * usdc_b_rate / 1_000_000_000 + 100),
        (1, xlm_accrued_fees * xlm_b_rate / 1_000_000_000 + 100),
    ]);
}
