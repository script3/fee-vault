#![cfg(test)]

use crate::dependencies::pool::{Client as PoolClient, Request};
use crate::storage::ONE_DAY_LEDGERS;
use crate::testutils::{create_fee_vault, EnvTestUtils};
use crate::FeeVaultClient;
use blend_contract_sdk::testutils::BlendFixture;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{vec, Address, Env};

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
