#![cfg(test)]

use crate::{constants::SCALAR_7, storage::ONE_DAY_LEDGERS, FeeVault};
use blend_contract_sdk::pool::{
    Client as PoolClient, Request, ReserveConfig, ReserveEmissionMetadata,
};
use blend_contract_sdk::testutils::BlendFixture;
use sep_41_token::testutils::MockTokenClient;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{
    testutils::{Address as _, BytesN as _, Ledger as _, LedgerInfo},
    vec, Address, BytesN, Env, String, Symbol,
};

// Defaults to a mock pool with a b_rate of 1_100_000_000 and a take_rate of 0_1000000.
pub(crate) fn register_fee_vault(
    e: &Env,
    constructor_args: Option<(Address, Address, i128)>,
) -> Address {
    e.register(
        FeeVault {},
        constructor_args.unwrap_or((
            Address::generate(e),
            mockpool::register_mock_pool_with_b_rate(e, 1_100_000_000).address,
            0_1000000,
        )),
    )
}

pub(crate) fn create_blend_pool(
    e: &Env,
    blend_fixture: &BlendFixture,
    admin: &Address,
    usdc: &MockTokenClient,
    xlm: &MockTokenClient,
) -> Address {
    // Mint usdc to admin
    usdc.mint(&admin, &200_000_0000000);
    // Mint xlm to admin
    xlm.mint(&admin, &200_000_0000000);

    // set up oracle
    let (oracle, oracle_client) = create_mock_oracle(e);
    oracle_client.set_data(
        &admin,
        &Asset::Other(Symbol::new(&e, "USD")),
        &vec![
            e,
            Asset::Stellar(usdc.address.clone()),
            Asset::Stellar(xlm.address.clone()),
        ],
        &7,
        &300,
    );
    oracle_client.set_price_stable(&vec![e, 1_000_0000, 100_0000]);
    let salt = BytesN::<32>::random(&e);
    let pool = blend_fixture.pool_factory.deploy(
        &admin,
        &String::from_str(e, "TEST"),
        &salt,
        &oracle,
        &0,
        &4,
    );
    let pool_client = PoolClient::new(e, &pool);
    blend_fixture
        .backstop
        .deposit(&admin, &pool, &20_0000_0000000);
    let reserve_config = ReserveConfig {
        c_factor: 900_0000,
        decimals: 7,
        index: 0,
        l_factor: 900_0000,
        max_util: 900_0000,
        reactivity: 0,
        r_base: 100_0000,
        r_one: 0,
        r_two: 0,
        r_three: 0,
        util: 0,
        collateral_cap: 170_141_183_460_469_231_731_687_303_715_884_105_727,
        enabled: true,
    };
    pool_client.queue_set_reserve(&usdc.address, &reserve_config);
    pool_client.set_reserve(&usdc.address);
    pool_client.queue_set_reserve(&xlm.address, &reserve_config);
    pool_client.set_reserve(&xlm.address);
    let emission_config = vec![
        e,
        ReserveEmissionMetadata {
            res_index: 0,
            res_type: 0,
            share: 250_0000,
        },
        ReserveEmissionMetadata {
            res_index: 0,
            res_type: 1,
            share: 250_0000,
        },
        ReserveEmissionMetadata {
            res_index: 1,
            res_type: 0,
            share: 250_0000,
        },
        ReserveEmissionMetadata {
            res_index: 1,
            res_type: 1,
            share: 250_0000,
        },
    ];
    pool_client.set_emissions_config(&emission_config);
    pool_client.set_status(&0);
    blend_fixture.backstop.add_reward(&pool, &None);

    // wait a week and start emissions
    e.jump(ONE_DAY_LEDGERS * 7);
    blend_fixture.emitter.distribute();
    // blend_fixture.backstop.gulp_emissions();
    // pool_client.gulp_emissions();

    // admin joins pool
    let requests = vec![
        e,
        Request {
            address: usdc.address.clone(),
            amount: 200_000_0000000,
            request_type: 2,
        },
        Request {
            address: usdc.address.clone(),
            amount: 100_000_0000000,
            request_type: 4,
        },
        Request {
            address: xlm.address.clone(),
            amount: 200_000_0000000,
            request_type: 2,
        },
        Request {
            address: xlm.address.clone(),
            amount: 100_000_0000000,
            request_type: 4,
        },
    ];
    pool_client
        .mock_all_auths()
        .submit(&admin, &admin, &admin, &requests);
    return pool;
}

/// Create a fee vault
pub(crate) fn create_fee_vault(e: &Env, admin: &Address, pool: &Address) -> Address {
    register_fee_vault(e, Some((admin.clone(), pool.clone(), 100_0000)))
}

pub trait EnvTestUtils {
    /// Jump the env by the given amount of ledgers. Assumes 5 seconds per ledger.
    fn jump(&self, ledgers: u32);

    /// Jump the env by the given amount of seconds. Incremends the sequence by 1.
    fn jump_time(&self, seconds: u64);

    /// Set the ledger to the default LedgerInfo
    ///
    /// Time -> 1441065600 (Sept 1st, 2015 12:00:00 AM UTC)
    /// Sequence -> 100
    fn set_default_info(&self);
}

impl EnvTestUtils for Env {
    fn jump(&self, ledgers: u32) {
        self.ledger().set(LedgerInfo {
            timestamp: self.ledger().timestamp().saturating_add(ledgers as u64 * 5),
            protocol_version: 22,
            sequence_number: self.ledger().sequence().saturating_add(ledgers),
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 30 * ONE_DAY_LEDGERS,
            min_persistent_entry_ttl: 30 * ONE_DAY_LEDGERS,
            max_entry_ttl: 365 * ONE_DAY_LEDGERS,
        });
    }

    fn jump_time(&self, seconds: u64) {
        self.ledger().set(LedgerInfo {
            timestamp: self.ledger().timestamp().saturating_add(seconds),
            protocol_version: 22,
            sequence_number: self.ledger().sequence().saturating_add(1),
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 30 * ONE_DAY_LEDGERS,
            min_persistent_entry_ttl: 30 * ONE_DAY_LEDGERS,
            max_entry_ttl: 365 * ONE_DAY_LEDGERS,
        });
    }

    fn set_default_info(&self) {
        self.ledger().set(LedgerInfo {
            timestamp: 1441065600, // Sept 1st, 2015 12:00:00 AM UTC
            protocol_version: 22,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 30 * ONE_DAY_LEDGERS,
            min_persistent_entry_ttl: 30 * ONE_DAY_LEDGERS,
            max_entry_ttl: 365 * ONE_DAY_LEDGERS,
        });
    }
}

pub fn assert_approx_eq_abs(a: i128, b: i128, delta: i128) {
    assert!(
        a > b - delta && a < b + delta,
        "assertion failed: `(left != right)` \
         (left: `{:?}`, right: `{:?}`, epsilon: `{:?}`)",
        a,
        b,
        delta
    );
}

/// Asset that `b` is within `percentage` of `a` where `percentage`
/// is a percentage in decimal form as a fixed-point number with 7 decimal
/// places
pub fn assert_approx_eq_rel(a: i128, b: i128, percentage: i128) {
    let rel_delta = b.fixed_mul_floor(percentage, SCALAR_7).unwrap();

    assert!(
        a > b - rel_delta && a < b + rel_delta,
        "assertion failed: `(left != right)` \
         (left: `{:?}`, right: `{:?}`, epsilon: `{:?}`)",
        a,
        b,
        rel_delta
    );
}

/// Oracle
use sep_40_oracle::testutils::{Asset, MockPriceOracleClient, MockPriceOracleWASM};

pub fn create_mock_oracle<'a>(e: &Env) -> (Address, MockPriceOracleClient<'a>) {
    let contract_id = Address::generate(e);
    e.register_at(&contract_id, MockPriceOracleWASM, ());
    (
        contract_id.clone(),
        MockPriceOracleClient::new(e, &contract_id),
    )
}

/// Mock pool to test b_rate updates
pub mod mockpool {

    use super::*;
    use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Symbol};

    const BRATE: Symbol = symbol_short!("b_rate");
    #[derive(Clone)]
    #[contracttype]
    pub struct Reserve {
        pub asset: Address,        // the underlying asset address
        pub index: u32,            // the reserve index in the pool
        pub l_factor: u32,         // the liability factor for the reserve
        pub c_factor: u32,         // the collateral factor for the reserve
        pub max_util: u32,         // the maximum utilization rate for the reserve
        pub last_time: u64,        // the last block the data was updated
        pub scalar: i128,          // scalar used for positions, b/d token supply, and credit
        pub d_rate: i128,          // the conversion rate from dToken to underlying (9 decimals)
        pub b_rate: i128,          // the conversion rate from bToken to underlying (9 decimals)
        pub ir_mod: i128,          // the interest rate curve modifier (9 decimals)
        pub b_supply: i128,        // the total supply of b tokens
        pub d_supply: i128,        // the total supply of d tokens
        pub backstop_credit: i128, // the total amount of underlying tokens owed to the backstop
        pub collateral_cap: i128, // the total amount of underlying tokens that can be used as collateral
        pub enabled: bool,        // is the reserve enabled
    }
    #[contract]
    pub struct MockPool;

    #[contractimpl]
    impl MockPool {
        pub fn set_b_rate(e: Env, b_rate: i128) {
            e.storage().instance().set(&BRATE, &b_rate);
        }

        /// Note: We're only interested in the `b_rate`
        pub fn get_reserve(e: Env, _reserve: Address) -> Reserve {
            Reserve {
                asset: e.current_contract_address(),
                index: 0,
                l_factor: 0,
                c_factor: 0,
                max_util: 0,
                last_time: 0,
                scalar: 0,
                d_rate: 0,
                b_rate: e.storage().instance().get(&BRATE).unwrap_or(1_000_000_000),
                ir_mod: 0,
                b_supply: 0,
                d_supply: 0,
                backstop_credit: 0,
                collateral_cap: 0,
                enabled: true,
            }
        }
    }

    pub fn register_mock_pool_with_b_rate(e: &Env, b_rate: i128) -> mockpool::MockPoolClient {
        let pool_address = e.register(mockpool::MockPool {}, ());
        let client = mockpool::MockPoolClient::new(e, &pool_address);
        client.set_b_rate(&b_rate);
        client
    }
}
