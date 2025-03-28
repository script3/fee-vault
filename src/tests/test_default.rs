#![cfg(test)]

use crate::constants::{SCALAR_12, SCALAR_7};
use crate::testutils::{assert_approx_eq_abs, create_blend_pool, create_fee_vault, EnvTestUtils};
use crate::FeeVaultClient;
use blend_contract_sdk::pool::{Client as PoolClient, PoolDataKey, Request};
use blend_contract_sdk::testutils::BlendFixture;
use sep_41_token::testutils::MockTokenClient;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{unwrap::UnwrapOptimized, vec, Address, Env};

#[test]
fn test_default() {
    let e = Env::default();
    e.cost_estimate().budget().reset_unlimited();
    e.mock_all_auths();
    e.set_default_info();

    let bombadil = Address::generate(&e);
    let gandalf = Address::generate(&e);
    let frodo = Address::generate(&e);
    let samwise = Address::generate(&e);

    let blnd = e
        .register_stellar_asset_contract_v2(bombadil.clone())
        .address();
    let usdc = e
        .register_stellar_asset_contract_v2(bombadil.clone())
        .address();
    let xlm = e
        .register_stellar_asset_contract_v2(bombadil.clone())
        .address();
    let usdc_client = MockTokenClient::new(&e, &usdc);
    let xlm_client = MockTokenClient::new(&e, &xlm);

    let blend_fixture = BlendFixture::deploy(&e, &bombadil, &blnd, &usdc);

    // usdc (0) and xlm (1) charge a fixed 10% borrow rate with 0% backstop take rate
    // emits to each reserve token evently, and starts emissions
    let pool = create_blend_pool(&e, &blend_fixture, &bombadil, &usdc_client, &xlm_client);
    let pool_client = PoolClient::new(&e, &pool);
    let fee_vault = create_fee_vault(&e, &gandalf, &pool, false, 100_0000);
    let fee_vault_client = FeeVaultClient::new(&e, &fee_vault);

    // Setup pool util rate
    // Bomadil deposits 200k tokens and borrows 100k tokens for a 50% util rate
    let requests = vec![
        &e,
        Request {
            address: usdc.clone(),
            amount: 200_000_0000000,
            request_type: 2,
        },
        Request {
            address: usdc.clone(),
            amount: 105_000_0000000,
            request_type: 4,
        },
        Request {
            address: xlm.clone(),
            amount: 200_000_0000000,
            request_type: 2,
        },
        Request {
            address: xlm.clone(),
            amount: 100_000_0000000,
            request_type: 4,
        },
    ];
    pool_client
        .mock_all_auths()
        .submit(&bombadil, &bombadil, &bombadil, &requests);

    fee_vault_client.add_reserve_vault(&usdc);

    let pool_usdc_balance_start = usdc_client.balance(&pool);

    // have samwise and frodo deposit funds into reserve vault
    let samwise_deposit: i128 = 1_000_0000000;
    let frodo_deposit: i128 = 9_000_0000000;
    usdc_client.mint(&samwise, &(samwise_deposit * 2));
    usdc_client.mint(&frodo, &(frodo_deposit * 2));

    fee_vault_client.deposit(&usdc, &samwise, &samwise_deposit);
    fee_vault_client.deposit(&usdc, &frodo, &frodo_deposit);

    assert_eq!(
        fee_vault_client.get_underlying_tokens(&usdc, &samwise),
        samwise_deposit
    );
    assert_eq!(
        fee_vault_client.get_underlying_tokens(&usdc, &frodo),
        frodo_deposit
    );
    assert_eq!(
        usdc_client.balance(&pool),
        pool_usdc_balance_start + samwise_deposit + frodo_deposit
    );

    // pass 1m day to accrue some fees (approx 0.41% gain w/ 10% fixed rate and ~50% util)
    // vault suppliers should see 90% of that, or ~0.37%
    e.jump_time(30 * 86400);

    // have frodo do a 10 stroop deposit to trigger fee accrual this block
    fee_vault_client.deposit(&usdc, &frodo, &10);

    // check fee accrual amount is not dust
    let cur_accrued = fee_vault_client.get_collected_fees(&usdc);
    assert!(cur_accrued > 0);

    let usdc_data = pool_client.get_reserve(&usdc);
    let pre_supply = usdc_data
        .data
        .b_rate
        .fixed_mul_floor(usdc_data.data.b_supply, SCALAR_12)
        .unwrap_optimized();
    // use magic to simulate a default situation of 10%
    e.as_contract(&pool, || {
        let res_data_key = PoolDataKey::ResData(usdc.clone());
        let mut new_res_data = usdc_data.data.clone();
        new_res_data.b_rate = new_res_data
            .b_rate
            .fixed_mul_floor(0_9000000, SCALAR_7)
            .unwrap_optimized();
        let new_supply = new_res_data
            .b_supply
            .fixed_mul_floor(new_res_data.b_rate, SCALAR_12)
            .unwrap_optimized();
        new_res_data.d_supply = (pre_supply - new_supply)
            .fixed_div_floor(new_res_data.d_rate, SCALAR_12)
            .unwrap_optimized();
        e.storage().persistent().set(&res_data_key, &new_res_data);
    });

    // estimate expected loss frodo and samwise should take, as a percentage
    let expected_loss = 1_0036986i128
        .fixed_mul_floor(0_9000000, SCALAR_7)
        .unwrap_optimized();

    // withdraw frodo at the same time and check he took expected loss
    let frodo_withdraw_amount = fee_vault_client.get_underlying_tokens(&usdc, &frodo);
    fee_vault_client.withdraw(&usdc, &frodo, &frodo_withdraw_amount);
    assert_approx_eq_abs(
        frodo_withdraw_amount,
        frodo_deposit
            .fixed_mul_floor(expected_loss, SCALAR_7)
            .unwrap_optimized(),
        0_0010000,
    );

    // skip some time
    e.jump_time(100);

    // withdraw samwise and check loss
    let samwise_withdraw_amount = fee_vault_client.get_underlying_tokens(&usdc, &samwise);
    fee_vault_client.withdraw(&usdc, &samwise, &samwise_withdraw_amount);
    assert_approx_eq_abs(
        samwise_withdraw_amount,
        samwise_deposit
            .fixed_mul_floor(expected_loss, SCALAR_7)
            .unwrap_optimized(),
        0_0010000,
    );
}
