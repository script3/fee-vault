#![cfg(test)]

use crate::testutils::{create_fee_vault, EnvTestUtils};
use crate::FeeVaultClient;
use blend_contract_sdk::testutils::BlendFixture;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env};

#[test]
#[should_panic(expected = "Error(Contract, #102)")]
fn test_over_withdraw() {
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
    // mint frodo usdc
    usdc_client.mint(&frodo, &100_0000_0000000);
    // deposit usdc in fee vault
    fee_vault_client.deposit(&bombadil, &1_000_000_0000, &0);

    // deposit more usdc in fee vault
    fee_vault_client.deposit(&frodo, &1_000_000_0000, &0);
    // attempt to withdraw more than available
    fee_vault_client.withdraw(&frodo, &1_000_000_0001, &0);
}
