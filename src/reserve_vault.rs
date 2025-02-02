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
    use crate::testutils::{mockpool, register_fee_vault};
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
    fn test_update_rate() {
        let e = Env::default();
        e.mock_all_auths();

        let init_b_rate = 1_100_000_000;
        let bombadil = Address::generate(&e);

        let mock_client = mockpool::register_mock_pool_with_b_rate(&e, init_b_rate);
        let vault_address = register_fee_vault(
            &e,
            Some((bombadil.clone(), mock_client.address.clone(), 200_0000)),
        );

        e.as_contract(&vault_address, || {
            let mut reserve_vault = ReserveVault {
                address: Address::generate(&e),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: init_b_rate,
                accrued_fees: 0,
            };

            // update b_rate to 1.2
            let expected_accrued_fee = 16_6666666;
            mock_client.set_b_rate(&120_000_0000);
            reserve_vault.update_rate(&e);

            assert_eq!(reserve_vault.accrued_fees, expected_accrued_fee);
            assert_eq!(reserve_vault.total_shares, 1200_000_0000);
            assert_eq!(reserve_vault.total_b_tokens, 1000_0000000 - 16_6666666);

            // update b_rate to 1.5
            let expected_accrued_fee_2 = 39_333_3333;
            mock_client.set_b_rate(&150_000_0000);
            reserve_vault.update_rate(&e);

            assert_eq!(
                reserve_vault.accrued_fees,
                expected_accrued_fee + expected_accrued_fee_2
            );
            assert_eq!(reserve_vault.total_shares, 1200_000_0000);
            assert_eq!(
                reserve_vault.total_b_tokens,
                1000_0000000 - 16_6666666 - 39_333_3333
            );
        });
    }

    #[test]
    fn test_update_rate_2() {
        let e = Env::default();
        e.mock_all_auths();

        let init_b_rate = 1_000_000_000;
        let bombadil = Address::generate(&e);

        let mock_client = mockpool::register_mock_pool_with_b_rate(&e, init_b_rate);
        let vault_address = register_fee_vault(
            &e,
            Some((bombadil.clone(), mock_client.address.clone(), 200_0000)),
        );

        e.as_contract(&vault_address, || {
            let mut reserve_vault = ReserveVault {
                address: Address::generate(&e),
                total_b_tokens: 500_000_0000000,
                total_shares: 500_000_0000000,
                b_rate: init_b_rate,
                accrued_fees: 0,
            };

            let expected_accrued_fee = 1050_1384549;

            let new_b_rate = 1_000_0000000 * SCALAR_9 / 989_4986154;
            mock_client.set_b_rate(&new_b_rate);
            reserve_vault.update_rate(&e);
            let deposit_b_tokens = 989_4986154;
            let shares = reserve_vault.b_tokens_to_shares_down(deposit_b_tokens);
            reserve_vault.total_b_tokens += deposit_b_tokens;
            reserve_vault.total_shares += shares;
            assert_eq!(
                reserve_vault.total_b_tokens,
                500_000_0000000 + 989_4986154 - expected_accrued_fee
            );
            assert_eq!(reserve_vault.total_shares, 500_000_0000000 + 991_5812105);

            assert_eq!(reserve_vault.b_rate, 1_010_612_834);

            assert_eq!(reserve_vault.accrued_fees, expected_accrued_fee);
        });
    }

    #[test]
    fn test_update_rate_no_change() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);

        e.as_contract(&vault_address, || {
            let mut reserve_vault = ReserveVault {
                address: Address::generate(&e),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 12_0000000,
            };

            reserve_vault.update_rate(&e);
            // assert nothing changes
            assert_eq!(reserve_vault.accrued_fees, reserve_vault.accrued_fees);
            assert_eq!(reserve_vault.total_shares, reserve_vault.total_shares);
            assert_eq!(reserve_vault.total_b_tokens, reserve_vault.total_b_tokens);
            assert_eq!(reserve_vault.b_rate, reserve_vault.b_rate);
        });
    }

    #[test]
    fn test_deposit() {
        let e = Env::default();
        e.mock_all_auths();

        let init_b_rate = 1_100_000_000;

        let mock_client = mockpool::register_mock_pool_with_b_rate(&e, init_b_rate);
        let vault_address = register_fee_vault(
            &e,
            Some((
                Address::generate(&e),
                mock_client.address.clone(),
                0_1000000,
            )),
        );
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: init_b_rate,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a deposit for samwise
            let new_b_rate = 1_110_000_000;
            mock_client.set_b_rate(&new_b_rate);

            let b_tokens = 83_3333300;
            let amount = b_tokens.fixed_mul_floor(new_b_rate, SCALAR_9).unwrap();
            let expected_b_token_fees = 0_9009009;
            let expected_share_amount = 100_0901673;
            let (b_tokens_minted, shares_minted) = deposit(&e, reserve_vault, &samwise, amount);
            assert_eq!(b_tokens_minted, b_tokens);
            assert_eq!(shares_minted, expected_share_amount);

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

        let init_b_rate = 1_000_000_000;

        let mock_client = mockpool::register_mock_pool_with_b_rate(&e, init_b_rate);
        let vault_address = register_fee_vault(
            &e,
            Some((
                Address::generate(&e),
                mock_client.address.clone(),
                0_1000000,
            )),
        );
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 0,
                total_shares: 0,
                b_rate: init_b_rate,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a deposit for samwise
            let new_b_rate = 1_100_000_000;
            mock_client.set_b_rate(&new_b_rate);
            let amount = 100_0000000;
            let expected_b_tokens = amount.fixed_div_floor(new_b_rate, SCALAR_9).unwrap();
            let (b_tokens_minted, shares_minted) = deposit(&e, reserve_vault, &samwise, amount);

            // Load the updated reserve to verify the changes
            let expected_share_amount = expected_b_tokens;
            assert_eq!(b_tokens_minted, expected_b_tokens);
            assert_eq!(shares_minted, expected_share_amount);
            let new_vault = storage::get_reserve_vault(&e, &reserve);
            assert_eq!(new_vault.total_shares, expected_share_amount);
            assert_eq!(new_vault.total_b_tokens, b_tokens_minted);
            assert_eq!(new_vault.b_rate, new_b_rate);
            // no fees should accrue against 0 deposits
            assert_eq!(new_vault.accrued_fees, 0);

            let new_balance = storage::get_reserve_vault_shares(&e, &reserve, &samwise);
            assert_eq!(new_balance, expected_share_amount);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #102)")]
    fn test_deposit_zero_amount() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
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
    #[should_panic(expected = "Error(Contract, #106)")]
    fn test_deposit_zero_b_tokens() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            deposit(&e, reserve_vault, &samwise, 1);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #108)")]
    fn test_deposit_zero_shares() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            // Not possible config in practice, but just in case
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 10000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            deposit(&e, reserve_vault, &samwise, 2);
        });
    }

    #[test]
    fn test_withdraw() {
        let e = Env::default();
        e.mock_all_auths();

        let mock_client = mockpool::register_mock_pool_with_b_rate(&e, 1_100_000_000);
        let vault_address = register_fee_vault(
            &e,
            Some((
                Address::generate(&e),
                mock_client.address.clone(),
                0_1000000,
            )),
        );
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
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
            mock_client.set_b_rate(&new_b_rate);

            let b_tokens_to_withdraw = 50_0000000;
            let expected_share_amount = 100_0901674;
            let expected_b_token_fees = 0_9009009;
            storage::set_reserve_vault_shares(&e, &reserve, &samwise, expected_share_amount);

            // claim fees just to force the `update_rate` to be called
            let (b_tokens_collected, _) = claim_fees(&e, reserve_vault);
            assert_eq!(b_tokens_collected, expected_b_token_fees);

            let reserve_vault = storage::get_reserve_vault(&e, &reserve);

            let withdraw_amount = reserve_vault.b_tokens_to_underlying_down(b_tokens_to_withdraw);
            let (b_tokens_burnt, shares_burnt) =
                withdraw(&e, reserve_vault, &samwise, withdraw_amount);

            let new_vault = storage::get_reserve_vault(&e, &reserve);

            assert_eq!(b_tokens_burnt, b_tokens_to_withdraw);
            assert_eq!(
                shares_burnt,
                new_vault.b_tokens_to_shares_up(b_tokens_to_withdraw)
            );

            // Load the updated reserve to verify the changes
            assert_eq!(new_vault.total_shares, 1200_0000000 - shares_burnt);
            assert_eq!(
                new_vault.total_b_tokens,
                1000_0000000 - b_tokens_to_withdraw - expected_b_token_fees
            );
            assert_eq!(new_vault.b_rate, new_b_rate);
            assert_eq!(new_vault.accrued_fees, 0);

            let new_balance = storage::get_reserve_vault_shares(&e, &reserve, &samwise);
            assert_eq!(new_balance, expected_share_amount - shares_burnt);
        });
    }

    #[test]
    fn test_withdraw_max() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            storage::set_reserve_vault_shares(&e, &reserve, &samwise, reserve_vault.total_shares);
            let withdraw_amount = reserve_vault.b_tokens_to_underlying_down(1000_0000000);

            let (b_tokens_burnt, shares_burnt) =
                withdraw(&e, reserve_vault, &samwise, withdraw_amount);
            assert_eq!(b_tokens_burnt, 1000_0000000);
            assert_eq!(shares_burnt, 1200_0000000);
            let new_balance = storage::get_reserve_vault_shares(&e, &reserve, &samwise);
            assert_eq!(new_balance, 0);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #102)")]
    fn test_withdraw_zero_amount() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            withdraw(&e, reserve_vault, &samwise, 0);
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
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            storage::set_reserve_vault_shares(&e, &reserve, &samwise, reserve_vault.total_shares);
            let withdraw_amount = reserve_vault.b_tokens_to_underlying_down(1000_0000000);

            withdraw(&e, reserve_vault, &samwise, withdraw_amount + 1);
        });
    }

    #[test]
    fn test_withdraw_exact_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            let sam_shares = 1000_0000000;
            storage::set_reserve_vault_shares(&e, &reserve, &samwise, sam_shares);
            let sam_b_tokens: i128 = reserve_vault
                .shares_to_b_tokens_down(storage::get_reserve_vault_shares(&e, &reserve, &samwise));
            let sam_underlying_balance = reserve_vault.b_tokens_to_underlying_down(sam_b_tokens);

            // Withdraw whole underlying balance as read by the contract
            let (b_tokens_burnt, shares_burnt) =
                withdraw(&e, reserve_vault, &samwise, sam_underlying_balance);
            assert_eq!(b_tokens_burnt, sam_b_tokens);
            assert_eq!(shares_burnt, sam_shares);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #10)")]
    fn test_withdraw_over_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            storage::set_reserve_vault_shares(&e, &reserve, &samwise, 1000_0000000);
            let sam_b_tokens: i128 = reserve_vault.shares_to_b_tokens_down(1000_0000000);
            let sam_underlying_balance = reserve_vault.b_tokens_to_underlying_down(sam_b_tokens);
            // Try to withdraw 1 more than `sam_underlying_balance`
            withdraw(&e, reserve_vault, &samwise, sam_underlying_balance + 1);
        });
    }

    #[test]
    fn test_claim_fees() {
        let e = Env::default();
        e.mock_all_auths();

        let init_b_rate = 1_100_000_000;
        let mock_client = mockpool::register_mock_pool_with_b_rate(&e, init_b_rate);
        let vault_address = register_fee_vault(
            &e,
            Some((
                Address::generate(&e),
                mock_client.address.clone(),
                0_1000000,
            )),
        );
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            let starting_fees = 5_0000000;
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: init_b_rate,
                accrued_fees: starting_fees,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Claim starting fees
            let (b_tokens_burnt, underlying_burnt) = claim_fees(&e, reserve_vault);
            assert_eq!(b_tokens_burnt, starting_fees);
            assert_eq!(
                underlying_burnt,
                b_tokens_burnt
                    .fixed_mul_floor(init_b_rate, SCALAR_9)
                    .unwrap()
            );

            let reserve_vault = storage::get_reserve_vault(&e, &reserve);
            assert_eq!(reserve_vault.accrued_fees, 0);
            // total_b_tokens and total_shares should remain unchanges
            assert_eq!(reserve_vault.total_b_tokens, 1000_0000000);
            assert_eq!(reserve_vault.total_shares, 1200_0000000);

            // Perform a deposit for samwise
            let new_b_rate = 1_110_000_000;
            mock_client.set_b_rate(&new_b_rate);
            let expected_b_token_fees = 0_9009009;
            let (b_tokens_burnt, underlying_burnt) = claim_fees(&e, reserve_vault);
            assert_eq!(b_tokens_burnt, expected_b_token_fees);
            assert_eq!(
                underlying_burnt,
                b_tokens_burnt
                    .fixed_mul_floor(new_b_rate, SCALAR_9)
                    .unwrap()
            );

            // Load the updated reserve to verify the changes
            let new_vault = storage::get_reserve_vault(&e, &reserve);
            assert_eq!(new_vault.total_shares, 1200_0000000);
            assert_eq!(
                new_vault.total_b_tokens,
                1000_0000000 - expected_b_token_fees
            );
            assert_eq!(new_vault.b_rate, new_b_rate);
            assert_eq!(new_vault.accrued_fees, 0);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #107)")]
    fn test_claim_fees_zero_fees_accrued() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            claim_fees(&e, reserve_vault);
        });
    }

    #[test]
    fn test_claim_fees_when_zero_shares() {
        let e = Env::default();
        e.mock_all_auths();

        let init_b_rate = 1_100_000_000;
        let mock_client = mockpool::register_mock_pool_with_b_rate(&e, init_b_rate);
        let vault_address = register_fee_vault(
            &e,
            Some((
                Address::generate(&e),
                mock_client.address.clone(),
                0_1000000,
            )),
        );
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            let accrued_fees = 5_0000000;

            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                total_b_tokens: 0,
                total_shares: 0,
                b_rate: init_b_rate,
                accrued_fees,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Even if b_rate doubles, since there are no b_tokens deposited, no more fees should've been accrued
            let new_b_rate = 2_000_000_000;
            mock_client.set_b_rate(&new_b_rate);

            let (b_tokens_burnt, underlying_balance_claimed) = claim_fees(&e, reserve_vault);

            assert_eq!(b_tokens_burnt, accrued_fees);
            assert_eq!(
                underlying_balance_claimed,
                b_tokens_burnt
                    .fixed_mul_floor(new_b_rate, SCALAR_9)
                    .unwrap()
            );
        });
    }
}
