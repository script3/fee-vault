#![cfg(test)]

use crate::constants::{SCALAR_7, SCALAR_9};
use crate::storage::ONE_DAY_LEDGERS;
use crate::testutils::{assert_approx_eq_rel, create_blend_pool, create_fee_vault, EnvTestUtils};
use crate::FeeVaultClient;
use blend_contract_sdk::pool::{Client as PoolClient, Request};
use blend_contract_sdk::testutils::BlendFixture;
use sep_41_token::testutils::MockTokenClient;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
use soroban_sdk::{vec, Address, Env};

#[test]
fn test_fee_accrual() {
    let e = Env::default();
    e.cost_estimate().budget().reset_unlimited();
    e.mock_all_auths();
    e.ledger().set(LedgerInfo {
        timestamp: 1441065600, // Sept 1st, 2015 12:00:00 AM UTC
        protocol_version: 22,
        sequence_number: 100,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 500 * ONE_DAY_LEDGERS,
        min_persistent_entry_ttl: 500 * ONE_DAY_LEDGERS,
        max_entry_ttl: 1000 * ONE_DAY_LEDGERS,
    });

    let bombadil = Address::generate(&e);
    let gandalf = Address::generate(&e);
    let frodo = Address::generate(&e);
    let samwise = Address::generate(&e);
    let merry = Address::generate(&e);

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
    // admin deposits 200m tokens and borrows 100m tokens for a 50% util rate
    // emits to each reserve token evently, and starts emissions
    let pool = create_blend_pool(&e, &blend_fixture, &bombadil, &usdc_client, &xlm_client);
    let pool_client = PoolClient::new(&e, &pool);
    let fee_vault = create_fee_vault(&e, &bombadil, &pool);
    let fee_vault_client = FeeVaultClient::new(&e, &fee_vault);

    fee_vault_client.add_reserve_vault(&usdc);
    fee_vault_client.add_reserve_vault(&xlm);
    fee_vault_client.set_take_rate(&0_1000000);

    /*
     * Deposit into pool
     * -> deposit 100 into fee each vault for each frodo and samwise
     * -> deposit 200 into pool for each reesrve for merry
     * -> bombadil borrow from pool to return to 50% util rate
     */

    // deposit into usdc fee vault
    let starting_balance = 1000_0000000;
    usdc_client.mint(&frodo, &starting_balance);
    usdc_client.mint(&samwise, &starting_balance);

    fee_vault_client.deposit(&usdc, &frodo, &starting_balance);
    fee_vault_client.deposit(&usdc, &samwise, &starting_balance);

    // deposit into usdc reserve
    let merry_starting_balance = starting_balance * 2;
    usdc_client.mint(&merry, &merry_starting_balance);
    pool_client.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &e,
            Request {
                request_type: 0,
                address: usdc.clone(),
                amount: merry_starting_balance,
            },
        ],
    );

    // borrow from pool to return USDC to 50% util rate
    let borrow_amount = merry_starting_balance;
    pool_client.submit(
        &bombadil,
        &bombadil,
        &bombadil,
        &vec![
            &e,
            Request {
                request_type: 4,
                address: usdc.clone(),
                amount: borrow_amount,
            },
        ],
    );

    // deposit into xlm fee vault
    xlm_client.mint(&frodo, &starting_balance);
    xlm_client.mint(&samwise, &starting_balance);

    fee_vault_client.deposit(&xlm, &frodo, &starting_balance);
    fee_vault_client.deposit(&xlm, &samwise, &starting_balance);

    // deposit into xlm reserve
    xlm_client.mint(&merry, &merry_starting_balance);
    pool_client.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &e,
            Request {
                request_type: 0,
                address: xlm.clone(),
                amount: merry_starting_balance,
            },
        ],
    );

    // borrow from pool to return XLM to 50% util rate
    let borrow_amount = merry_starting_balance;
    pool_client.submit(
        &bombadil,
        &bombadil,
        &bombadil,
        &vec![
            &e,
            Request {
                request_type: 4,
                address: xlm.clone(),
                amount: borrow_amount,
            },
        ],
    );

    /*
     * Cause a bunch of accruals to verify fees are charged correctly.
     *
     * -> Cause a b_rate update on the pool every day for 100 days
     * -> For USDC, accrued to fee vault daily.
     * -> For XLM, accrued to fee vault ~monthly.
     * -> Verify fee's charged for each reserve are approximately the same
     */
    usdc_client.mint(&gandalf, &1000_0000000);
    xlm_client.mint(&gandalf, &1000_0000000);
    for day in 0..365 {
        e.jump_time(86400);

        let usdc_deposit = 10000;
        // deposit into usdc fee vault every day
        fee_vault_client.deposit(&usdc, &gandalf, &usdc_deposit);

        // deposit into xlm fee vault every month
        let mut xlm_deposit = 0;
        if day % 30 == 0 {
            xlm_deposit = 300000;
            fee_vault_client.deposit(&xlm, &gandalf, &xlm_deposit);
        }

        // supply from pool to cause b_rate update and maintain ~50% util rate
        // 100m tokens borrowed for each reserve @ a 10% borrow rate
        let approx_daily_interest = 0_2000000;
        pool_client.submit(
            &bombadil,
            &bombadil,
            &bombadil,
            &vec![
                &e,
                Request {
                    request_type: 2,
                    address: xlm.clone(),
                    amount: approx_daily_interest + xlm_deposit / 2,
                },
                Request {
                    request_type: 2,
                    address: usdc.clone(),
                    amount: approx_daily_interest + usdc_deposit / 2,
                },
            ],
        );
    }

    // deposit into both fee vaults on final ledger to update b_rate
    fee_vault_client.deposit(&usdc, &gandalf, &100_0000000);
    fee_vault_client.deposit(&xlm, &gandalf, &100_0000000);

    // calculate merry profit for 200 USDC and 200 XLM deposits
    pool_client.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &e,
            Request {
                request_type: 1,
                address: usdc.clone(),
                amount: merry_starting_balance * 2,
            },
            Request {
                request_type: 1,
                address: xlm.clone(),
                amount: merry_starting_balance * 2,
            },
        ],
    );
    let merry_final_usdc = usdc_client.balance(&merry);
    let merry_profit_usdc = merry_final_usdc - merry_starting_balance;
    let merry_final_xlm = xlm_client.balance(&merry);
    let merry_profit_xlm = merry_final_xlm - merry_starting_balance;

    // validate frodo can withdraw his expected share of the profit
    // -> @dev: this is expected to be less as than expected as fees are accrued in b_tokens, reducing future interest
    let usdc_vault = fee_vault_client.get_reserve_vault(&usdc);
    let usdc_withdrawal_amount = usdc_vault
        .shares_to_b_tokens_down(starting_balance)
        .fixed_mul_floor(usdc_vault.b_rate, SCALAR_9)
        .unwrap();
    let frodo_profit_usdc = usdc_withdrawal_amount - starting_balance;
    // let frodo_profit_usdc = 4_7207977;
    assert_approx_eq_rel(
        frodo_profit_usdc,
        (merry_profit_usdc / 2)
            .fixed_mul_floor(0_9000000, SCALAR_7)
            .unwrap(),
        0_0100000,
    );
    let usdc_withdraw_amount = starting_balance + frodo_profit_usdc;
    fee_vault_client.withdraw(&usdc, &frodo, &usdc_withdraw_amount);

    assert_eq!(usdc_client.balance(&frodo), usdc_withdraw_amount);
    // -> verify no more than dust shares left for frodo
    assert!(fee_vault_client.get_shares(&usdc, &frodo) < 10);

    // -> @dev: this is expected to be less than expected as fees are accrued in b_tokens, reducing future interest
    let xlm_vault = fee_vault_client.get_reserve_vault(&xlm);
    let xlm_withdrawal_amount = xlm_vault
        .shares_to_b_tokens_down(starting_balance)
        .fixed_mul_floor(xlm_vault.b_rate, SCALAR_9)
        .unwrap();
    let frodo_profit_xlm = xlm_withdrawal_amount - starting_balance;
    // let frodo_profit_xlm = 4_7300002;
    assert_approx_eq_rel(
        frodo_profit_xlm,
        (merry_profit_xlm / 2)
            .fixed_mul_floor(0_9000000, SCALAR_7)
            .unwrap(),
        0_0100000,
    );
    let withdraw_amount_xlm = starting_balance + frodo_profit_xlm;
    fee_vault_client.withdraw(&xlm, &frodo, &withdraw_amount_xlm);

    assert_eq!(xlm_client.balance(&frodo), withdraw_amount_xlm);
    // -> verify no more than dust shares left for frodo
    assert!(fee_vault_client.get_shares(&xlm, &frodo) < 10);

    // verify profit is close regardless of accrual rate
    assert_approx_eq_rel(frodo_profit_xlm, frodo_profit_usdc, 0_0100000);

    // admin claim profits
    let pre_claim_usdc = usdc_client.balance(&bombadil);
    let admin_usdc_fees = usdc_vault
        .accrued_fees
        .fixed_mul_floor(usdc_vault.b_rate, SCALAR_9)
        .unwrap();
    fee_vault_client.claim_fees(&usdc, &bombadil, &admin_usdc_fees);
    assert_eq!(
        usdc_client.balance(&bombadil),
        admin_usdc_fees + pre_claim_usdc
    );
    // -> verify only dust leftover in fee vault
    let post_claim_usdc_vault = fee_vault_client.get_reserve_vault(&usdc);
    assert!(post_claim_usdc_vault.accrued_fees < 10);

    // verify merry profit is approximately equal to total vault profit.
    assert_approx_eq_rel(
        admin_usdc_fees + frodo_profit_usdc * 2,
        merry_profit_usdc,
        0_0100000,
    );

    let pre_claim_xlm = xlm_client.balance(&bombadil);
    let admin_xlm_fees = xlm_vault
        .accrued_fees
        .fixed_mul_floor(xlm_vault.b_rate, SCALAR_9)
        .unwrap()
        - 10;
    fee_vault_client.claim_fees(&xlm, &bombadil, &admin_xlm_fees);
    assert_eq!(
        xlm_client.balance(&bombadil),
        admin_xlm_fees + pre_claim_xlm
    );
    // -> verify only dust leftover in fee vault
    let post_claim_xlm_vault = fee_vault_client.get_reserve_vault(&xlm);
    assert!(post_claim_xlm_vault.accrued_fees < 10);

    // verify merry profit is approximately equal to total vault profit.
    assert_approx_eq_rel(
        admin_xlm_fees + frodo_profit_xlm * 2,
        merry_profit_xlm,
        0_0100000,
    );
}
