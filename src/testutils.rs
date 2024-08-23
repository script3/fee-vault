// #![cfg(test)]

// use crate::{
//     constants::SCALAR_7, storage::ONE_DAY_LEDGERS, BackstopBootstrapper, BackstopBootstrapperClient,
// };
// use blend_contract_sdk::testutils::BlendFixture;
// use soroban_fixed_point_math::FixedPoint;
// use soroban_sdk::{
//     testutils::{Ledger as _, LedgerInfo},
//     Address, Env,
// };

// pub(crate) fn create_bootstrapper(e: &Env, blend_fixture: &BlendFixture) -> Address {
//     let address = e.register_contract(None, BackstopBootstrapper {});
//     let client = BackstopBootstrapperClient::new(e, &address);
//     client.initialize(
//         &blend_fixture.backstop.address,
//         &blend_fixture.backstop_token.address,
//         &blend_fixture.pool_factory.address,
//     );
//     address
// }

// pub trait EnvTestUtils {
//     /// Jump the env by the given amount of ledgers. Assumes 5 seconds per ledger.
//     fn jump(&self, ledgers: u32);

//     /// Set the ledger to the default LedgerInfo
//     ///
//     /// Time -> 1441065600 (Sept 1st, 2015 12:00:00 AM UTC)
//     /// Sequence -> 100
//     fn set_default_info(&self);
// }

// impl EnvTestUtils for Env {
//     fn jump(&self, ledgers: u32) {
//         self.ledger().set(LedgerInfo {
//             timestamp: self.ledger().timestamp().saturating_add(ledgers as u64 * 5),
//             protocol_version: 20,
//             sequence_number: self.ledger().sequence().saturating_add(ledgers),
//             network_id: Default::default(),
//             base_reserve: 10,
//             min_temp_entry_ttl: 30 * ONE_DAY_LEDGERS,
//             min_persistent_entry_ttl: 30 * ONE_DAY_LEDGERS,
//             max_entry_ttl: 365 * ONE_DAY_LEDGERS,
//         });
//     }

//     fn set_default_info(&self) {
//         self.ledger().set(LedgerInfo {
//             timestamp: 1441065600, // Sept 1st, 2015 12:00:00 AM UTC
//             protocol_version: 20,
//             sequence_number: 100,
//             network_id: Default::default(),
//             base_reserve: 10,
//             min_temp_entry_ttl: 30 * ONE_DAY_LEDGERS,
//             min_persistent_entry_ttl: 30 * ONE_DAY_LEDGERS,
//             max_entry_ttl: 365 * ONE_DAY_LEDGERS,
//         });
//     }
// }

// pub fn assert_approx_eq_abs(a: i128, b: i128, delta: i128) {
//     assert!(
//         a > b - delta && a < b + delta,
//         "assertion failed: `(left != right)` \
//          (left: `{:?}`, right: `{:?}`, epsilon: `{:?}`)",
//         a,
//         b,
//         delta
//     );
// }

// /// Asset that `b` is within `percentage` of `a` where `percentage`
// /// is a percentage in decimal form as a fixed-point number with 7 decimal
// /// places
// pub fn assert_approx_eq_rel(a: i128, b: i128, percentage: i128) {
//     let rel_delta = b.fixed_mul_floor(percentage, SCALAR_7).unwrap();

//     assert!(
//         a > b - rel_delta && a < b + rel_delta,
//         "assertion failed: `(left != right)` \
//          (left: `{:?}`, right: `{:?}`, epsilon: `{:?}`)",
//         a,
//         b,
//         rel_delta
//     );
// }

// // ***** Comet Utils *****

// const SCALAR_7_F64: f64 = SCALAR_7 as f64;

// /// Estimate the number of shares to be minted when running close
// pub fn est_close_mint(
//     blnd: i128,
//     usdc: i128,
//     comet_blnd: i128,
//     comet_usdc: i128,
//     comet_shares: i128,
// ) -> i128 {
//     let mut blnd_f64 = blnd as f64 / SCALAR_7_F64;
//     let mut usdc_f64 = usdc as f64 / SCALAR_7_F64;
//     let mut comet_blnd_f64 = comet_blnd as f64 / SCALAR_7_F64;
//     let mut comet_usdc_f64 = comet_usdc as f64 / SCALAR_7_F64;
//     let mut comet_shares_f64 = comet_shares as f64 / SCALAR_7_F64;

//     let shares_blnd = (comet_shares_f64 * blnd_f64) / comet_blnd_f64;
//     let shares_usdc = (comet_shares_f64 * usdc_f64) / comet_usdc_f64;
//     if shares_blnd > shares_usdc {
//         // more BLND relative to USDC
//         let mut shares = shares_usdc;
//         // calculate how much BLND is spent to mint shares
//         let spent_blnd = ((shares + comet_shares_f64) / comet_shares_f64 - 1.0) * comet_blnd_f64;
//         blnd_f64 -= spent_blnd;
//         comet_blnd_f64 += spent_blnd;
//         comet_shares_f64 += shares;
//         // add how many shares can be minted with the remaining USDC
//         shares += comet_shares_f64 * ((1.0 + blnd_f64 / comet_blnd_f64).powf(0.8) - 1.0);
//         (shares * SCALAR_7_F64) as i128
//     } else {
//         // more USDC relative to BLND
//         let mut shares = shares_blnd;
//         // calculate how much BLND is spent to mint shares
//         let spent_usdc = ((shares + comet_shares_f64) / comet_shares_f64 - 1.0) * comet_usdc_f64;
//         usdc_f64 -= spent_usdc;
//         comet_usdc_f64 += spent_usdc;
//         comet_shares_f64 += shares;
//         // add how many shares can be minted with the remaining USDC
//         shares += comet_shares_f64 * ((1.0 + usdc_f64 / comet_usdc_f64).powf(0.2) - 1.0);
//         (shares * SCALAR_7_F64) as i128
//     }
// }
