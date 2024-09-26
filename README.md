# Overview

This is a fee vault for Blend pools. It is used to allow an admin to collect a portion of the interest earned from blend pools by the vault depositors along with all emissions accrued by vault depositors. Wallets and integrating protocols are the entities typically interested in this functionality.

# How it works

The fee vault contract interacts with the underlying blend pool on behalf of users. It tracks an internal b_rate value and accrues fees on behalf of the admin whenever a user interacts with the vault. The admin can then claim the fees at a later time.

# Usage

## Setup

To set up a fee vault for a blend pool, the admin must first deploy a new fee vault contract.

Once the contracts deployed, the admin can initialize the fee vault by calling `initialize`.

```rust
   /// Initialize the contract
    ///
    /// ### Arguments
    /// * `admin` - The admin address
    /// * `pool` - The blend pool address
    /// * `take_rate` - The take rate for the fee vault, 7 decimal precision
    ///
    /// ### Panics
    /// * `AlreadyInitializedError` - If the contract has already been initialized
    /// * `InvalidTakeRate` - If the take rate is not within 0 and 1_000_0000
    pub fn initialize(e: Env, admin: Address, pool: Address, take_rate: i128)
```

After initializing the contract, the admin must add all pool reserves they wish to support to the vault. This is done by calling `add_reserve` with the reserve address and the reserve's take rate.

```rust
    /// Adds a new reserve to the fee vault
    ///
    /// ### Arguments
    /// * `reserve_id` - The ID of the reserve to add,
    /// must be the same as the blend pool reserve id
    /// * `reserve_address` - The address of the reserve to add,
    /// must be the same as the blend pool reserve address
    ///
    /// ### Note
    /// DO NOT call this function without ensuring the reserve id and address
    /// correspond to the blend pool reserve id and address.
    /// Doing so will cause you to be unable to support the reserve of that id in the future.
    pub fn add_reserve(e: Env, reserve_id: u32, reserve_address: Address)
```

## Integration

To integrate the fee vault into your app or protocol, you will just need to have users deposit with the vaults `deposit` function.

```rust
    /// Deposits tokens into the fee vault for a specific reserve
    ///
    /// ### Arguments
    /// * `from` - The address of the user making the deposit
    /// * `amount` - The amount of tokens to deposit
    /// * `reserve_id` - The ID of the reserve to deposit
    ///
    /// ### Returns
    /// * `i128` - The amount of b-tokens received in exchange for the deposited underlying tokens
    pub fn deposit(e: &Env, from: Address, amount: i128, reserve_id: u32) -> i128
```

and withdraw using the `withdraw` function.

```rust
    /// Withdraws tokens from the fee vault for a specific reserve
    ///
    /// ### Arguments
    /// * `from` - The address of the user making the withdrawal
    /// * `id` - The ID of the reserve to withdraw from
    /// * `amount` - The amount of tokens to withdraw
    ///
    /// ### Returns
    /// * `i128` - The amount of b_tokens withdrawn
    pub fn withdraw(e: &Env, from: Address, id: u32, amount: i128) -> i128
```

You can display to users their current asset balance using the `get_deposits_in_underlying` function.

```rust
    /// Fetch a deposits for a user
    ///
    /// ### Arguments
    /// * `ids` - The ids of the reserves
    /// * `user` - The address of the user
    ///
    /// ### Returns
    /// * `Map<Address, i128>` - A map of underlying addresses and underlying deposit amounts
    pub fn get_deposits_in_underlying(e: Env, ids: Vec<u32>, user: Address)
```

# Limitations

## Collateralizing and Borrowing

The fee vault contract does not currently support collateralizing and borrowing. It only supplies and withdraws tokens from the blend pool.

## Withdrawal Dust Clearing

It's difficult to correctly fully withdraw a users funds without introducing significantly more complexity to the fee vault contract. The current contract implementation instead wipes a users deposit balance if they have less than 0.0000010 tokens remaining.

## Accurately Reading Current User Underlying Balance

It's difficult to accurately estimate the users current underlying balance in a read function. Instead we return the users underlying balance as of the last b_rate observed by the vault.
