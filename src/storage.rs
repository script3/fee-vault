use soroban_sdk::{contracttype, panic_with_error, unwrap::UnwrapOptimized, Address, Env, Symbol};

use crate::{errors::FeeVaultError, reserve_vault::ReserveVault};

//********** Storage Keys **********//

const POOL_KEY: &str = "Pool";
const ADMIN_KEY: &str = "Admin";
const TAKE_RATE_KEY: &str = "TakeRate";

#[derive(Clone)]
#[contracttype]
pub struct DepositKey {
    reserve: Address, // the reserve asset address
    user: Address,    // the user who owns the deposit
}

#[derive(Clone)]
#[contracttype]
pub enum FeeVaultDataKey {
    Deposit(DepositKey),
    ResVault(Address),
}

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

/// Set a reserve's vault data
///
/// ### Arguments
/// * `reserve` - The address of the reserve asset
/// * `vault` - The reserve vault data
pub fn set_reserve_vault(e: &Env, reserve: &Address, vault: &ReserveVault) {
    let key = FeeVaultDataKey::ResVault(reserve.clone());
    e.storage()
        .persistent()
        .set::<FeeVaultDataKey, ReserveVault>(&key, vault);
    e.storage()
        .persistent()
        .extend_ttl(&key, LEDGER_THRESHOLD_USER, LEDGER_BUMP_USER);
}

/// Get a reserve's vault data
///
/// ### Arguments
/// * `reserve` - The address of the reserve asset
pub fn get_reserve_vault(e: &Env, reserve: &Address) -> ReserveVault {
    let key = FeeVaultDataKey::ResVault(reserve.clone());
    let result = e
        .storage()
        .persistent()
        .get::<FeeVaultDataKey, ReserveVault>(&key);
    match result {
        Some(reserve_data) => {
            e.storage()
                .persistent()
                .extend_ttl(&key, LEDGER_THRESHOLD_USER, LEDGER_BUMP_USER);
            reserve_data
        }
        None => panic_with_error!(e, FeeVaultError::ReserveNotFound),
    }
}

/// Check if a reserve has a vault
///
/// ### Arguments
/// * `reserve` - The address of the reserve asset
pub fn has_reserve_vault(e: &Env, reserve: &Address) -> bool {
    let key = FeeVaultDataKey::ResVault(reserve.clone());
    e.storage().persistent().has(&key)
}

/// Get the number of vault shares a user owns. Shares are stored with 7 decimal places of precision.
///
/// ### Arguments
/// * `reserve` - The address of the reserve asset
/// * `user` - The address of the user
pub fn get_reserve_vault_shares(e: &Env, reserve: &Address, user: &Address) -> i128 {
    let key = FeeVaultDataKey::Deposit(DepositKey {
        reserve: reserve.clone(),
        user: user.clone(),
    });
    let result = e.storage().persistent().get::<FeeVaultDataKey, i128>(&key);
    match result {
        Some(shares) => {
            e.storage()
                .persistent()
                .extend_ttl(&key, LEDGER_THRESHOLD_USER, LEDGER_BUMP_USER);
            shares
        }
        None => 0,
    }
}

/// Set the number of vault shares a user owns. Shares are stored with 7 decimal places of precision.
///
/// ### Arguments
/// * `reserve` - The address of the reserve asset
/// * `user` - The address of the user
/// * `shares` - The number of shares the user owns
pub fn set_reserve_vault_shares(e: &Env, reserve: &Address, user: &Address, shares: i128) {
    let key = FeeVaultDataKey::Deposit(DepositKey {
        reserve: reserve.clone(),
        user: user.clone(),
    });
    e.storage()
        .persistent()
        .set::<FeeVaultDataKey, i128>(&key, &shares);
    e.storage()
        .persistent()
        .extend_ttl(&key, LEDGER_THRESHOLD_USER, LEDGER_BUMP_USER);
}
