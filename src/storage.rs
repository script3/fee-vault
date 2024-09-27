use soroban_sdk::{unwrap::UnwrapOptimized, Address, Env, Map, Symbol};

use crate::types::ReserveData;

//********** Storage Keys **********//

const POOL_KEY: &str = "Pool";
const ADMIN_KEY: &str = "Admin";
const IS_INIT_KEY: &str = "IsInit";
const TAKE_RATE_KEY: &str = "TakeRate";

//********** Storage Utils **********//

pub const ONE_DAY_LEDGERS: u32 = 17280; // assumes 5 seconds per ledger on average

const LEDGER_BUMP_SHARED: u32 = 31 * ONE_DAY_LEDGERS;
const LEDGER_THRESHOLD_SHARED: u32 = LEDGER_BUMP_SHARED - ONE_DAY_LEDGERS;

const LEDGER_BUMP_USER: u32 = 120 * ONE_DAY_LEDGERS;
const LEDGER_THRESHOLD_USER: u32 = LEDGER_BUMP_USER - 20 * ONE_DAY_LEDGERS;

/// Bump the instance lifetime by the defined amount
pub fn extend_instance(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
}

/********** Instance **********/

/// Check if the contract has been initialized
pub fn get_is_init(e: &Env) -> bool {
    e.storage().instance().has(&Symbol::new(e, IS_INIT_KEY))
}

/// Set the contract as initialized
pub fn set_is_init(e: &Env) {
    e.storage()
        .instance()
        .set::<Symbol, bool>(&Symbol::new(e, IS_INIT_KEY), &true);
}

/// Get the pool address
pub fn get_pool(e: &Env) -> Address {
    e.storage()
        .instance()
        .get::<Symbol, Address>(&Symbol::new(e, POOL_KEY))
        .unwrap_optimized()
}

/// Set the pool address
pub fn set_pool(e: &Env, pool: Address) {
    e.storage()
        .instance()
        .set::<Symbol, Address>(&Symbol::new(e, POOL_KEY), &pool);
}

/// Get the backstop token address
pub fn get_admin(e: &Env) -> Address {
    e.storage()
        .instance()
        .get::<Symbol, Address>(&Symbol::new(e, ADMIN_KEY))
        .unwrap_optimized()
}

/// Set the admin address
pub fn set_admin(e: &Env, admin: Address) {
    e.storage()
        .instance()
        .set::<Symbol, Address>(&Symbol::new(e, ADMIN_KEY), &admin);
}

/// Get the take rate for the fee vault
pub fn get_take_rate(e: &Env) -> i128 {
    e.storage()
        .instance()
        .get::<Symbol, i128>(&Symbol::new(e, TAKE_RATE_KEY))
        .unwrap_optimized()
}

/// Set the take rate for the fee vault
pub fn set_take_rate(e: &Env, take_rate: i128) {
    e.storage()
        .instance()
        .set::<Symbol, i128>(&Symbol::new(e, TAKE_RATE_KEY), &take_rate);
}

/********** Persistent **********/

/// Set a reserve's data
pub fn set_reserve_data(e: &Env, reserve_id: u32, data: ReserveData) {
    e.storage()
        .persistent()
        .set::<u32, ReserveData>(&reserve_id, &data);
    e.storage()
        .persistent()
        .extend_ttl(&reserve_id, LEDGER_THRESHOLD_USER, LEDGER_BUMP_USER);
}

/// Get a reserve's data
pub fn get_reserve_data(e: &Env, reserve_id: u32) -> Option<ReserveData> {
    let result = e
        .storage()
        .persistent()
        .get::<u32, ReserveData>(&reserve_id);
    if result.is_some() {
        e.storage()
            .persistent()
            .extend_ttl(&reserve_id, LEDGER_THRESHOLD_USER, LEDGER_BUMP_USER);
    }
    result
}

/// Get a user's deposits
/// deposit amount stored in shares, 7 decimal places of precision
pub fn get_user_deposits(e: &Env, user: &Address) -> Map<u32, i128> {
    let result = e
        .storage()
        .persistent()
        .get::<Address, Map<u32, i128>>(&user);
    if let Some(user_data) = result {
        e.storage()
            .persistent()
            .extend_ttl(user, LEDGER_THRESHOLD_USER, LEDGER_BUMP_USER);
        user_data
    } else {
        Map::new(e)
    }
}

/// Set a user's deposits
/// deposit amount stored in shares, 7 decimal places of precision
pub fn set_user_deposits(e: &Env, user: &Address, data: Map<u32, i128>) {
    e.storage()
        .persistent()
        .set::<Address, Map<u32, i128>>(user, &data);
    e.storage()
        .persistent()
        .extend_ttl(user, LEDGER_THRESHOLD_USER, LEDGER_BUMP_USER);
}
