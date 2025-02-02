use crate::{
    constants::{SCALAR_7, SCALAR_9},
    errors::FeeVaultError,
    pool, storage,
    validator::require_positive,
};
use soroban_fixed_point_math::{i128, FixedPoint};
use soroban_sdk::{contracttype, panic_with_error, Address, Env};

#[contracttype]
pub struct ReserveVault {
    /// The reserve asset address
    pub address: Address,
    /// The reserve's last bRate
    pub b_rate: i128,
    /// The total shares issued by the reserve vault
    pub total_shares: i128,
    /// The total bToken deposits owned by the reserve vault depositors. Excludes accrued fees.
    pub total_b_tokens: i128,
    /// The number of bTokens the admin is due
    pub accrued_fees: i128,
}

impl ReserveVault {
    /// Converts a b_token amount to shares rounding down
    pub fn b_tokens_to_shares_down(&self, amount: i128) -> i128 {
        if self.total_shares == 0 || self.total_b_tokens == 0 {
            return amount;
        }
        amount
            .fixed_mul_floor(self.total_shares, self.total_b_tokens)
            .unwrap()
    }

    /// Converts a b_token amount to shares rounding up
    pub fn b_tokens_to_shares_up(&self, amount: i128) -> i128 {
        if self.total_shares == 0 || self.total_b_tokens == 0 {
            return amount;
        }
        amount
            .fixed_mul_ceil(self.total_shares, self.total_b_tokens)
            .unwrap()
    }

    /// Coverts a share amount to a b_token amount rounding down
    pub fn shares_to_b_tokens_down(&self, amount: i128) -> i128 {
        amount
            .fixed_div_floor(self.total_shares, self.total_b_tokens)
            .unwrap()
    }

    /// Coverts a b_token amount to an underlying token amount rounding down
    ///
    /// ### Note
    /// This function performs the calculations based on the last observed b_rate.
    /// If `update_rate` hasn't been invoked in the same ledger, it may yield incorrect results.
    pub fn b_tokens_to_underlying_down(&self, amount: i128) -> i128 {
        amount.fixed_mul_floor(self.b_rate, SCALAR_9).unwrap()
    }

    /// Coverts a b_token amount to an underlying token amount rounding up
    ///
    /// ### Note
    /// This function performs the calculations based on the last observed b_rate.
    /// If `update_rate` hasn't been invoked in the same ledger, it may yield incorrect results.
    pub fn b_tokens_to_underlying_up(&self, amount: i128) -> i128 {
        amount.fixed_mul_ceil(self.b_rate, SCALAR_9).unwrap()
    }

    /// Coverts an underlying amount to a b_token amount rounding down
    ///
    /// ### Note
    /// This function performs the calculations based on the last observed b_rate.
    /// If `update_rate` hasn't been invoked in the same ledger, it may yield incorrect results.
    pub fn underlying_to_b_tokens_down(&self, amount: i128) -> i128 {
        amount.fixed_div_floor(self.b_rate, SCALAR_9).unwrap()
    }

    /// Coverts an underlying amount to a b_token amount rounding up
    ///
    /// ### Note
    /// This function performs the calculations based on the last observed b_rate.
    /// If `update_rate` hasn't been invoked in the same ledger, it may yield incorrect results.
    pub fn underlying_to_b_tokens_up(&self, amount: i128) -> i128 {
        amount.fixed_div_ceil(self.b_rate, SCALAR_9).unwrap()
    }

    /// Updates the reserve's bRate and accrues fees to the admin in accordance with the portion of interest they earned
    pub fn update_rate(&mut self, e: &Env) {
        let new_rate = pool::reserve_b_rate(e, &self.address);
        if new_rate == self.b_rate {
            return;
        }

        // Calculate the total accrued b_tokens - 7 decimal places of precision
        let admin_take_b_tokens = self
            .total_b_tokens
            .fixed_mul_floor(new_rate - self.b_rate, SCALAR_9)
            .unwrap()
            .fixed_mul_floor(storage::get_take_rate(e), SCALAR_7)
            .unwrap()
            .fixed_div_floor(new_rate, SCALAR_9)
            .unwrap();

        // Update the reserve's bRate
        self.b_rate = new_rate;

        // if no interest was accrued we do not accrue fees
        if admin_take_b_tokens <= 0 {
            return;
        }

        self.total_b_tokens = self.total_b_tokens - admin_take_b_tokens;
        self.accrued_fees = self.accrued_fees + admin_take_b_tokens;
    }
}

/// Deposit into the reserve vault. Does not perform the call to the pool to deposit the tokens.
///
/// ### Arguments
/// * `vault` - The reserve vault to deposit into
/// * `user` - The user that deposited the tokens
/// * `amount` - The amount of underlying deposited
///
/// ### Returns
/// * `(i128, i128)` - (The amount of b_tokens minted to the vault, the amount of shares minted to the user)
///
/// ### Panics
/// * If the underlying amount is less than or equal to 0
pub fn deposit(e: &Env, mut vault: ReserveVault, user: &Address, amount: i128) -> (i128, i128) {
    require_positive(e, amount, FeeVaultError::InvalidAmount);

    vault.update_rate(e);
    let b_tokens_amount = vault.underlying_to_b_tokens_down(amount);
    require_positive(e, b_tokens_amount, FeeVaultError::InvalidBTokensMinted);

    let mut user_shares = storage::get_reserve_vault_shares(e, &vault.address, user);
    let share_amount = vault.b_tokens_to_shares_down(b_tokens_amount);
    require_positive(e, share_amount, FeeVaultError::InvalidSharesMinted);

    vault.total_shares += share_amount;
    vault.total_b_tokens += b_tokens_amount;
    user_shares += share_amount;
    storage::set_reserve_vault(e, &vault.address, &vault);
    storage::set_reserve_vault_shares(e, &vault.address, user, user_shares);
    (b_tokens_amount, share_amount)
}

/// Withdraw from the reserve vault. Does not perform the call to the pool to withdraw the tokens.
///
/// ### Arguments
/// * `vault` - The reserve vault to deposit into
/// * `user` - The user withdrawing tokens
/// * `amount` - The amount of underlying amount withdrawn from the vault
///
/// ### Returns
/// * `(i128, i128)` - (The amount of b_tokens burned from the vault, the amount of shares burned from the user)
///
/// ### Panics
/// * If the amount is less than or equal to 0
/// * If the user does not have enough shares or bTokens to withdraw
pub fn withdraw(e: &Env, mut vault: ReserveVault, user: &Address, amount: i128) -> (i128, i128) {
    require_positive(e, amount, FeeVaultError::InvalidAmount);

    vault.update_rate(e);
    let b_tokens_amount = vault.underlying_to_b_tokens_up(amount);

    let mut user_shares = storage::get_reserve_vault_shares(e, &vault.address, user);
    let share_amount = vault.b_tokens_to_shares_up(b_tokens_amount);
    if vault.total_shares < share_amount || vault.total_b_tokens < b_tokens_amount {
        panic_with_error!(e, FeeVaultError::InsufficientReserves);
    }
    vault.total_shares -= share_amount;
    vault.total_b_tokens -= b_tokens_amount;
    if share_amount > user_shares {
        panic_with_error!(e, FeeVaultError::BalanceError);
    }
    user_shares -= share_amount;
    storage::set_reserve_vault(e, &vault.address, &vault);
    storage::set_reserve_vault_shares(e, &vault.address, user, user_shares);
    (b_tokens_amount, share_amount)
}

/// Claim fees from the reserve vault. Does not perform the call to the pool to claim the fees.
///
/// ### Arguments
/// * `vault` - The reserve vault to deposit into
///
/// ### Panics
/// * If the accrued bToken amount is less than or equal to 0
pub fn claim_fees(e: &Env, mut vault: ReserveVault) -> (i128, i128) {
    vault.update_rate(e);
    let b_tokens_amount = vault.accrued_fees;
    require_positive(e, b_tokens_amount, FeeVaultError::InvalidBTokensBurnt);

    let underlying_amount = vault.b_tokens_to_underlying_down(b_tokens_amount);
    vault.accrued_fees = 0;
    storage::set_reserve_vault(e, &vault.address, &vault);
    (b_tokens_amount, underlying_amount)
}

/// Note: Test suite is temporarily broken. Will be updated with full blend integration
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutils::register_fee_vault;
    use soroban_sdk::{testutils::Address as _, Address};

    #[test]
    fn test_b_tokens_to_shares_down() {
        let e = Env::default();
        let mut vault = ReserveVault {
            address: Address::generate(&e),
            b_rate: 1_000_000_000,
            total_shares: 0,
            total_b_tokens: 0,
            accrued_fees: 0,
        };

        // rounds down
        vault.total_shares = 200_0000001;
        vault.total_b_tokens = 100_0000000;
        let b_tokens = vault.b_tokens_to_shares_down(1_0000000);
        assert_eq!(b_tokens, 2_0000000);

        // returns amount if total_shares is 0
        vault.total_shares = 0;
        vault.total_b_tokens = 100_0000000;
        let b_tokens = vault.b_tokens_to_shares_down(1_0000000);
        assert_eq!(b_tokens, 1_0000000);

        // returns amount if total_b_tokens is 0
        vault.total_shares = 200_0000000;
        vault.total_b_tokens = 0;
        let b_tokens = vault.b_tokens_to_shares_down(1_0000000);
        assert_eq!(b_tokens, 1_0000000);
    }

    #[test]
    fn test_b_tokens_to_shares_up() {
        let e = Env::default();
        let mut vault = ReserveVault {
            address: Address::generate(&e),
            b_rate: 1_000_000_000,
            total_shares: 0,
            total_b_tokens: 0,
            accrued_fees: 0,
        };

        // rounds up
        vault.total_shares = 200_0000001;
        vault.total_b_tokens = 100_0000000;
        let b_tokens = vault.b_tokens_to_shares_up(1_0000000);
        assert_eq!(b_tokens, 2_0000001);

        // returns amount if total_shares is 0
        vault.total_shares = 0;
        vault.total_b_tokens = 100_0000000;
        let b_tokens = vault.b_tokens_to_shares_up(1_0000000);
        assert_eq!(b_tokens, 1_0000000);

        // returns amount if total_b_tokens is 0
        vault.total_shares = 200_0000000;
        vault.total_b_tokens = 0;
        let b_tokens = vault.b_tokens_to_shares_up(1_0000000);
        assert_eq!(b_tokens, 1_0000000);
    }

    #[test]
    fn test_shares_to_b_tokens_down() {
        let e = Env::default();
        let mut vault = ReserveVault {
            address: Address::generate(&e),
            b_rate: 1_000_000_000,
            total_shares: 0,
            total_b_tokens: 0,
            accrued_fees: 0,
        };

        // rounds down
        vault.total_shares = 200_0000001;
        vault.total_b_tokens = 100_0000000;
        let b_tokens = vault.shares_to_b_tokens_down(2_0000000);
        assert_eq!(b_tokens, 0_9999999);

        // returns 0 if total_b_tokens is 0
        vault.total_shares = 200_0000000;
        vault.total_b_tokens = 0;
        let b_tokens = vault.shares_to_b_tokens_down(2_0000000);
        assert_eq!(b_tokens, 0);
    }

    #[test]
    fn test_deposit() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a deposit for samwise
            let new_b_rate = 1_110_000_000;
            let b_tokens = 83_3333300;
            let expected_b_token_fees = 0_9009009;
            let expected_share_amount = 100_0901673;
            deposit(&e, reserve_vault, &samwise, b_tokens);

            // Load the updated reserve to verify the changes
            let new_vault = storage::get_reserve_vault(&e, &reserve);
            assert_eq!(new_vault.total_shares, 1200_0000000 + expected_share_amount);
            assert_eq!(
                new_vault.total_b_tokens,
                1000_0000000 + b_tokens - expected_b_token_fees
            );
            assert_eq!(new_vault.b_rate, new_b_rate);
            assert_eq!(new_vault.accrued_fees, expected_b_token_fees);

            let new_balance = storage::get_reserve_vault_shares(&e, &reserve, &samwise);
            assert_eq!(new_balance, expected_share_amount);
        });
    }

    #[test]
    fn test_initial_deposit() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 0,
                total_shares: 0,
                b_rate: 1_000_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a deposit for samwise
            let new_b_rate = 1_100_000_000;
            let b_tokens = 80_0000000;
            deposit(&e, reserve_vault, &samwise, b_tokens);

            // Load the updated reserve to verify the changes
            let expected_share_amount = b_tokens;
            let new_vault = storage::get_reserve_vault(&e, &reserve);
            assert_eq!(new_vault.total_shares, expected_share_amount);
            assert_eq!(new_vault.total_b_tokens, b_tokens);
            assert_eq!(new_vault.b_rate, new_b_rate);
            // no fees should accrue against 0 deposits
            assert_eq!(new_vault.accrued_fees, 0);

            let new_balance = storage::get_reserve_vault_shares(&e, &reserve, &samwise);
            assert_eq!(new_balance, expected_share_amount);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #106)")]
    fn test_deposit_zero_b_tokens() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            deposit(&e, reserve_vault, &samwise, 0);
        });
    }

    #[test]
    fn test_withdraw() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a withdraw for samwise
            let new_b_rate = 1_110_000_000;
            let b_tokens = 83_3333300;
            let expected_share_amount = 100_0901674;
            let expected_b_token_fees = 0_9009009;
            storage::set_reserve_vault_shares(&e, &reserve, &samwise, expected_share_amount);
            withdraw(&e, reserve_vault, &samwise, b_tokens);

            // Load the updated reserve to verify the changes
            let new_vault = storage::get_reserve_vault(&e, &reserve);
            assert_eq!(new_vault.total_shares, 1200_0000000 - expected_share_amount);
            assert_eq!(
                new_vault.total_b_tokens,
                1000_0000000 - b_tokens - expected_b_token_fees
            );
            assert_eq!(new_vault.b_rate, new_b_rate);
            assert_eq!(new_vault.accrued_fees, expected_b_token_fees);

            let new_balance = storage::get_reserve_vault_shares(&e, &reserve, &samwise);
            assert_eq!(new_balance, 0);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #107)")]
    fn test_withdraw_zero_b_tokens() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            withdraw(&e, reserve_vault, &samwise, 0, 1_100_000_000);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #105)")]
    fn test_withdraw_more_b_tokens_than_vault() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a withdraw for samwise
            let b_tokens = reserve_vault.total_b_tokens + 1;

            storage::set_reserve_vault_shares(&e, &reserve, &samwise, reserve_vault.total_shares);
            withdraw(&e, reserve_vault, &samwise, b_tokens, 1_100_000_000);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #10)")]
    fn test_withraw_over_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a withdraw for samwise
            let new_b_rate = 1_110_000_000;
            let b_tokens = 83_3333300;
            let expected_share_amount = 100_0901674;
            storage::set_reserve_vault_shares(&e, &reserve, &samwise, expected_share_amount - 1);
            withdraw(&e, reserve_vault, &samwise, b_tokens);
        });
    }

    #[test]
    fn test_claim_fees() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let starting_fees = 5_0000000;
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: starting_fees,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a deposit for samwise
            let new_b_rate = 1_110_000_000;
            let expected_b_token_fees = 0_9009009;
            let b_tokens = 5_5000000;
            claim_fees(&e, reserve_vault);

            // Load the updated reserve to verify the changes
            let new_vault = storage::get_reserve_vault(&e, &reserve);
            assert_eq!(new_vault.total_shares, 1200_0000000);
            assert_eq!(
                new_vault.total_b_tokens,
                1000_0000000 - expected_b_token_fees
            );
            assert_eq!(new_vault.b_rate, new_b_rate);
            assert_eq!(
                new_vault.accrued_fees,
                starting_fees + expected_b_token_fees - b_tokens
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #107)")]
    fn test_claim_fees_zero_b_tokens() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 5_0000000,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            claim_fees(&e, reserve_vault);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #103)")]
    fn test_claim_fees_b_tokens_more_than_accrued() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 5_0000000,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a deposit for samwise
            let new_b_rate = 1_110_000_000;
            let expected_b_token_fees = 0_9009009;
            let b_tokens = 5_0000000 + expected_b_token_fees + 1;
            claim_fees(&e, reserve_vault);
        });
    }
}
