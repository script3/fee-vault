use crate::constants::{SCALAR_7, SCALAR_9};
use crate::types::ReserveData;
use crate::{errors::FeeVaultError, storage};
use soroban_fixed_point_math::{i128, FixedPoint};
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Map};

#[contracttype]
pub struct Reserve {
    pub id: u32,
    pub address: Address,
    pub b_rate: i128,
    pub total_deposits: i128,
    pub total_b_tokens: i128,
    pub deposits: Map<Address, i128>,
}

impl Reserve {
    pub fn load(e: &Env, id: u32) -> Self {
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
        let data = ReserveData {
            address: self.address.clone(),
            b_rate: self.b_rate,
            total_deposits: self.total_deposits,
            total_b_tokens: self.total_b_tokens,
            deposits: self.deposits.clone(),
        };
        storage::set_reserve_data(e, self.id, data);
    }

    pub fn update_rate(&mut self, e: &Env, underlying_amount: i128, b_tokens_amount: i128) {
        let new_rate = underlying_amount
            .fixed_div_floor(b_tokens_amount, SCALAR_9)
            .unwrap();
        let accrued_interest = self
            .total_b_tokens
            .fixed_mul_floor(new_rate - self.b_rate, SCALAR_9)
            .unwrap();
        let admin_fee = accrued_interest
            .fixed_mul_floor(storage::get_take_rate(e), SCALAR_7)
            .unwrap();
        self.b_rate = new_rate;
        let admin_address = storage::get_admin(e);
        self.deposit(admin_address, admin_fee, 0);
    }

    pub fn deposit(&mut self, user: Address, amount: i128, b_tokens_amount: i128) {
        let share_amount = self.underlying_to_shares(amount);
        let user_balance = self.deposits.get(user.clone()).unwrap_or(0);
        self.deposits.set(user, user_balance + share_amount);
        self.total_deposits += share_amount;
        self.total_b_tokens += b_tokens_amount;
    }

    pub fn withdraw(&mut self, e: &Env, user: Address, amount: i128, b_tokens_amount: i128) {
        let user_balance = self
            .deposits
            .get(user.clone())
            .unwrap_or_else(|| panic_with_error!(e, FeeVaultError::InsufficientBalance));
        let share_amount = self.underlying_to_shares(amount);

        if share_amount > user_balance {
            panic_with_error!(e, FeeVaultError::InsufficientBalance);
        }

        let new_balance = user_balance - share_amount;
        if new_balance == 0 {
            self.deposits.remove(user);
        } else {
            self.deposits.set(user, new_balance);
        }

        self.total_deposits -= share_amount;
        self.total_b_tokens -= b_tokens_amount;
    }

    pub fn underlying_to_shares(&self, amount: i128) -> i128 {
        amount
            .fixed_div_floor(
                self.b_rate
                    .fixed_mul_floor(self.total_b_tokens, SCALAR_9)
                    .unwrap(),
                SCALAR_7,
            )
            .unwrap()
            .fixed_mul_floor(self.total_deposits, SCALAR_7)
            .unwrap()
    }

    pub fn shares_to_underlying(&self, amount: i128) -> i128 {
        amount
            .fixed_div_floor(self.total_deposits, SCALAR_7)
            .unwrap()
            .fixed_mul_floor(
                self.b_rate
                    .fixed_div_floor(self.total_b_tokens, SCALAR_9)
                    .unwrap(),
                SCALAR_9,
            )
            .unwrap()
    }
}
