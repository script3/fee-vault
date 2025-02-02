use soroban_sdk::{Address, Env, Symbol};

pub struct FeeVaultEvents {}

impl FeeVaultEvents {
    /// Emitted when a new reserve vault is created
    ///
    /// - topics - `["new_reserve_vault", reserve: Address]`
    /// - data - Void
    pub fn new_reserve_vault(e: &Env, reserve: &Address) {
        let topics = (Symbol::new(&e, "new_reserve_vault"), reserve.clone());
        e.events().publish(topics, ());
    }

    /// Emitted when a deposit is performed against a reserve vault
    ///
    /// - topics - `["vault_deposit", reserve: Address, from: Address]`
    /// - data - `[amount: i128, shares: i128, b_tokens: i128]`
    pub fn vault_deposit(
        e: &Env,
        reserve: &Address,
        from: &Address,
        amount: i128,
        shares: i128,
        b_tokens: i128,
    ) {
        let topics = (
            Symbol::new(&e, "vault_deposit"),
            reserve.clone(),
            from.clone(),
        );
        e.events().publish(topics, (amount, shares, b_tokens));
    }

    /// Emitted when a withdraw is performed against a reserve vault
    ///
    /// - topics - `["vault_withdraw", reserve: Address, from: Address]`
    /// - data - `[amount: i128, shares: i128, b_tokens: i128]`
    pub fn vault_withdraw(
        e: &Env,
        reserve: &Address,
        from: &Address,
        amount: i128,
        shares: i128,
        b_tokens: i128,
    ) {
        let topics = (
            Symbol::new(&e, "vault_withdraw"),
            reserve.clone(),
            from.clone(),
        );
        e.events().publish(topics, (amount, shares, b_tokens));
    }

    /// Emitted when fees are claimed from a reserve vault
    ///
    /// - topics - `["vault_fee_claim", reserve: Address, admin: Address]`
    /// - data - `[amount: i128, b_tokens: i128]`
    pub fn vault_fee_claim(
        e: &Env,
        reserve: &Address,
        admin: &Address,
        amount: i128,
        b_tokens: i128,
    ) {
        let topics = (
            Symbol::new(&e, "vault_fee_claim"),
            reserve.clone(),
            admin.clone(),
        );
        e.events().publish(topics, (amount, b_tokens));
    }

    /// Emitted when fees are claimed from a reserve vault
    ///
    /// - topics - `["vault_fee_claim", reserve: Address, admin: Address]`
    /// - data - `amount: i128`
    pub fn vault_emissions_claim(e: &Env, reserve: &Address, admin: &Address, amount: i128) {
        let topics = (
            Symbol::new(&e, "vault_emissions_claim"),
            reserve.clone(),
            admin.clone(),
        );
        e.events().publish(topics, amount);
    }
}
