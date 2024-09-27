use crate::{errors::FeeVaultError, storage};
use soroban_fixed_point_math::i128;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Map};

#[contracttype]
pub struct User {
    pub address: Address,         // The user's address
    pub deposits: Map<u32, i128>, // The user's deposits, keyed by reserve id and stored in shares with 7 decimal places of precision
}

impl User {
    pub fn load(e: &Env, address: Address) -> Self {
        // Load the reserve data from storage
        let deposits = storage::get_user_deposits(e, &address);
        Self { address, deposits }
    }

    pub fn store(&self, e: &Env) {
        // Store the reserve data in storage
        storage::set_user_deposits(e, &self.address, self.deposits.clone());
    }

    pub fn deposit(&mut self, reserve_id: u32, amount: i128) {
        let existing_amount = self.deposits.get(reserve_id).unwrap_or(0);
        self.deposits.set(reserve_id, existing_amount + amount);
    }

    /// Withdraws tokens from the reserve
    pub fn withdraw(&mut self, e: &Env, reserve_id: u32, amount: i128) {
        let user_balance = self
            .deposits
            .get(reserve_id)
            .unwrap_or_else(|| panic_with_error!(e, FeeVaultError::InsufficientBalance));

        if amount > user_balance {
            panic_with_error!(e, FeeVaultError::InsufficientBalance);
        }

        let new_balance = user_balance - amount;
        if new_balance <= 10 {
            // we remove deposits with less than 10 stroops to avoid dust getting stuck
            self.deposits.remove(reserve_id);
        } else {
            self.deposits.set(reserve_id, new_balance);
        }
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
            let samwise_address = Address::generate(&e);

            let mut samwise = User::load(&e, samwise_address.clone());
            // Perform a deposit for samwise
            samwise.deposit(0, 100_000_0000);
            samwise.store(&e);

            // Load the updated reserve to verify the changes
            let expected_share_amount = 100_000_0000;
            let updated_user = User::load(&e, samwise_address.clone());
            let updated_samwise_balance = updated_user.deposits.get(0).unwrap();

            // Assertions
            assert_eq!(updated_samwise_balance, expected_share_amount.clone());
            // Perform a deposit for samwise
            samwise.deposit(1, 200_000_0000);
            samwise.store(&e);

            // Load the updated reserve to verify the changes
            let expected_share_amount = 200_000_0000;
            let updated_user = User::load(&e, samwise_address.clone());
            let updated_samwise_balance = updated_user.deposits.get(1).unwrap();

            // Assertions
            assert_eq!(updated_samwise_balance, expected_share_amount.clone());
        });
    }

    #[test]
    fn test_withdraw() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let vault_address = register_fee_vault(&e);

        e.as_contract(&vault_address, || {
            let samwise_address = Address::generate(&e);

            let mut deposits = Map::new(&e);
            deposits.set(0, 100_000_0000);
            deposits.set(1, 200_000_0000);

            // Add the reserve to storage
            storage::set_user_deposits(&e, &samwise_address.clone(), deposits);

            // Perform a withdrawal for samwise
            let mut samwise = User::load(&e, samwise_address.clone());
            samwise.withdraw(&e, 0, 80_000_0000);
            samwise.store(&e);

            // Load the updated reserve to verify the changes
            let updated_user = User::load(&e, samwise_address.clone());
            let updated_samwise_balance = updated_user.deposits.get(0).unwrap();

            // Assertions
            assert_eq!(updated_samwise_balance, 20_000_0000);
            assert_eq!(updated_user.deposits.get(1).unwrap(), 200_000_0000);

            // Fully withdraw
            samwise.withdraw(&e, 0, 19_999_9999);
            samwise.store(&e);

            // Load the updated reserve to verify the changes
            let updated_user = User::load(&e, samwise_address.clone());
            let updated_samwise_balance = updated_user.deposits.get(0);
            assert!(updated_samwise_balance.is_none());
            assert_eq!(updated_user.deposits.get(1).unwrap(), 200_000_0000);

            // Withdraw more
            samwise.withdraw(&e, 1, 200_000_0000);
            samwise.store(&e);

            // Load the updated reserve to verify the changes
            let updated_user = User::load(&e, samwise_address.clone());
            let updated_samwise_balance = updated_user.deposits.get(1);
            assert!(updated_samwise_balance.is_none());
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

            let mut deposits = Map::new(&e);
            deposits.set(0, 100_000_0000);

            // Add the reserve to storage
            storage::set_user_deposits(&e, &samwise.clone(), deposits);

            let mut user = User::load(&e, samwise.clone());
            // Perform a withdrawal for samwise
            user.withdraw(&e, 0, 200_000_0000);
        });
    }
}
