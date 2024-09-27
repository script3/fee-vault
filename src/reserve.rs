use crate::constants::{SCALAR_7, SCALAR_9};
use crate::types::ReserveData;
use crate::{errors::FeeVaultError, storage};
use soroban_fixed_point_math::{i128, FixedPoint};
use soroban_sdk::{contracttype, panic_with_error, Address, Env};

#[contracttype]
pub struct Reserve {
    pub id: u32, // The reserve's ID - should correspond to the index of the reserve in the pool
    pub address: Address, // The reserve's address
    pub b_rate: i128, // The reserve's bRate
    pub total_deposits: i128, // The total deposits associated with the reserve
    pub total_b_tokens: i128, // The total bToken deposits associated with the reserve
    pub accrued_fees: i128, // The number of bTokens the admin has accrues
}

impl Reserve {
    pub fn load(e: &Env, id: u32) -> Self {
        // Load the reserve data from storage
        let data = storage::get_reserve_data(e, id)
            .unwrap_or_else(|| panic_with_error!(e, FeeVaultError::ReserveNotFound));

        Self {
            id,
            address: data.address,
            b_rate: data.b_rate,
            total_deposits: data.total_deposits,
            total_b_tokens: data.total_b_tokens,
            accrued_fees: data.accrued_fees,
        }
    }

    pub fn store(&self, e: &Env) {
        // Store the reserve data in storage
        let data = ReserveData {
            address: self.address.clone(),
            b_rate: self.b_rate,
            total_deposits: self.total_deposits,
            total_b_tokens: self.total_b_tokens,
            accrued_fees: self.accrued_fees,
        };
        storage::set_reserve_data(e, self.id, data);
    }

    /// Updates the reserve's bRate and accrues fees to the admin in accordance with the portion of interest they earned
    pub fn update_rate(&mut self, e: &Env, underlying_amount: i128, b_tokens_amount: i128) {
        // Calculate the new bRate - 9 decimal places of precision
        let new_rate = underlying_amount
            .fixed_div_floor(b_tokens_amount, SCALAR_9)
            .unwrap();

        // Calculate the total accrued b_tokens - 7 decimal places of precision
        let admin_take_b_tokens = self
            .total_b_tokens
            .fixed_mul_floor(new_rate - self.b_rate, SCALAR_9)
            .unwrap()
            .fixed_mul_floor(storage::get_take_rate(e), SCALAR_7)
            .unwrap()
            .fixed_div_floor(new_rate, SCALAR_9)
            .unwrap();

        // if no interest was accrued we do not accrue fees
        if admin_take_b_tokens <= 0 {
            // Update the reserve's bRate
            self.b_rate = new_rate;
            return;
        }
        // Update the reserve's bRate
        self.b_rate = new_rate;

        self.total_b_tokens = self.total_b_tokens - admin_take_b_tokens;
        self.accrued_fees = self.accrued_fees + admin_take_b_tokens;
    }

    /// Deposits tokens into reserve
    pub fn deposit(&mut self, b_tokens_amount: i128) -> i128 {
        // Calculate the share amount
        let share_amount = if self.total_b_tokens == 0 || self.total_deposits == 0 {
            b_tokens_amount
        } else {
            // REVIEW: It would make more sense if this function (and the other one) took care of the case where
            // total_deposits === 0. Also - what are we doing about potential inflation attacks?
            // On first read it appears to be safe since we track both numbers manually, and there is no
            // donation function. However, it might be worth a callout via a comment at least if we are
            // relying on no ability to donate.
            self.b_tokens_to_shares_down(b_tokens_amount)
        };
        // Update the total deposits and bToken deposits
        self.total_deposits += share_amount;
        self.total_b_tokens += b_tokens_amount;
        share_amount
    }

    /// Withdraws tokens from the reserve
    pub fn withdraw(&mut self, e: &Env, b_tokens_amount: i128) -> i128 {
        let share_amount = if self.total_b_tokens == 0 || self.total_deposits == 0 {
            panic_with_error!(e, FeeVaultError::InsufficientReserves);
        } else {
            self.b_tokens_to_shares_up(b_tokens_amount)
        };

        if self.total_deposits < share_amount || self.total_b_tokens < b_tokens_amount {
            panic_with_error!(e, FeeVaultError::InsufficientReserves);
        }

        // Update the total deposits and bToken deposits
        self.total_deposits -= share_amount;
        self.total_b_tokens -= b_tokens_amount;
        share_amount
    }

    /// Converts a b_token amount to shares rounding down
    pub fn b_tokens_to_shares_down(&self, amount: i128) -> i128 {
        if self.total_deposits == 0 {
            return amount;
        }
        amount
            .fixed_mul_floor(self.total_deposits, self.total_b_tokens)
            .unwrap()
    }

    /// Converts a b_token amount to shares rounding up
    pub fn b_tokens_to_shares_up(&self, amount: i128) -> i128 {
        if self.total_deposits == 0 {
            return amount;
        }
        amount
            .fixed_mul_ceil(self.total_deposits, self.total_b_tokens)
            .unwrap()
    }

    /// Coverts a share amount to a b_token amount rounding down
    pub fn shares_to_b_tokens_down(&self, amount: i128) -> i128 {
        amount
            .fixed_div_floor(self.total_deposits, self.total_b_tokens)
            .unwrap()
    }
}

#[cfg(test)]
mod tests {

    use soroban_sdk::{testutils::Address as _, Address};

    use crate::testutils::register_fee_vault;

    use super::*;

    #[test]
    fn test_deposit() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let reserve_data = ReserveData {
                address: Address::generate(&e),
                total_b_tokens: 1000_0000000,
                total_deposits: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // Perform a deposit for samwise
            reserve.deposit(83_3333300);
            reserve.store(&e);

            // Load the updated reserve to verify the changes
            let expected_share_amount = 99_9999960;
            let updated_reserve = Reserve::load(&e, 0);
            let updated_total_deposits = updated_reserve.total_deposits;
            let updated_total_b_tokens = updated_reserve.total_b_tokens;

            // Assertions
            assert_eq!(updated_total_deposits, 1200_0000000 + expected_share_amount);
            assert_eq!(updated_total_b_tokens, 1000_0000000 + 83_3333300);
        });
    }

    #[test]
    fn test_initial_deposit() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let reserve_data = ReserveData {
                address: Address::generate(&e),
                total_b_tokens: 0,
                total_deposits: 0,
                b_rate: 1_000_000_000,
                accrued_fees: 0,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // Perform a deposit for samwise
            reserve.deposit(80_000_0000);
            reserve.store(&e);

            // Load the updated reserve to verify the changes
            let expected_share_amount = 80_000_0000;
            let updated_reserve = Reserve::load(&e, 0);
            let updated_total_deposits = updated_reserve.total_deposits;
            let updated_total_b_tokens = updated_reserve.total_b_tokens;

            // Assertions
            assert_eq!(updated_total_deposits, expected_share_amount);
            assert_eq!(updated_total_b_tokens, 80_000_0000);
        });
    }

    #[test]
    fn test_withdraw() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let reserve_data = ReserveData {
                address: Address::generate(&e),
                total_b_tokens: 2000_000_0000,
                total_deposits: 2200_000_0000,
                b_rate: 1_200_000_000,
                accrued_fees: 0,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // Perform a withdrawal for samwise
            reserve.withdraw(&e, 80_000_0000);
            reserve.store(&e);

            // Load the updated reserve to verify the changes
            let expected_share_amount = 88_000_0000;
            let updated_reserve = Reserve::load(&e, 0);
            let updated_total_deposits = updated_reserve.total_deposits;
            let updated_total_b_tokens = updated_reserve.total_b_tokens;

            // Assertions
            assert_eq!(
                updated_total_deposits,
                2200_000_0000 - expected_share_amount
            );
            assert_eq!(updated_total_b_tokens, 2000_000_0000 - 80_000_0000);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #105)")]
    fn test_over_withdraw() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let reserve_data = ReserveData {
                address: Address::generate(&e),
                total_b_tokens: 2_000_0000,
                total_deposits: 2_200_0000,
                b_rate: 1_200_000_000,
                accrued_fees: 0,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // Perform a withdrawal for samwise
            reserve.withdraw(&e, 200_000_0000);
        });
    }

    #[test]
    fn test_update_rate() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let bombadil = Address::generate(&e);
            storage::set_admin(&e, bombadil.clone());

            storage::set_take_rate(&e, 200_0000);

            let reserve_data = ReserveData {
                address: Address::generate(&e),
                total_b_tokens: 1000_0000000,
                total_deposits: 1200_0000000,
                b_rate: 1_100_000_000,
                accrued_fees: 0,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // update b_rate to 1.2
            let expected_accrued_fee = 16_6666666;
            reserve.update_rate(&e, 120_000_0000, 100_000_0000);
            reserve.store(&e);
            assert_eq!(reserve.accrued_fees, expected_accrued_fee);
            assert_eq!(reserve.total_deposits, 1200_000_0000);
            assert_eq!(reserve.total_b_tokens, 1000_0000000 - 16_6666666);

            // update b_rate to 1.5
            let expected_accrued_fee_2 = 39_333_3333;
            reserve.update_rate(&e, 150_000_0000, 100_000_0000);
            reserve.store(&e);
            assert_eq!(
                reserve.accrued_fees,
                expected_accrued_fee + expected_accrued_fee_2
            );
            assert_eq!(reserve.total_deposits, 1200_000_0000);
            assert_eq!(
                reserve.total_b_tokens,
                1000_0000000 - 16_6666666 - 39_333_3333
            );
        });
    }
    #[test]
    fn test_update_rate_2() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let bombadil = Address::generate(&e);
            storage::set_admin(&e, bombadil.clone());

            storage::set_take_rate(&e, 200_0000);

            let reserve_data = ReserveData {
                address: Address::generate(&e),
                total_b_tokens: 500_000_0000000,
                total_deposits: 500_000_0000000,
                b_rate: 1_000_000_000,
                accrued_fees: 0,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            let expected_accrued_fee = 1050_1384549;
            reserve.update_rate(&e, 1_000_0000000, 989_4986154);
            reserve.deposit(989_4986154);
            reserve.store(&e);
            assert_eq!(
                reserve.total_b_tokens,
                500_000_0000000 + 989_4986154 - expected_accrued_fee
            );
            assert_eq!(reserve.total_deposits, 500_000_0000000 + 991_5812105);

            assert_eq!(reserve.b_rate, 1_010_612_834);

            assert_eq!(reserve.accrued_fees, expected_accrued_fee);
        });
    }
}
