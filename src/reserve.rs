use crate::constants::{SCALAR_7, SCALAR_9};
use crate::types::ReserveData;
use crate::{errors::FeeVaultError, storage};
use soroban_fixed_point_math::{i128, FixedPoint};
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Map};

#[contracttype]
pub struct Reserve {
    pub id: u32, // The reserve's ID - should correspond to the index of the reserve in the pool
    pub address: Address, // The reserve's address
    pub b_rate: i128, // The reserve's bRate
    pub total_deposits: i128, // The total deposits associated with the reserve
    pub total_b_tokens: i128, // The total bToken deposits associated with the reserve
    pub deposits: Map<Address, i128>, // The user deposits associated with the reserve
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
            deposits: data.deposits,
        }
    }

    pub fn store(&self, e: &Env) {
        // Store the reserve data in storage
        let data = ReserveData {
            address: self.address.clone(),
            b_rate: self.b_rate,
            total_deposits: self.total_deposits,
            total_b_tokens: self.total_b_tokens,
            deposits: self.deposits.clone(),
        };
        storage::set_reserve_data(e, self.id, data);
    }

    /// Updates the reserve's bRate and mints a deposit to the admin in accordance with the portion of interest they earned
    pub fn update_rate(&mut self, e: &Env, underlying_amount: i128, b_tokens_amount: i128) {
        // Calculate the new bRate - 9 decimal places of precision
        let new_rate = underlying_amount
            .fixed_div_floor(b_tokens_amount, SCALAR_9)
            .unwrap();
        // Calculate the total accrued interest - 7 decimal places of precision
        let accrued_interest = self
            .total_b_tokens
            .fixed_mul_floor(new_rate - self.b_rate, SCALAR_9)
            .unwrap();
        // Update the reserve's bRate
        self.b_rate = new_rate;

        // Calculate the admin fee - 7 decimal places of precision
        if accrued_interest <= 0 {
            return;
        }
        let admin_fee = accrued_interest
            .fixed_mul_floor(storage::get_take_rate(e), SCALAR_7)
            .unwrap();

        // calculate admins accrued fees
        let pct_accrual = admin_fee
            .fixed_div_floor(new_rate, SCALAR_9)
            .unwrap()
            .fixed_div_floor(self.total_b_tokens, SCALAR_7)
            .unwrap();
        let accrued_shares = pct_accrual
            .fixed_mul_floor(self.total_deposits, SCALAR_7)
            .unwrap()
            .fixed_div_floor(SCALAR_7 - pct_accrual, SCALAR_7)
            .unwrap();

        // Mint the admin fee to the admin address
        let admin_address = storage::get_admin(e);
        let total_fees = self.deposits.get(admin_address.clone()).unwrap_or(0);
        self.deposits
            .set(admin_address, total_fees + accrued_shares);
        self.total_deposits += accrued_shares;
    }

    /// Deposits tokens into the reserve
    pub fn deposit(&mut self, user: Address, b_tokens_amount: i128) {
        // Calculate the share amount
        let share_amount = if self.total_b_tokens == 0 || self.total_deposits == 0 {
            b_tokens_amount
        } else {
            self.b_tokens_to_shares_down(b_tokens_amount)
        };
        // Update the user's balance
        let user_balance = self.deposits.get(user.clone()).unwrap_or(0);
        self.deposits.set(user, user_balance + share_amount);
        // Update the total deposits and bToken deposits
        self.total_deposits += share_amount;
        self.total_b_tokens += b_tokens_amount;
    }

    /// Withdraws tokens from the reserve
    pub fn withdraw(&mut self, e: &Env, user: Address, b_tokens_amount: i128) {
        let user_balance = self
            .deposits
            .get(user.clone())
            .unwrap_or_else(|| panic_with_error!(e, FeeVaultError::InsufficientBalance));
        let share_amount = self.b_tokens_to_shares_up(b_tokens_amount);

        if share_amount > user_balance {
            panic_with_error!(e, FeeVaultError::InsufficientBalance);
        }

        let new_balance = user_balance - share_amount;
        if new_balance <= 10 {
            // we remove deposits with less than 10 stroops to avoid dust getting stuck
            self.deposits.remove(user);
        } else {
            self.deposits.set(user, new_balance);
        }

        // Update the total deposits and bToken deposits
        self.total_deposits -= share_amount;
        self.total_b_tokens -= b_tokens_amount;
    }

    /// Converts the underlying amount to shares
    /// Rounds down
    pub fn underlying_to_shares(&self, amount: i128) -> i128 {
        amount
            .fixed_div_floor(
                self.b_rate
                    .fixed_mul_ceil(self.total_b_tokens, SCALAR_9)
                    .unwrap(),
                SCALAR_7,
            )
            .unwrap()
            .fixed_mul_floor(self.total_deposits, SCALAR_7)
            .unwrap()
    }

    /// Converts the share amount to underlying
    /// Rounds down
    pub fn shares_to_underlying(&self, amount: i128) -> i128 {
        amount
            .fixed_div_floor(self.total_deposits, self.total_b_tokens)
            .unwrap()
            .fixed_mul_floor(self.b_rate, SCALAR_9)
            .unwrap()
    }

    /// Converts a b_token amount to shares rounding down
    pub fn b_tokens_to_shares_down(&self, amount: i128) -> i128 {
        amount
            .fixed_div_floor(self.total_b_tokens, self.total_deposits)
            .unwrap()
    }

    /// Converts a b_token amount to shares rounding up
    pub fn b_tokens_to_shares_up(&self, amount: i128) -> i128 {
        amount
            .fixed_div_ceil(self.total_b_tokens, self.total_deposits)
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::println;

    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        vec, Address,
    };

    use crate::testutils::{create_fee_vault, register_fee_vault};

    use super::*;
    #[test]
    fn test_shares_to_underlying() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        let reserve_data = ReserveData {
            address: Address::generate(&e),
            deposits: Map::new(&e),
            total_b_tokens: 1000_0000000,
            total_deposits: 1200_0000000,
            b_rate: 1_100_000_000,
        };

        // setup pool with deposits
        e.as_contract(&vault_address, || {
            storage::set_reserve_data(&e, 0, reserve_data);
            let reserve = Reserve::load(&e, 0);
            let share_amount = 100_0000000;
            let underlying_amount = reserve.shares_to_underlying(share_amount);
            let expected_underlying_amount = 91_6666300;
            assert_eq!(underlying_amount, expected_underlying_amount);
        });
    }

    #[test]
    fn test_underlying_to_shares() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        let reserve_data = ReserveData {
            address: Address::generate(&e),
            deposits: Map::new(&e),
            total_b_tokens: 1000_0000000,
            total_deposits: 1200_0000000,
            b_rate: 1_100_000_000,
        };

        // setup pool with deposits
        e.as_contract(&vault_address, || {
            storage::set_reserve_data(&e, 0, reserve_data);
            let reserve = Reserve::load(&e, 0);
            let underlying_amount = 91_6666300;
            let share_amount = reserve.underlying_to_shares(underlying_amount);
            let expected_share_amount = 99_9999600;
            assert_eq!(share_amount, expected_share_amount);
        });
    }

    #[test]
    fn test_deposit() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let samwise = Address::generate(&e);
            let frodo = Address::generate(&e);

            let reserve_data = ReserveData {
                address: Address::generate(&e),
                deposits: Map::new(&e),
                total_b_tokens: 1000_0000000,
                total_deposits: 1200_0000000,
                b_rate: 1_100_000_000,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // Perform a deposit for samwise
            reserve.deposit(samwise.clone(), 83_3333300);
            reserve.store(&e);

            // Load the updated reserve to verify the changes
            let expected_share_amount = 99_9999960;
            let updated_reserve = Reserve::load(&e, 0);
            let updated_total_deposits = updated_reserve.total_deposits;
            let updated_total_b_tokens = updated_reserve.total_b_tokens;
            let updated_samwise_balance = updated_reserve.deposits.get(samwise.clone()).unwrap();

            // Assertions
            assert_eq!(updated_samwise_balance, expected_share_amount.clone());
            assert_eq!(updated_total_deposits, 1200_0000000 + expected_share_amount);
            assert_eq!(updated_total_b_tokens, 1000_0000000 + 83_3333300);

            // Perform a deposit for frodo
            reserve.deposit(frodo.clone(), 83_3333300);
            reserve.store(&e);

            // Load the updated reserve to verify the changes
            let expected_share_amount = 999999960;
            let updated_reserve = Reserve::load(&e, 0);
            let updated_total_deposits = updated_reserve.total_deposits;
            let updated_total_b_tokens = updated_reserve.total_b_tokens;
            let updated_frodo_balance = updated_reserve.deposits.get(frodo).unwrap();
            assert_eq!(updated_frodo_balance, expected_share_amount);
            assert_eq!(
                updated_total_deposits,
                1200_0000000 + 99_9999960 + expected_share_amount
            );
            assert_eq!(
                updated_total_b_tokens,
                1000_0000000 + 83_3333300 + 83_3333300
            );

            // Perform another deposit for samwise
            reserve.deposit(samwise.clone(), 83_3333300);
            reserve.store(&e);

            // Load the updated reserve to verify the changes
            let expected_share_amount = 99_9999960;
            let updated_reserve = Reserve::load(&e, 0);
            let updated_total_deposits = updated_reserve.total_deposits;
            let updated_total_b_tokens = updated_reserve.total_b_tokens;
            let updated_samwise_balance = updated_reserve.deposits.get(samwise).unwrap();

            // Assertions
            assert_eq!(updated_samwise_balance, expected_share_amount.clone() * 2);
            assert_eq!(
                updated_total_deposits,
                1200_0000000 + expected_share_amount * 3
            );
            assert_eq!(updated_total_b_tokens, 1000_0000000 + 83_3333300 * 3);
        });
    }

    #[test]
    fn test_initial_deposit() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let samwise = Address::generate(&e);
            let frodo = Address::generate(&e);

            let reserve_data = ReserveData {
                address: Address::generate(&e),
                deposits: Map::new(&e),
                total_b_tokens: 0,
                total_deposits: 0,
                b_rate: 1_000_000_000,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // Perform a deposit for samwise
            reserve.deposit(samwise.clone(), 80_000_0000);
            reserve.store(&e);

            // Load the updated reserve to verify the changes
            let expected_share_amount = 80_000_0000;
            let updated_reserve = Reserve::load(&e, 0);
            let updated_total_deposits = updated_reserve.total_deposits;
            let updated_total_b_tokens = updated_reserve.total_b_tokens;
            let updated_samwise_balance = updated_reserve.deposits.get(samwise).unwrap();

            // Assertions
            assert_eq!(updated_samwise_balance, expected_share_amount.clone());
            assert_eq!(updated_total_deposits, expected_share_amount);
            assert_eq!(updated_total_b_tokens, 80_000_0000);

            // Perform a deposit for frodo
            reserve.deposit(frodo.clone(), 80_000_0000);
            reserve.store(&e);

            // Load the updated reserve to verify the changes
            let expected_share_amount = 80_000_0000;
            let updated_reserve = Reserve::load(&e, 0);
            let updated_total_deposits = updated_reserve.total_deposits;
            let updated_total_b_tokens = updated_reserve.total_b_tokens;
            let updated_frodo_balance = updated_reserve.deposits.get(frodo).unwrap();
            assert_eq!(updated_frodo_balance, expected_share_amount);
            assert_eq!(updated_total_deposits, expected_share_amount * 2);
            assert_eq!(updated_total_b_tokens, expected_share_amount * 2);
        });
    }

    #[test]
    fn test_withdraw() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let samwise = Address::generate(&e);
            let frodo = Address::generate(&e);

            let mut deposits = Map::new(&e);
            deposits.set(samwise.clone(), 100_000_0000);
            deposits.set(frodo.clone(), 55_000_0000);

            let reserve_data = ReserveData {
                address: Address::generate(&e),
                deposits,
                total_b_tokens: 2000_000_0000,
                total_deposits: 2200_000_0000,
                b_rate: 1_200_000_000,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // Perform a withdrawal for samwise
            reserve.withdraw(&e, samwise.clone(), 80_000_0000);
            reserve.store(&e);

            // Load the updated reserve to verify the changes
            let expected_share_amount = 88_000_0000;
            let updated_reserve = Reserve::load(&e, 0);
            let updated_total_deposits = updated_reserve.total_deposits;
            let updated_total_b_tokens = updated_reserve.total_b_tokens;
            let updated_samwise_balance = updated_reserve.deposits.get(samwise).unwrap();

            // Assertions
            assert_eq!(updated_samwise_balance, 12_000_0000);
            assert_eq!(
                updated_total_deposits,
                2200_000_0000 - expected_share_amount
            );
            assert_eq!(updated_total_b_tokens, 2000_000_0000 - 80_000_0000);

            // Perform a withdrawal for frodo
            reserve.withdraw(&e, frodo.clone(), 50_000_0000);
            reserve.store(&e);

            // Load the updated reserve to verify the changes
            let updated_reserve = Reserve::load(&e, 0);
            let updated_total_deposits = updated_reserve.total_deposits;
            let updated_total_b_tokens = updated_reserve.total_b_tokens;
            assert!(updated_reserve.deposits.get(frodo).is_none());
            assert_eq!(
                updated_total_deposits,
                2200_000_0000 - expected_share_amount - 55_000_0000
            );
            assert_eq!(
                updated_total_b_tokens,
                2000_000_0000 - 80_000_0000 - 50_000_0000
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #102)")]
    fn test_over_withdraw() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let samwise = Address::generate(&e);
            let frodo = Address::generate(&e);

            let mut deposits = Map::new(&e);
            deposits.set(samwise.clone(), 100_000_0000);
            deposits.set(frodo.clone(), 55_000_0000);

            let reserve_data = ReserveData {
                address: Address::generate(&e),
                deposits,
                total_b_tokens: 2000_000_0000,
                total_deposits: 2200_000_0000,
                b_rate: 1_200_000_000,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // Perform a withdrawal for samwise
            reserve.withdraw(&e, samwise.clone(), 200_000_0000);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #102)")]
    fn test_over_withdraw_2() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let samwise = Address::generate(&e);

            let deposits = Map::new(&e);

            let reserve_data = ReserveData {
                address: Address::generate(&e),
                deposits,
                total_b_tokens: 2000_000_0000,
                total_deposits: 2200_000_0000,
                b_rate: 1_200_000_000,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // Perform a withdrawal for samwise
            reserve.withdraw(&e, samwise.clone(), 200_000_0000);
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
                deposits: Map::new(&e),
                total_b_tokens: 1000_0000000,
                total_deposits: 1200_0000000,
                b_rate: 1_100_000_000,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // update b_rate to 1.2
            let expected_accrued_fee = 20_3389003;
            reserve.update_rate(&e, 120_000_0000, 100_000_0000);
            reserve.store(&e);
            assert_eq!(
                reserve.deposits.get(bombadil.clone()).unwrap(),
                expected_accrued_fee
            );
            assert_eq!(reserve.total_deposits, 1200_000_0000 + expected_accrued_fee);

            // update b_rate to 1.5
            let expected_accrued_fee_2 = 50_8474541;
            reserve.update_rate(&e, 150_000_0000, 100_000_0000);
            reserve.store(&e);
            assert_eq!(
                reserve.deposits.get(bombadil.clone()).unwrap(),
                expected_accrued_fee + expected_accrued_fee_2
            );
            assert_eq!(
                reserve.total_deposits,
                1200_000_0000 + expected_accrued_fee + expected_accrued_fee_2
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
                deposits: Map::new(&e),
                total_b_tokens: 50_000_0000000,
                total_deposits: 50_000_0000000,
                b_rate: 1_000_000_000,
            };

            // Add the reserve to storage
            storage::set_reserve_data(&e, 0, reserve_data);

            let mut reserve = Reserve::load(&e, 0);
            // update b_rate to 1.2
            let expected_accrued_fee = 106_1283400;
            reserve.update_rate(&e, 9999999999990, 9894986154500);
            reserve.store(&e);
            assert_eq!(
                reserve.deposits.get(bombadil.clone()).unwrap(),
                expected_accrued_fee
            );
            assert_eq!(
                reserve.total_deposits,
                50_000_0000000 + expected_accrued_fee
            );
        });
    }
}
