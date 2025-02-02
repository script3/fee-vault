use crate::constants::{SCALAR_7, SCALAR_9};
use crate::{errors::FeeVaultError, storage};
use soroban_fixed_point_math::{i128, FixedPoint};
use soroban_sdk::{contracttype, panic_with_error, Address, Env};

#[contracttype]
pub struct ReserveVault {
    /// The reserve asset address
    pub address: Address,
    /// The reserve id in the pool
    pub reserve_id: u32,
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

    /// Updates the reserve's bRate and accrues fees to the admin in accordance with the portion of interest they earned
    fn update_rate(&mut self, e: &Env, underlying_amount: i128, b_tokens_amount: i128) {
        // Calculate the new bRate - 9 decimal places of precision
        let new_rate = underlying_amount
            .fixed_div_floor(b_tokens_amount, SCALAR_9)
            .unwrap();

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

/// Deposit into the reserve vault. This function expects the deposit to have already been made
/// into the pool, and accounts for the deposit in the reserve vault.
///
/// ### Arguments
/// * `vault` - The reserve vault to deposit into
/// * `user` - The user that deposited the tokens
/// * `underlying_amount` - The amount of underlying tokens deposited
/// * `b_tokens_amount` - The amount of bTokens minted to the vault
///
/// ### Returns
/// * `i128` - The amount of shares minted
///
/// ### Panics
/// * If the underlying amount is less than or equal to 0
/// * If the bTokens amount is less than or equal to 0
pub fn deposit(
    e: &Env,
    mut vault: ReserveVault,
    user: &Address,
    underlying_amount: i128,
    b_tokens_amount: i128,
) -> i128 {
    if underlying_amount <= 0 {
        panic_with_error!(e, FeeVaultError::InvalidAmount);
    }
    if b_tokens_amount <= 0 {
        panic_with_error!(e, FeeVaultError::InvalidBTokensMinted);
    }
    vault.update_rate(e, underlying_amount, b_tokens_amount);

    let mut user_shares = storage::get_reserve_vault_shares(e, &vault.address, user);
    let share_amount = vault.b_tokens_to_shares_down(b_tokens_amount);
    vault.total_shares += share_amount;
    vault.total_b_tokens += b_tokens_amount;
    user_shares += share_amount;
    storage::set_reserve_vault(e, &vault.address, &vault);
    storage::set_reserve_vault_shares(e, &vault.address, user, user_shares);
    share_amount
}

/// Withdraw from the reserve vault. This function expects the withdraw to have already been made
/// from the pool, and only accounts for the withdraw from the reserve vault.
///
/// ### Arguments
/// * `vault` - The reserve vault to deposit into
/// * `user` - The user withdrawing tokens
/// * `underlying_amount` - The amount of underlying tokens withdrawn
/// * `b_tokens_amount` - The amount of bTokens burnt from the vault
///
/// ### Returns
/// * `i128` - The amount of shares burnt
///
/// ### Panics
/// * If the underlying amount is less than or equal to 0
/// * If the bTokens amount is less than or equal to 0
/// * If the user does not have enough shares to withdraw
pub fn withdraw(
    e: &Env,
    mut vault: ReserveVault,
    user: &Address,
    underlying_amount: i128,
    b_tokens_amount: i128,
) -> i128 {
    if underlying_amount <= 0 {
        panic_with_error!(e, FeeVaultError::InvalidAmount);
    }
    if b_tokens_amount <= 0 {
        panic_with_error!(e, FeeVaultError::InvalidBTokensBurnt);
    }
    vault.update_rate(e, underlying_amount, b_tokens_amount);

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
    share_amount
}

/// Claim fees from the reserve vault. This function expects the withdraw to have already been made
/// from the pool, and only accounts for the claim from the reserve vault.
///
/// ### Arguments
/// * `vault` - The reserve vault to deposit into
/// * `underlying_amount` - The amount of underlying tokens withdrawn
/// * `b_tokens_amount` - The amount of bTokens burnt from the vault
///
/// ### Panics
/// * If the underlying amount is less than or equal to 0
/// * If the bTokens amount is less than or equal to 0
/// * If their are insufficient fees to claim
pub fn claim_fees(
    e: &Env,
    mut vault: ReserveVault,
    underlying_amount: i128,
    b_tokens_amount: i128,
) {
    if underlying_amount <= 0 {
        panic_with_error!(e, FeeVaultError::InvalidAmount);
    }
    if b_tokens_amount <= 0 {
        panic_with_error!(e, FeeVaultError::InvalidBTokensBurnt);
    }
    vault.update_rate(e, underlying_amount, b_tokens_amount);
    if b_tokens_amount > vault.accrued_fees {
        panic_with_error!(e, FeeVaultError::InsufficientAccruedFees);
    }
    vault.accrued_fees -= b_tokens_amount;
    storage::set_reserve_vault(e, &vault.address, &vault);
}

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
            reserve_id: 0,
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
            reserve_id: 0,
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
            reserve_id: 0,
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

        let vault_address = register_fee_vault(&e, None);

        e.as_contract(&vault_address, || {
            let bombadil = Address::generate(&e);
            storage::set_admin(&e, bombadil.clone());

            storage::set_take_rate(&e, 200_0000);

            let mut reserve_vault = ReserveVault {
                address: Address::generate(&e),
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };

            // update b_rate to 1.2
            let expected_accrued_fee = 16_6666666;
            reserve_vault.update_rate(&e, 120_000_0000, 100_000_0000);
            assert_eq!(reserve_vault.accrued_fees, expected_accrued_fee);
            assert_eq!(reserve_vault.total_shares, 1200_000_0000);
            assert_eq!(reserve_vault.total_b_tokens, 1000_0000000 - 16_6666666);

            // update b_rate to 1.5
            let expected_accrued_fee_2 = 39_333_3333;
            reserve_vault.update_rate(&e, 150_000_0000, 100_000_0000);
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

        let vault_address = register_fee_vault(&e, None);

        e.as_contract(&vault_address, || {
            let bombadil = Address::generate(&e);
            storage::set_admin(&e, bombadil.clone());

            storage::set_take_rate(&e, 200_0000);

            let mut reserve_vault = ReserveVault {
                address: Address::generate(&e),
                reserve_id: 0,
                total_b_tokens: 500_000_0000000,
                total_shares: 500_000_0000000,
                b_rate: 1_000_000_000,
                accrued_fees: 0,
            };

            let expected_accrued_fee = 1050_1384549;
            reserve_vault.update_rate(&e, 1_000_0000000, 989_4986154);
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
            let bombadil = Address::generate(&e);
            storage::set_admin(&e, bombadil.clone());
            storage::set_take_rate(&e, 0_1000000);

            let mut reserve_vault = ReserveVault {
                address: Address::generate(&e),
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 12_0000000,
            };

            let b_tokens = 100_0000000;
            let underlying = 110_0000000;
            reserve_vault.update_rate(&e, underlying, b_tokens);
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

        let vault_address = register_fee_vault(&e, None);
        let samwise = Address::generate(&e);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a deposit for samwise
            let new_b_rate = 1_110_000_000;
            let b_tokens = 83_3333300;
            let underlying = b_tokens.fixed_mul_floor(new_b_rate, SCALAR_9).unwrap();
            let expected_b_token_fees = 0_9009009;
            let expected_share_amount = 100_0901673;
            deposit(&e, reserve_vault, &samwise, underlying, b_tokens);

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
                reserve_id: 0,
                total_b_tokens: 0,
                total_shares: 0,
                b_rate: 1_000_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a deposit for samwise
            let new_b_rate = 1_100_000_000;
            let b_tokens = 80_0000000;
            let underlying = b_tokens.fixed_mul_floor(new_b_rate, SCALAR_9).unwrap();
            deposit(&e, reserve_vault, &samwise, underlying, b_tokens);

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
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            deposit(&e, reserve_vault, &samwise, 1, 0);
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
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            deposit(&e, reserve_vault, &samwise, 0, 1);
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
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a withdraw for samwise
            let new_b_rate = 1_110_000_000;
            let b_tokens = 83_3333300;
            let underlying = b_tokens.fixed_mul_floor(new_b_rate, SCALAR_9).unwrap();
            let expected_share_amount = 100_0901674;
            let expected_b_token_fees = 0_9009009;
            storage::set_reserve_vault_shares(&e, &reserve, &samwise, expected_share_amount);
            withdraw(&e, reserve_vault, &samwise, underlying, b_tokens);

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
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            withdraw(&e, reserve_vault, &samwise, 1, 0);
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
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            withdraw(&e, reserve_vault, &samwise, 0, 1);
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
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a withdraw for samwise
            let b_tokens = reserve_vault.total_b_tokens + 1;
            let underlying = b_tokens
                .fixed_mul_floor(reserve_vault.b_rate, SCALAR_9)
                .unwrap();
            storage::set_reserve_vault_shares(&e, &reserve, &samwise, reserve_vault.total_shares);
            withdraw(&e, reserve_vault, &samwise, underlying, b_tokens);
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
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            // Perform a withdraw for samwise
            let new_b_rate = 1_110_000_000;
            let b_tokens = 83_3333300;
            let underlying = b_tokens.fixed_mul_floor(new_b_rate, SCALAR_9).unwrap();
            let expected_share_amount = 100_0901674;
            storage::set_reserve_vault_shares(&e, &reserve, &samwise, expected_share_amount - 1);
            withdraw(&e, reserve_vault, &samwise, underlying, b_tokens);
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
                reserve_id: 0,
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
            let underlying = b_tokens.fixed_mul_floor(new_b_rate, SCALAR_9).unwrap();
            claim_fees(&e, reserve_vault, underlying, b_tokens);

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
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 5_0000000,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            claim_fees(&e, reserve_vault, 1, 0);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #102)")]
    fn test_claim_fees_zero_amount() {
        let e = Env::default();
        e.mock_all_auths();

        let vault_address = register_fee_vault(&e, None);
        let reserve = Address::generate(&e);

        e.as_contract(&vault_address, || {
            storage::set_take_rate(&e, 0_1000000);
            let reserve_vault = ReserveVault {
                address: reserve.clone(),
                reserve_id: 0,
                total_b_tokens: 1000_0000000,
                total_shares: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 5_0000000,
            };
            storage::set_reserve_vault(&e, &reserve, &reserve_vault);

            claim_fees(&e, reserve_vault, 0, 1);
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
                reserve_id: 0,
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
            let underlying = b_tokens.fixed_mul_floor(new_b_rate, SCALAR_9).unwrap();
            claim_fees(&e, reserve_vault, underlying, b_tokens);
        });
    }
}
