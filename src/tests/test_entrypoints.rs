#![cfg(test)]

use crate::{
    reserve_vault::ReserveVault,
    storage,
    testutils::{create_blend_pool, create_fee_vault, mockpool, register_fee_vault},
    FeeVaultClient,
};
use blend_contract_sdk::testutils::BlendFixture;
use sep_41_token::testutils::MockTokenClient;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
    vec, Address, Env, Error, IntoVal, Symbol,
};

#[test]
fn test_constructor_ok() {
    let e = Env::default();
    e.mock_all_auths();
    let samwise = Address::generate(&e);
    let blend_pool = Address::generate(&e);
    let take_rate = 1_000_0000;

    let vault_address =
        register_fee_vault(&e, Some((samwise.clone(), blend_pool.clone(), take_rate)));

    assert_eq!(
        e.auths()[0],
        (
            samwise.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    vault_address.clone(),
                    Symbol::new(&e, "__constructor"),
                    vec![
                        &e,
                        samwise.into_val(&e),
                        blend_pool.into_val(&e),
                        take_rate.into_val(&e),
                    ]
                )),
                sub_invocations: std::vec![]
            }
        )
    );

    e.as_contract(&vault_address, || {
        assert_eq!(storage::get_admin(&e), samwise);
        assert_eq!(storage::get_take_rate(&e), take_rate);
        assert_eq!(storage::get_pool(&e), blend_pool);
    });
}

#[test]
#[should_panic(expected = "Error(Context, InvalidAction)")]
fn test_constructor_negative_take_rate() {
    let e = Env::default();
    e.mock_all_auths();
    let samwise = Address::generate(&e);
    // Note: This fails with `InvalidAction` during testing, rather than `InvalidTakeRate`
    register_fee_vault(&e, Some((samwise.clone(), samwise.clone(), -1)));
}

#[test]
#[should_panic(expected = "Error(Context, InvalidAction)")]
fn test_constructor_take_rate_over_max() {
    let e = Env::default();
    e.mock_all_auths();
    let samwise = Address::generate(&e);

    // Note: This fails with `InvalidAction` during testing, rather than `InvalidTakeRate`
    register_fee_vault(&e, Some((samwise.clone(), samwise.clone(), 1_000_0001)));
}

#[test]
fn test_get_b_tokens() {
    let e = Env::default();
    e.mock_all_auths();

    let samwise = Address::generate(&e);
    let frodo = Address::generate(&e);
    let reserve = Address::generate(&e);
    let init_b_rate = 1_000_000_000;

    let mock_client = mockpool::register_mock_pool_with_b_rate(&e, init_b_rate);
    let vault_address = register_fee_vault(
        &e,
        Some((samwise.clone(), mock_client.address.clone(), 0_1000000)),
    );

    let vault_client = FeeVaultClient::new(&e, &vault_address);

    e.as_contract(&vault_address, || {
        let reserve_vault = ReserveVault {
            address: reserve.clone(),
            total_b_tokens: 1000_0000000,
            total_shares: 1200_0000000,
            b_rate: init_b_rate,
            accrued_fees: 0,
        };
        storage::set_reserve_vault(&e, &reserve, &reserve_vault);

        // samwise owns 10% of the pool, frodo owns 90%
        storage::set_reserve_vault_shares(&e, &reserve, &samwise, 120_0000000);
        storage::set_reserve_vault_shares(&e, &reserve, &frodo, 1080_0000000);
    });
    assert_eq!(vault_client.get_b_tokens(&reserve, &samwise), 100_0000000);
    assert_eq!(vault_client.get_b_tokens(&reserve, &frodo), 900_0000000);

    // b_rate is increased by 10%. `take_rate` is 10%
    mock_client.set_b_rate(&1_100_000_000);
    let expected_accrued_fees = 90909090_i128;
    let expected_total_b_tokens = 1000_0000000 - expected_accrued_fees;

    // Ensure get_b_tokens always returns updated results, even though b_rate hasn't been updated
    assert_eq!(
        vault_client.get_b_tokens(&reserve, &samwise),
        expected_total_b_tokens.fixed_mul_floor(10, 100).unwrap()
    );
    assert_eq!(
        vault_client.get_b_tokens(&reserve, &frodo),
        expected_total_b_tokens.fixed_mul_floor(90, 100).unwrap()
    );

    // The view function shouldn't mutate the state
    e.as_contract(&vault_address, || {
        let reserve_vault = storage::get_reserve_vault(&e, &reserve);
        assert_eq!(reserve_vault.accrued_fees, 0);
        assert_eq!(reserve_vault.total_b_tokens, 1000_0000000);
        assert_eq!(reserve_vault.total_shares, 1200_0000000);
        assert_eq!(reserve_vault.b_rate, 1_000_000_000);
    });

    // Should return 0 if vault doesn't exist or user doesn't have any shares
    let non_existent_reserve = Address::generate(&e);
    let non_existent_user = Address::generate(&e);
    assert_eq!(
        vault_client.get_b_tokens(&non_existent_reserve, &samwise),
        0
    );
    assert_eq!(vault_client.get_b_tokens(&reserve, &non_existent_user), 0);
    assert_eq!(
        vault_client.get_b_tokens(&non_existent_reserve, &non_existent_user),
        0
    );
}

#[test]
fn test_set_take_rate() {
    let e = Env::default();
    e.mock_all_auths();

    let samwise = Address::generate(&e);

    let vault_address = register_fee_vault(
        &e,
        Some((samwise.clone(), Address::generate(&e), 0_1000000)),
    );
    let vault_client = FeeVaultClient::new(&e, &vault_address);

    // Take rate should be in range 0..1_000_0000
    assert_eq!(
        vault_client.try_set_take_rate(&-1).err(),
        Some(Ok(Error::from_contract_error(104)))
    );
    assert_eq!(
        vault_client.try_set_take_rate(&1_000_0001).err(),
        Some(Ok(Error::from_contract_error(104)))
    );

    // Set take rate to 0.5
    let take_rate = 500_000;
    vault_client.set_take_rate(&take_rate);
    assert_eq!(
        e.auths()[0],
        (
            samwise.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    vault_address.clone(),
                    Symbol::new(&e, "set_take_rate"),
                    vec![&e, take_rate.into_val(&e),]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    e.as_contract(&vault_address, || {
        assert_eq!(storage::get_take_rate(&e), 500_000);
    });
    // Setting the take_rate to 0 or 100% should be possible
    vault_client.set_take_rate(&0);
    e.as_contract(&vault_address, || {
        assert_eq!(storage::get_take_rate(&e), 0);
    });
    vault_client.set_take_rate(&1_000_0000);
    e.as_contract(&vault_address, || {
        assert_eq!(storage::get_take_rate(&e), 1_000_0000);
    });
}

#[test]
fn test_set_admin() {
    let e = Env::default();
    e.mock_all_auths();

    let samwise = Address::generate(&e);
    let frodo = Address::generate(&e);

    let vault_address = register_fee_vault(
        &e,
        Some((samwise.clone(), Address::generate(&e), 0_1000000)),
    );
    let vault_client = FeeVaultClient::new(&e, &vault_address);

    e.as_contract(&vault_address, || {
        // samwise is the current admin
        assert_eq!(storage::get_admin(&e), samwise.clone());
    });

    vault_client.set_admin(&frodo);

    let authorized_function = AuthorizedInvocation {
        function: AuthorizedFunction::Contract((
            vault_address.clone(),
            Symbol::new(&e, "set_admin"),
            vec![&e, frodo.into_val(&e)],
        )),
        sub_invocations: std::vec![],
    };
    // auths[0] should be the old admin, auths[1] should be the new admin
    assert_eq!(
        e.auths(),
        std::vec![
            (samwise.clone(), authorized_function.clone()),
            (frodo.clone(), authorized_function)
        ]
    );

    e.as_contract(&vault_address, || {
        // The new admin is frodo
        assert_eq!(storage::get_admin(&e), frodo);
    });

    // Frodo should be able to also set a new admin
    let new_admin = Address::generate(&e);
    vault_client.set_admin(&new_admin);

    let new_authorized_function = AuthorizedInvocation {
        function: AuthorizedFunction::Contract((
            vault_address.clone(),
            Symbol::new(&e, "set_admin"),
            vec![&e, new_admin.into_val(&e)],
        )),
        sub_invocations: std::vec![],
    };
    assert_eq!(
        e.auths(),
        std::vec![
            (frodo.clone(), new_authorized_function.clone()),
            (new_admin.clone(), new_authorized_function)
        ]
    );
}

#[test]
fn test_add_reserve_vault() {
    let e = Env::default();
    e.mock_all_auths();

    let samwise = Address::generate(&e);
    let reserve = Address::generate(&e);

    let mock_client = mockpool::register_mock_pool_with_b_rate(&e, 1_100_000_000);
    let vault_address = register_fee_vault(
        &e,
        Some((samwise.clone(), mock_client.address.clone(), 0_1000000)),
    );

    let vault_client = FeeVaultClient::new(&e, &vault_address);

    // Trying to get the reserve vault before adding it should fail
    assert_eq!(
        vault_client.try_get_reserve_vault(&reserve).err(),
        Some(Ok(Error::from_contract_error(100)))
    );

    vault_client.add_reserve_vault(&reserve);
    assert_eq!(
        e.auths()[0],
        (
            samwise.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    vault_address.clone(),
                    Symbol::new(&e, "add_reserve_vault"),
                    vec![&e, reserve.into_val(&e),]
                )),
                sub_invocations: std::vec![]
            }
        )
    );

    let reserve_info = vault_client.get_reserve_vault(&reserve);

    assert_eq!(reserve_info.address, reserve);
    assert_eq!(reserve_info.total_b_tokens, 0);
    assert_eq!(reserve_info.total_shares, 0);
    // The init b_rate of the pool at the time of registering the vault was 1.1
    assert_eq!(reserve_info.b_rate, 1_100_000_000);
    assert_eq!(reserve_info.accrued_fees, 0);

    // Trying to add a vault for the same reserve should fail
    assert_eq!(
        vault_client.try_add_reserve_vault(&reserve).err(),
        Some(Ok(Error::from_contract_error(101)))
    );
}

#[test]
#[should_panic(expected = "Error(Storage, MissingValue)")]
fn test_add_invalid_reserve() {
    let e = Env::default();
    e.cost_estimate().budget().reset_unlimited();
    e.mock_all_auths();

    let bombadil = Address::generate(&e);

    // Deploy a blend pool with 2 mock assets, USDC and XLM
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
    let pool = create_blend_pool(&e, &blend_fixture, &bombadil, &usdc_client, &xlm_client);
    let fee_vault = create_fee_vault(&e, &bombadil, &pool);
    let fee_vault_client = FeeVaultClient::new(&e, &fee_vault);

    // Adding an existent reserve should succeed
    assert_eq!(fee_vault_client.try_add_reserve_vault(&usdc).is_ok(), true);
    // Adding the same reserve again should fail
    assert_eq!(
        fee_vault_client.try_add_reserve_vault(&usdc).err(),
        Some(Ok(Error::from_contract_error(101)))
    );

    // Adding a different reserve should also succeed
    assert_eq!(fee_vault_client.try_add_reserve_vault(&xlm).is_ok(), true);

    // Adding a non-existent reserve should fail
    fee_vault_client.add_reserve_vault(&Address::generate(&e));
}
