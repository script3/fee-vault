#![cfg(test)]

use crate::constants::MIN_DUST;
use crate::testutils::{create_blend_pool, create_fee_vault, EnvTestUtils};
use crate::FeeVaultClient;
use blend_contract_sdk::testutils::BlendFixture;
use sep_41_token::testutils::MockTokenClient;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Error};

#[test]
fn test_dust() {
    let e = Env::default();
    e.budget().reset_unlimited();
    e.mock_all_auths();
    e.set_default_info();
    
    let bombadil = Address::generate(&e);
    let frodo = Address::generate(&e);
    let samwise = Address::generate(&e);

    let blnd = e.register_stellar_asset_contract(bombadil.clone());
    let usdc = e.register_stellar_asset_contract(bombadil.clone());
    let xlm = e.register_stellar_asset_contract(bombadil.clone());
    let usdc_client = MockTokenClient::new(&e, &usdc);
    let xlm_client = MockTokenClient::new(&e, &xlm);


    let blend_fixture = BlendFixture::deploy(&e, &bombadil, &blnd, &usdc);
    // usdc (0) and xlm (1) charge a fixed 10% borrow rate with 0% backstop take rate
    // admin deposits 200m tokens and borrows 100m tokens for a 50% util rate
    // emits to each reserve token evently, and starts emissions
    let pool = create_blend_pool(&e, &blend_fixture, &bombadil, &usdc_client, &xlm_client);
    let fee_vault = create_fee_vault(&e, &bombadil, &pool);
    let fee_vault_client = FeeVaultClient::new(&e, &fee_vault);

    fee_vault_client.add_reserve_vault(&0, &usdc);
    fee_vault_client.set_take_rate(&0_1000000);

    let starting_balance = 100_0000000;
    usdc_client.mint(&frodo, &starting_balance);
    usdc_client.mint(&samwise, &starting_balance);

    // verify deposit has dust protection
    let result = fee_vault_client.try_deposit(&usdc, &frodo, &(MIN_DUST - 1));
    assert_eq!(result.err(), Some(Ok(Error::from_contract_error(102))));

    let result = fee_vault_client.try_deposit(&usdc, &frodo, &(-1));
    assert_eq!(result.err(), Some(Ok(Error::from_contract_error(102))));

    fee_vault_client.deposit(&usdc, &frodo, &MIN_DUST);
    assert_eq!(fee_vault_client.get_shares(&usdc, &frodo), MIN_DUST);

    // verify withdraw has dust protection
    let result = fee_vault_client.try_withdraw(&usdc, &frodo, &(MIN_DUST - 1));
    assert_eq!(result.err(), Some(Ok(Error::from_contract_error(102))));

    let result = fee_vault_client.try_withdraw(&usdc, &frodo, &(-1));
    assert_eq!(result.err(), Some(Ok(Error::from_contract_error(102))));

    fee_vault_client.withdraw(&usdc, &frodo, &MIN_DUST);
    assert_eq!(fee_vault_client.get_shares(&usdc, &frodo), 0);

    // deposit funds
    fee_vault_client.deposit(&usdc, &frodo, &(starting_balance / 2));

    // allow interest to accrue
    e.jump_time(7 * 24 * 60 * 60);

    // withdraw funds to accrue fees
    fee_vault_client.withdraw(&usdc, &frodo, &(starting_balance / 2));

    // verify claim has dust protection
    let result = fee_vault_client.try_claim_fees(&usdc, &bombadil, &(MIN_DUST - 1));
    assert_eq!(result.err(), Some(Ok(Error::from_contract_error(102))));

    let result = fee_vault_client.try_claim_fees(&usdc, &bombadil, &(-1));
    assert_eq!(result.err(), Some(Ok(Error::from_contract_error(102))));

    let pre_claim_usdc = usdc_client.balance(&bombadil);
    fee_vault_client.claim_fees(&usdc, &bombadil, &MIN_DUST);
    assert_eq!(usdc_client.balance(&bombadil), pre_claim_usdc + MIN_DUST);
}
