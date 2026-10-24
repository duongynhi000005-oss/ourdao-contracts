use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, String, Vec};

use super::common::*;
use crate::util::{MAX_STAKE_BONUS, STAKE_WEIGHT_UNIT};
use crate::{OurDao, OurDaoClient};

// ==================== issue #7: property tests ====================
//
// The example tests in the sibling modules pin down behavior at specific, hand-picked numbers.
// These instead generate wide ranges of inputs (amounts, treasury sizes,
// stakes, contributions â€” including near-`i128::MAX` boundaries) and check
// invariants that must hold for *any* input, not just the ones a human
// happened to write down. `amount` inputs are capped at `AMOUNT_BOUND`
// (rather than the full `i128` range) specifically to stay clear of the
// *intermediate* overflow in `amount * BASIS_POINTS` â€” the invariants below
// are about the post-clamp behavior of these functions, not about auditing
// every arithmetic op in isolation.

/// Keeps `amount * BASIS_POINTS` (BASIS_POINTS == 10_000) well clear of
/// i128 overflow while still exercising values many orders of magnitude
/// larger than any real loan or treasury.
const AMOUNT_BOUND: i128 = i128::MAX / 20_000;

fn contract_with_treasury(treasury: i128) -> (Env, OurDaoClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_id = sac.address();
    let token_mint = token::StellarAssetClient::new(&env, &token_id);

    let admin = Address::generate(&env);
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);
    let mut admins = Vec::new(&env);
    admins.push_back(admin);
    client.initialize(&admins, &5_100u32, &FEE, &token_id, &policy());

    if treasury > 0 {
        token_mint.mint(&contract_id, &treasury);
    }
    (env, client)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    /// `calculate_loan_terms`'s rate is a linear curve clamped to
    /// `[min_interest_rate, max_interest_rate]` â€” no amount or treasury
    /// size should ever push it outside that band, and the quoted
    /// repayment should never be less than the amount requested.
    #[test]
    fn loan_terms_rate_stays_within_policy_bounds(
        amount in 0i128..=AMOUNT_BOUND,
        treasury in 0i128..=AMOUNT_BOUND,
    ) {
        let (_env, client) = contract_with_treasury(treasury);
        let terms = client.calculate_loan_terms(&amount);
        let p = policy();
        prop_assert!(terms.interest_rate >= p.min_interest_rate);
        prop_assert!(terms.interest_rate <= p.max_interest_rate);
        prop_assert!(terms.total_repayment >= amount);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(80))]

    /// One base vote plus a square-root boost of up to `MAX_STAKE_BONUS` (5)
    /// bonus votes â€” `k` of them cost `STAKE_WEIGHT_UNIT * k^2` staked tokens
    /// (#182) â€” so weight is always in `[1, 6]` for any non-negative stake,
    /// up to and including `i128::MAX`.
    #[test]
    fn voting_weight_stays_in_bounds(stake in 0i128..=i128::MAX) {
        let env = Env::default();
        let contract_id = env.register(OurDao, ());
        let who = Address::generate(&env);
        let weight = env.as_contract(&contract_id, || {
            crate::storage::set_stake(&env, &who, stake);
            crate::util::voting_weight(&env, &who)
        });
        prop_assert!((1..=6).contains(&weight));
    }

    /// More stake never costs voting weight.
    #[test]
    fn voting_weight_is_monotonic_in_stake(
        a in 0i128..=i128::MAX,
        delta in 0i128..=i128::MAX,
    ) {
        let b = a.saturating_add(delta);
        let env = Env::default();
        let contract_id = env.register(OurDao, ());
        let who = Address::generate(&env);
        let (wa, wb) = env.as_contract(&contract_id, || {
            crate::storage::set_stake(&env, &who, a);
            let wa = crate::util::voting_weight(&env, &who);
            crate::storage::set_stake(&env, &who, b);
            let wb = crate::util::voting_weight(&env, &who);
            (wa, wb)
        });
        prop_assert!(wb >= wa);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(80))]

    /// The quadratic curve (#182) is flat inside each band: wherever a stake
    /// lands, its boost is the band index `k`, both edges of the band
    /// (`k^2` and `(k + 1)^2 - 1` units) earn `k`, and crossing into the next
    /// band is the only thing that buys another vote.
    #[test]
    fn boost_is_constant_within_each_square_band(stake in 0i128..=AMOUNT_BOUND) {
        let band = crate::util::stake_boost(stake);
        let low = STAKE_WEIGHT_UNIT * band * band;
        let next = STAKE_WEIGHT_UNIT * (band + 1) * (band + 1);

        prop_assert!(stake >= low, "stake below the {band}th band");
        prop_assert_eq!(crate::util::stake_boost(low), band);
        if band < MAX_STAKE_BONUS {
            prop_assert!(stake < next);
            prop_assert_eq!(crate::util::stake_boost(next - 1), band);
            prop_assert_eq!(crate::util::stake_boost(next), band + 1);
        }
    }

    /// Past the last band the boost is pinned at the cap, however much more
    /// is staked â€” the whale ceiling.
    #[test]
    fn boost_saturates_at_the_cap(surplus in 0i128..=AMOUNT_BOUND) {
        let stake = STAKE_WEIGHT_UNIT * MAX_STAKE_BONUS * MAX_STAKE_BONUS + surplus;
        prop_assert_eq!(crate::util::stake_boost(stake), MAX_STAKE_BONUS);
    }

    /// Quadratic scaling is never more generous than the linear rule it
    /// replaces: `isqrt(x) <= x` for every `x`, so no stake earns more bonus
    /// votes than `stake / STAKE_WEIGHT_UNIT` did, and a large one earns far
    /// fewer.
    #[test]
    fn boost_never_exceeds_the_linear_rule(stake in 0i128..=AMOUNT_BOUND) {
        let linear = (stake / STAKE_WEIGHT_UNIT).min(MAX_STAKE_BONUS);
        prop_assert!(crate::util::stake_boost(stake) <= linear);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(80))]

    /// `isqrt` never rounds up and never drops a whole step: it is exact on a
    /// perfect square, one below either square it lands on the nearer root.
    #[test]
    fn isqrt_never_rounds_up(r in 1i128..=1i128 << 60) {
        prop_assert_eq!(crate::util::isqrt(r * r), r);
        prop_assert_eq!(crate::util::isqrt(r * r - 1), r - 1);
        prop_assert_eq!(crate::util::isqrt(r * r + 2 * r), r);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// The pull-based accumulator splits `interest` equally across
    /// `active` members: every member ends up with exactly
    /// `interest / active` claimable, the sum never exceeds `interest`,
    /// and whatever isn't evenly divisible (`interest % active`) is
    /// exactly what's left retained by the treasury.
    #[test]
    fn distributed_interest_never_exceeds_collected(
        active in 1u32..=8,
        interest in 0i128..=i128::MAX,
    ) {
        let (env, client) = contract_with_treasury(0);
        let contract_id = client.address.clone();

        let mut members = Vec::new(&env);
        for _ in 0..active {
            let m = Address::generate(&env);
            let sac = token::StellarAssetClient::new(&env, &client.get_token());
            sac.mint(&m, &FEE);
            client.register_member(&m);
            members.push_back(m);
        }

        let asset = client.get_token();
        env.as_contract(&contract_id, || {
            crate::loans::distribute_interest(&env, interest, &asset);
        });

        let per_member = interest / active as i128;
        let mut sum = 0i128;
        for m in members.iter() {
            let pending = client.get_pending_yield(&m);
            prop_assert_eq!(pending, per_member);
            sum += pending;
        }
        prop_assert!(sum <= interest);
        prop_assert_eq!(interest - sum, interest % active as i128);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    /// `calculate_exit_share` is a pro-rata slice of the treasury.
    /// However a member's `contribution` got to its current value
    /// (join at the fee, then possibly reduced by default slashing â€”
    /// see issue's rejoin-after-exit note), each member's share is
    /// bounded by their contribution's fraction of the total, so the
    /// sum across every member can never exceed the treasury itself.
    #[test]
    fn exit_shares_never_exceed_treasury(
        contributions in prop::collection::vec(0i128..=FEE, 1..=5),
        extra_treasury in 0i128..=AMOUNT_BOUND,
    ) {
        let (env, client) = contract_with_treasury(extra_treasury);
        let contract_id = client.address.clone();
        let token_id = client.get_token();

        let mut members = Vec::new(&env);
        for &c in contributions.iter() {
            let m = Address::generate(&env);
            let sac = token::StellarAssetClient::new(&env, &token_id);
            sac.mint(&m, &FEE);
            client.register_member(&m);
            env.as_contract(&contract_id, || {
                let mut rec = crate::storage::get_member(&env, &m).unwrap();
                let delta = c - rec.contribution;
                rec.contribution = c;
                crate::storage::set_member(&env, &rec);
                crate::storage::set_active_contributions(
                    &env,
                    crate::storage::get_active_contributions(&env) + delta,
                );
            });
            members.push_back(m);
        }

        let treasury = client.get_treasury_balance();
        let sum: i128 = members.iter().map(|m| client.calculate_exit_share(&m)).sum();
        prop_assert!(sum <= treasury);
    }
}

// ==================== issue #290: staked funds are never lendable ====================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(25))]

    /// Staked funds are never lendable (#290):
    /// 1. `get_treasury_balance()` strictly excludes `TotalStaked`.
    /// 2. Loans exceeding net treasury ratio are rejected with `ExceedsTreasuryRatio`.
    /// 3. After disbursing an approved loan, the contract's actual token balance
    ///    remains strictly `>= TotalStaked`.
    /// 4. When lendable treasury is exhausted, loan disbursement fails with
    ///    `InsufficientTreasury`, strictly protecting staked tokens from disbursement
    ///    even when raw token balance has sufficient funds.
    #[test]
    fn staked_funds_are_never_lendable_property(
        extra_treasury in 1_000i128..=5_000_000,
        stakes in prop::collection::vec(100i128..=200_000, 2..=3),
        ratio_bps in 500u32..=5_000u32,
    ) {
        let s = setup(3);
        let token_mint = token::StellarAssetClient::new(&s.env, &s.token.address);
        if extra_treasury > 0 {
            token_mint.mint(&s.client.address, &extra_treasury);
        }

        let mut total_staked = 0i128;
        for (i, &amt) in stakes.iter().enumerate() {
            let m = s.members.get(i as u32).unwrap();
            token_mint.mint(&m, &amt);
            s.client.stake(&m, &amt);
            total_staked += amt;
        }

        // Simulate token backing for stakes held in DAO custody
        token_mint.mint(&s.client.address, &total_staked);

        let initial_fee_treasury = 3 * FEE;
        let expected_net_treasury = initial_fee_treasury + extra_treasury;
        let actual_token_balance = s.token.balance(&s.client.address);
        prop_assert_eq!(actual_token_balance, expected_net_treasury + total_staked);

        // Invariant 1: get_treasury_balance strictly excludes TotalStaked
        let spendable_treasury = s.client.get_treasury_balance();
        prop_assert_eq!(spendable_treasury, expected_net_treasury);
        prop_assert!(spendable_treasury < actual_token_balance);

        let borrower = s.members.get(0).unwrap();
        let v1 = s.members.get(1).unwrap();
        let v2 = s.members.get(2).unwrap();

        // Max allowed loan under policy (50% max_loan_to_treasury_ratio)
        let max_allowed = (spendable_treasury * 5_000) / crate::types::BASIS_POINTS;

        // Invariant 2: Sizing loan against actual_token_balance (consuming stake) is rejected
        let illegitimate_amount = ((actual_token_balance * 5_000) / crate::types::BASIS_POINTS).max(max_allowed + 1);
        if illegitimate_amount > max_allowed {
            let res = s.client.try_request_loan(&borrower, &illegitimate_amount, &None);
            prop_assert_eq!(res, Err(Ok(crate::Error::ExceedsTreasuryRatio)));
        }

        // Invariant 3: Valid loan within spendable reserves disburses without touching stake
        let valid_loan_amount = ((spendable_treasury * ratio_bps as i128) / crate::types::BASIS_POINTS).max(1);
        if valid_loan_amount <= max_allowed {
            let pid = s.client.request_loan(&borrower, &valid_loan_amount, &None);
            advance(&s.env, EDITING + 1);

            s.client.vote_on_loan_proposal(&v1, &pid, &true);
            let p_check = s.client.get_loan_proposal(&pid).unwrap();
            if p_check.status == crate::types::ProposalStatus::Pending {
                s.client.vote_on_loan_proposal(&v2, &pid, &true);
            }

            let loan_prop = s.client.get_loan_proposal(&pid).unwrap();
            prop_assert_eq!(loan_prop.status, crate::types::ProposalStatus::Approved);

            // Invariant 4: Token balance after disbursement strictly protects staked funds
            let balance_after = s.token.balance(&s.client.address);
            prop_assert!(
                balance_after >= total_staked,
                "DAO token balance ({}) fell below total_staked ({})",
                balance_after,
                total_staked
            );
            prop_assert_eq!(
                balance_after,
                actual_token_balance - valid_loan_amount,
                "Disbursement must only deduct the requested loan amount"
            );
            prop_assert_eq!(
                balance_after,
                total_staked + (expected_net_treasury - valid_loan_amount),
                "Remaining balance must equal total_staked plus remaining spendable reserves"
            );
        }
    }

    /// Loan disbursement strictly fails if it would consume stake (#290 acceptance criteria)
    #[test]
    fn loan_disbursement_never_consumes_staked_funds(
        raw_treasury in 2_000i128..=500_000,
        staked in 500i128..=200_000,
    ) {
        let s = setup(3);
        let token_mint = token::StellarAssetClient::new(&s.env, &s.token.address);
        token_mint.mint(&s.client.address, &raw_treasury);

        let staker = s.members.get(1).unwrap();
        token_mint.mint(&staker, &staked);
        s.client.stake(&staker, &staked);
        token_mint.mint(&s.client.address, &staked);

        let net_spendable = s.client.get_treasury_balance();
        let total_raw = s.token.balance(&s.client.address);
        prop_assert_eq!(total_raw, net_spendable + staked);

        let borrower = s.members.get(0).unwrap();
        let v1 = s.members.get(1).unwrap();
        let v2 = s.members.get(2).unwrap();

        // Request loan for max allowed ratio (50% of net_spendable)
        let loan_amt = (net_spendable * 5_000) / crate::types::BASIS_POINTS;
        let pid = s.client.request_loan(&borrower, &loan_amt, &None);

        // Before loan voting, drain spendable treasury to an external destination
        // so net_spendable drops below loan_amt, but total_raw STILL contains enough tokens from stake!
        let drain_amt = net_spendable - loan_amt + 1; // leaving net_spendable = loan_amt - 1 < loan_amt
        let dest = Address::generate(&s.env);
        let reason = String::from_str(&s.env, "drain");
        let t_pid = s.client.propose_treasury_withdrawal(&borrower, &drain_amt, &dest, &reason, &false);
        s.client.vote_on_treasury_proposal(&borrower, &t_pid, &true);
        let t_check = s.client.get_treasury_proposal(&t_pid).unwrap();
        if t_check.status == crate::types::ProposalStatus::Pending {
            s.client.vote_on_treasury_proposal(&v1, &t_pid, &true);
        }

        let net_spendable_now = s.client.get_treasury_balance();
        prop_assert!(net_spendable_now < loan_amt);
        // But raw tokens in DAO still exceed loan_amt because staked funds are present!
        let raw_tokens_now = s.token.balance(&s.client.address);
        prop_assert!(raw_tokens_now >= loan_amt);

        // Advance to loan voting window
        advance(&s.env, EDITING + 1);

        // Voting passes, but disbursement MUST NOT consume the staked tokens!
        s.client.vote_on_loan_proposal(&v1, &pid, &true);
        let l_check = s.client.get_loan_proposal(&pid).unwrap();
        if l_check.status == crate::types::ProposalStatus::Pending {
            s.client.vote_on_loan_proposal(&v2, &pid, &true);
        }

        let prop = s.client.get_loan_proposal(&pid).unwrap();
        // Since net spendable is insufficient, proposal goes to ApprovedPendingDisbursement
        prop_assert_eq!(prop.status, crate::types::ProposalStatus::ApprovedPendingDisbursement);

        // Attempting to disburse must fail with InsufficientTreasury
        let disburse_res = s.client.try_disburse_approved_loan(&pid);
        prop_assert_eq!(disburse_res, Err(Ok(crate::Error::InsufficientTreasury)));

        // Total staked tokens remain 100% untouched
        prop_assert_eq!(s.token.balance(&s.client.address), raw_tokens_now);
        prop_assert!(s.token.balance(&s.client.address) >= staked);
    }
}
