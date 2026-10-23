use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{Address, Env, Vec};

use super::common::*;
use crate::types::ProposalStatus;
use crate::util::{
    isqrt, stake_boost, voting_weight, BASE_VOTE_WEIGHT, MAX_STAKE_BONUS, STAKE_WEIGHT_UNIT,
};
use crate::Error;

#[test]
fn staking_boosts_voting_weight() {
    let s = setup(2); // required for loan = ceil(2*51%) = 2
    let borrower = s.members.get(0).unwrap();
    let staker = s.members.get(1).unwrap();

    // Quadratic curve: 2 bonus votes cost 2^2 * 100 = 400 staked tokens
    // (#182). One staked yes-vote = weight 3 >= 2.
    s.client.stake(&staker, &400);
    assert_eq!(s.client.get_stake(&staker), 400);

    let pid = s.client.request_loan(&borrower, &500, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&staker, &pid, &true);

    let prop = s.client.get_loan_proposal(&pid).unwrap();
    assert_eq!(prop.for_votes, 3);
    assert_eq!(prop.status, ProposalStatus::Approved);

    // Unstake returns tokens.
    let before = s.token.balance(&staker);
    s.client.unstake(&staker, &400);
    assert_eq!(s.token.balance(&staker), before + 400);
    assert_eq!(s.client.get_stake(&staker), 0);
}

#[test]
fn rejected_stake_transfer_leaves_stake_storage_unchanged() {
    let s = rejecting_setup(1);
    let member = s.members.get(0).unwrap();
    let dao_balance_before = s.token.balance(&s.client.address);

    s.token.set_reject_transfers(&true);
    let result = s.client.try_stake(&member, &500);
    assert!(result.is_err());

    assert_eq!(s.client.get_stake(&member), 0);
    let total_staked = s.env.as_contract(&s.client.address, || {
        crate::storage::get_total_staked(&s.env)
    });
    assert_eq!(total_staked, 0);
    assert_eq!(s.token.balance(&s.client.address), dao_balance_before);
    let has_stake_time = s.env.as_contract(&s.client.address, || {
        s.env
            .storage()
            .persistent()
            .has(&crate::storage::DataKey::StakeTime(member.clone()))
    });
    assert!(
        !has_stake_time,
        "stake timestamp must roll back with the rejected transfer"
    );
}

#[test]
fn rejected_unstake_transfer_leaves_stake_storage_unchanged() {
    let s = rejecting_setup(1);
    let member = s.members.get(0).unwrap();

    // Stake succeeds while transfers are allowed.
    s.client.stake(&member, &500);
    assert_eq!(s.client.get_stake(&member), 500);

    // Reject the payout leg: counters must stay in sync with the vault.
    s.token.set_reject_transfers(&true);
    let result = s.client.try_unstake(&member, &500);
    assert!(result.is_err());

    assert_eq!(s.client.get_stake(&member), 500);
    let total_staked = s.env.as_contract(&s.client.address, || {
        crate::storage::get_total_staked(&s.env)
    });
    assert_eq!(total_staked, 500);
}

// Issue #193: StakingRewardClaimed event on yield distribution
#[test]
fn claim_rewards_emits_staking_reward_claimed_event_and_updates_snapshot() {
    let s = setup(3);
    let borrower = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();

    let pid = s.client.request_loan(&borrower, &1_000, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&v1, &pid, &true);
    s.client.vote_on_loan_proposal(&v2, &pid, &true);
    s.client.repay_loan(&borrower, &0);

    let loan = s.client.get_loan(&0).unwrap();
    let interest = loan.total_repayment - loan.principal;
    let expected_share = interest / 3;
    assert!(expected_share > 0);

    assert_eq!(s.client.get_pending_yield(&v1), expected_share);

    // Claim rewards
    let claimed = s.client.claim_rewards(&v1);
    assert_eq!(claimed, expected_share);

    // Verify event payload and topics
    assert!(emitted(&s.env, "claimed"));

    // Find the claimed event in event log
    let all_events = s.env.events().all();
    let events_vec = all_events.events();
    let event = events_vec
        .iter()
        .rev()
        .find(|e| {
            let ContractEventBody::V0(body) = &e.body;
            matches!(body.topics.first(), Some(ScVal::Symbol(sym)) if sym.0.to_utf8_string_lossy() == "claimed")
        })
        .expect("claimed event not found");

    let ContractEventBody::V0(body) = &event.body;
    assert_eq!(body.topics.len(), 3);
    // Topic 0: symbol "claimed"
    match &body.topics[0] {
        ScVal::Symbol(sym) => assert_eq!(sym.0.to_utf8_string_lossy(), "claimed"),
        _ => panic!("unexpected topic 0"),
    }
    // Verify topic 2 has the claimed amount
    match &body.topics[2] {
        ScVal::I128(amount) => {
            let val = ((amount.hi as i128) << 64) | (amount.lo as i128);
            assert_eq!(val, expected_share);
        }
        _ => panic!("unexpected topic 2"),
    }

    // Verify accumulator snapshot updated on member record
    assert_eq!(s.client.get_pending_yield(&v1), 0);
    assert_eq!(
        s.client.try_claim_rewards(&v1),
        Err(Ok(Error::NothingToClaim))
    );
}

// ===========================================================================
// Issue #182: quadratic (square-root) staking boost
// ===========================================================================

/// Records `stake` for `who` and reads back the voting weight the contract
/// would give them, bypassing the transfer so arbitrarily large stakes can be
/// exercised without minting the matching tokens.
fn weight_with_stake(env: &Env, contract: &Address, who: &Address, stake: i128) -> i128 {
    env.as_contract(contract, || {
        crate::storage::set_stake(env, who, stake);
        voting_weight(env, who)
    })
}

#[test]
fn isqrt_floors_the_real_square_root() {
    // Exact squares, the values just below and just above them, and both ends
    // of the i128 range.
    let cases: [(i128, i128); 19] = [
        (0, 0),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 2),
        (5, 2),
        (8, 2),
        (9, 3),
        (15, 3),
        (16, 4),
        (17, 4),
        (24, 4),
        (25, 5),
        (99, 9),
        (100, 10),
        (1_000_000, 1_000),
        (1_000_001, 1_000),
        (i128::MAX, 13_043_817_825_332_782_212),
        (i128::MIN, 0),
    ];
    for (n, expected) in cases {
        assert_eq!(isqrt(n), expected, "isqrt({n})");
    }
}

#[test]
fn isqrt_is_exact_on_every_perfect_square() {
    for k in 1i128..=2_000 {
        // `k^2` is a perfect square, `k^2 - 1` sits just below it, and
        // `k^2 + 2k` is one short of `(k + 1)^2`.
        assert_eq!(isqrt(k * k), k, "isqrt({})", k * k);
        assert_eq!(isqrt(k * k - 1), k - 1, "isqrt({} - 1)", k * k);
        assert_eq!(isqrt(k * k + 2 * k), k, "isqrt(({})^2 - 1)", k + 1);
    }
}

/// The defining property of the quadratic curve (#182): `k` bonus votes cost
/// `STAKE_WEIGHT_UNIT * k^2` staked tokens (100, 400, 900, 1600, 2500), and
/// each band's upper edge is one token short of the next bonus vote.
#[test]
fn bonus_votes_follow_the_square_root_curve() {
    for k in 0i128..=MAX_STAKE_BONUS {
        let stake = STAKE_WEIGHT_UNIT * k * k;
        assert_eq!(stake_boost(stake), k, "boost at {stake} staked");

        if k < MAX_STAKE_BONUS {
            let next_vote = STAKE_WEIGHT_UNIT * (k + 1) * (k + 1);
            assert_eq!(stake_boost(next_vote - 1), k, "boost at {next_vote} - 1");
            assert_eq!(stake_boost(next_vote), k + 1, "boost at {next_vote}");
        }
    }
}

/// Voting power grows with the *square root* of the stake: 4x the tokens buys
/// 2x the votes, and 2x the tokens buys strictly less than 2x the votes — a
/// linear curve would have paid out in full for both.
#[test]
fn quadrupling_stake_doubles_the_boost() {
    for k in 1i128..=MAX_STAKE_BONUS {
        let k_votes = STAKE_WEIGHT_UNIT * k * k;
        assert_eq!(stake_boost(k_votes * 4), (k * 2).min(MAX_STAKE_BONUS));
    }
    for k in 1i128..=MAX_STAKE_BONUS / 2 {
        let k_votes = STAKE_WEIGHT_UNIT * k * k;
        assert!(
            stake_boost(k_votes * 2) < k * 2,
            "2x the stake of {k_votes} buys < {} votes",
            k * 2
        );
    }
}

// ==================== issue #303: boundary tests for staking curve ====================
#[test]
fn staking_curve_boundary_tests() {
    // Test boundary at 5 bonus votes (MAX_STAKE_BONUS).
    // Formula: k bonus votes cost k^2 * STAKE_WEIGHT_UNIT tokens.
    // At k=5: need 5^2 * 100 = 2500 tokens for exactly 5 bonus votes.
    let at_5_bonus = STAKE_WEIGHT_UNIT * 5 * 5; // 2500
    assert_eq!(stake_boost(at_5_bonus), 5);

    // Just below 5 bonus votes: at 2499 tokens, should get 4 bonus votes.
    let below_5_bonus = at_5_bonus - 1;
    assert_eq!(stake_boost(below_5_bonus), 4);

    // Above the 5-bonus cap: at 6^2 * 100 = 3600 tokens, should still be capped at 5.
    let above_5_bonus = STAKE_WEIGHT_UNIT * 6 * 6; // 3600
    assert_eq!(stake_boost(above_5_bonus), 5);

    // Even at much higher stakes, the boost is capped at MAX_STAKE_BONUS (5).
    assert_eq!(stake_boost(STAKE_WEIGHT_UNIT * 10 * 10), 5);
    assert_eq!(stake_boost(STAKE_WEIGHT_UNIT * 100 * 100), 5);
}

#[test]
fn staking_curve_boundary_voting_weight() {
    let s = setup(2);
    let staker = s.members.get(0).unwrap();
    let borrower = s.members.get(1).unwrap();

    // Stake at the 5-bonus boundary: 2500 tokens = 5 bonus votes + 1 base = 6 voting weight.
    let at_5_bonus = STAKE_WEIGHT_UNIT * 5 * 5; // 2500
    s.client.stake(&staker, &at_5_bonus);

    let pid = s.client.request_loan(&borrower, &500, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&staker, &pid, &true);

    let prop = s.client.get_loan_proposal(&pid).unwrap();
    assert_eq!(prop.for_votes, 6); // 1 base + 5 bonus

    // Repay the first loan so borrower no longer has an active loan.
    s.client.repay_loan(&borrower, &pid);

    // Stake above the cap: 3600 tokens should still give only 5 bonus votes = 6 voting weight.
    s.client.unstake(&staker, &at_5_bonus);
    let above_5_bonus = STAKE_WEIGHT_UNIT * 6 * 6; // 3600
    s.client.stake(&staker, &above_5_bonus);

    let pid2 = s.client.request_loan(&borrower, &500, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&staker, &pid2, &true);

    let prop2 = s.client.get_loan_proposal(&pid2).unwrap();
    assert_eq!(prop2.for_votes, 6); // Still capped at 1 base + 5 bonus
}

#[test]
fn voting_weight_is_capped_above_the_square_root_curve() {
    let env = Env::default();
    let contract = env.register(crate::OurDao, ());
    let who = Address::generate(&env);

    // The cap is reached at 5^2 * 100 = 2500 staked tokens; a stake seven
    // orders of magnitude larger buys nothing more.
    let capped = BASE_VOTE_WEIGHT + MAX_STAKE_BONUS;
    assert_eq!(weight_with_stake(&env, &contract, &who, 2_500), capped);
    assert_eq!(weight_with_stake(&env, &contract, &who, 1_000_000), capped);
    assert_eq!(weight_with_stake(&env, &contract, &who, i128::MAX), capped);

    // The floor: without a full vote's worth of stake a member still holds
    // exactly their one base vote.
    assert_eq!(
        weight_with_stake(&env, &contract, &who, 0),
        BASE_VOTE_WEIGHT
    );
    assert_eq!(
        weight_with_stake(&env, &contract, &who, 99),
        BASE_VOTE_WEIGHT
    );
}

/// End-to-end through the public API: a member's weight follows their staked
/// balance as `1 + isqrt(stake / 100)`, capped at 6.
#[test]
fn member_voting_weight_tracks_staked_balance() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    let contract = s.client.address.clone();

    let mut staked = 0i128;
    for (target, boost) in [
        (100, 1),
        (400, 2),
        (900, 3),
        (1_600, 4),
        (2_500, 5),
        (100_000, 5),
    ] {
        s.client.stake(&member, &(target - staked));
        staked = target;
        assert_eq!(s.client.get_stake(&member), staked);
        assert_eq!(
            weight_with_stake(&s.env, &contract, &member, staked),
            BASE_VOTE_WEIGHT + boost,
            "weight with {staked} staked"
        );
    }

    // Unstaking (past the cooldown) gives the weight back, down to the base
    // vote.
    advance(&s.env, VOTING_PERIOD + 1);
    s.client.unstake(&member, &staked);
    assert_eq!(
        weight_with_stake(&s.env, &contract, &member, 0),
        BASE_VOTE_WEIGHT
    );
}

/// Democratic balance (#182): a whale staking 900k tokens still holds only 6
/// votes — one short of the 7 a 12-member DAO needs — and a single unstaked
/// ally is enough to carry the proposal, which no amount of staking by one
/// member alone could do.
#[test]
fn a_whale_cannot_decide_a_proposal_on_its_own() {
    let s = setup(12);
    let borrower = s.members.get(0).unwrap();
    let whale = s.members.get(1).unwrap();
    let ally = s.members.get(2).unwrap();

    let whale_stake = 900_000;
    s.client.stake(&whale, &whale_stake);
    assert_eq!(stake_boost(whale_stake), MAX_STAKE_BONUS);

    let pid = s.client.request_loan(&borrower, &500, &None);
    advance(&s.env, EDITING + 1);

    // A whale-sized stake is capped at 6 votes, one short of the 7 required.
    s.client.vote_on_loan_proposal(&whale, &pid, &true);
    let prop = s.client.get_loan_proposal(&pid).unwrap();
    assert_eq!(prop.for_votes, BASE_VOTE_WEIGHT + MAX_STAKE_BONUS);
    assert_eq!(prop.status, ProposalStatus::Pending);

    // One unstaked member (a single base vote) tips it over.
    s.client.vote_on_loan_proposal(&ally, &pid, &true);
    let prop = s.client.get_loan_proposal(&pid).unwrap();
    assert_eq!(prop.for_votes, 7);
    assert_eq!(prop.status, ProposalStatus::Approved);
}

/// The same total stake, distributed differently: pooled in one member it buys
/// 6 votes, spread over four members it buys 20. Concentrating capital is a
/// bad deal under the quadratic curve, which is the balance the linear curve
/// used to break.
#[test]
fn pooled_stake_is_outvoted_by_the_same_stake_spread_out() {
    let s = setup(5);
    let whale = s.members.get(0).unwrap();
    let mut spread: Vec<Address> = Vec::new(&s.env);
    for i in 1..5 {
        spread.push_back(s.members.get(i).unwrap());
    }

    let total = 9_000;
    let quarter = total / 4;
    s.client.stake(&whale, &total);
    for member in spread.iter() {
        s.client.stake(&member, &quarter);
    }

    let pooled = weight_with_stake(&s.env, &s.client.address, &whale, total);
    let shared: i128 = spread
        .iter()
        .map(|m| weight_with_stake(&s.env, &s.client.address, &m, quarter))
        .sum();

    assert_eq!(pooled, BASE_VOTE_WEIGHT + MAX_STAKE_BONUS);
    assert_eq!(shared, 20);
    assert!(shared > pooled);
}

// ==================== issue #93: get_voting_weight view + stake threshold views ====================

/// Zero stake → base weight of BASE_VOTE_WEIGHT (1).
#[test]
fn voting_weight_zero_stake_is_base() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    assert_eq!(s.client.get_stake(&member), 0);
    assert_eq!(s.client.get_voting_weight(&member), BASE_VOTE_WEIGHT);
}

/// Exactly one STAKE_WEIGHT_UNIT staked → quadratic boost = isqrt(1) = 1,
/// so total weight = BASE_VOTE_WEIGHT + 1 = 2.
#[test]
fn voting_weight_exactly_one_unit() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    let unit = s.client.get_stake_weight_unit();
    s.client.stake(&member, &unit);
    assert_eq!(s.client.get_voting_weight(&member), BASE_VOTE_WEIGHT + 1);
}

/// One token short of a full unit → isqrt(0) = 0, still base weight.
#[test]
fn voting_weight_one_below_unit_is_still_base() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    let unit = s.client.get_stake_weight_unit();
    s.client.stake(&member, &(unit - 1));
    assert_eq!(s.client.get_voting_weight(&member), BASE_VOTE_WEIGHT);
}

/// Stake far above the cap; bonus is capped at MAX_STAKE_BONUS.
#[test]
fn voting_weight_capped_at_max_bonus() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    let unit = s.client.get_stake_weight_unit();
    let cap = s.client.get_max_stake_bonus();
    // Quadratic cap: MAX_STAKE_BONUS^2 * unit gets the cap, multiply by 10 to go well past it.
    s.client.stake(&member, &(unit * cap * cap * 10));
    assert_eq!(s.client.get_voting_weight(&member), BASE_VOTE_WEIGHT + cap);
}

/// Stake exactly at the quadratic cap threshold: MAX_STAKE_BONUS^2 * unit.
#[test]
fn voting_weight_at_exact_cap() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    let unit = s.client.get_stake_weight_unit();
    let cap = s.client.get_max_stake_bonus();
    // isqrt(cap^2 * unit / unit) = isqrt(cap^2) = cap.
    s.client.stake(&member, &(unit * cap * cap));
    assert_eq!(s.client.get_voting_weight(&member), BASE_VOTE_WEIGHT + cap);
}

/// The constant views are reachable on-chain and match the compile-time values.
#[test]
fn stake_threshold_constants_are_discoverable() {
    let s = setup(1);
    assert_eq!(
        s.client.get_stake_weight_unit(),
        crate::util::STAKE_WEIGHT_UNIT
    );
    assert_eq!(s.client.get_max_stake_bonus(), crate::util::MAX_STAKE_BONUS);
}

// ===========================================================================
// Issue #175: Prevent unstaking while member has an active pending proposal
// ===========================================================================

#[test]
fn unstake_rejected_while_proposal_in_voting() {
    let s = setup(3);
    let borrower = s.members.get(0).unwrap();

    s.client.stake(&borrower, &200);
    let pid = s.client.request_loan(&borrower, &500, &None);

    advance(&s.env, EDITING + 1);
    let prop = s.client.get_loan_proposal(&pid).unwrap();
    assert_eq!(prop.phase, crate::types::ProposalPhase::Voting);

    let res = s.client.try_unstake(&borrower, &200);
    assert_eq!(
        res,
        Err(Ok(crate::Error::HasActiveLoan)),
        "should not be able to unstake with an active pending proposal"
    );
}

#[test]
fn unstake_rejected_while_proposal_in_editing() {
    let s = setup(3);
    let borrower = s.members.get(0).unwrap();

    s.client.stake(&borrower, &200);
    let _pid = s.client.request_loan(&borrower, &500, &None);

    let res = s.client.try_unstake(&borrower, &200);
    assert_eq!(
        res,
        Err(Ok(crate::Error::HasActiveLoan)),
        "should not be able to unstake with a proposal in editing phase"
    );
}

#[test]
fn unstake_allowed_after_proposal_approved() {
    let s = setup(3);
    let borrower = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();

    s.client.stake(&borrower, &200);
    let pid = s.client.request_loan(&borrower, &500, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&v1, &pid, &true);
    s.client.vote_on_loan_proposal(&v2, &pid, &true);

    let prop = s.client.get_loan_proposal(&pid).unwrap();
    assert_eq!(prop.status, ProposalStatus::Approved);

    // Advance past the stake cooldown (voting_period).
    advance(&s.env, VOTING_PERIOD + 1);

    s.client.unstake(&borrower, &200);
    assert_eq!(s.client.get_stake(&borrower), 0);
}

#[test]
fn unstake_allowed_after_proposal_expired() {
    let s = setup(3);
    let borrower = s.members.get(0).unwrap();

    s.client.stake(&borrower, &200);
    let _pid = s.client.request_loan(&borrower, &500, &None);

    // Advance well past the voting deadline so the proposal expires.
    advance(&s.env, EDITING + VOTING_PERIOD + 1);

    s.client.unstake(&borrower, &200);
    assert_eq!(s.client.get_stake(&borrower), 0);
}

#[test]
fn unstaking_during_open_loan_ballot_is_rejected() {
    let s = setup(3);
    let borrower = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    s.client.stake(&voter, &200);
    let pid = s.client.request_loan(&borrower, &500, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&voter, &pid, &false);
    advance(&s.env, VOTING_PERIOD / 2);
    assert_eq!(
        s.client.try_unstake(&voter, &200),
        Err(Ok(Error::HasActiveLoan))
    );
}

#[test]
fn unstaking_during_open_treasury_ballot_is_rejected() {
    let s = setup(4);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    s.client.stake(&voter, &200);
    let pid = s.client.propose_treasury_withdrawal(
        &proposer,
        &100,
        &Address::generate(&s.env),
        &soroban_sdk::String::from_str(&s.env, "vote"),
        &false,
    );
    s.client.vote_on_treasury_proposal(&voter, &pid, &false);
    advance(&s.env, VOTING_PERIOD / 2);
    assert_eq!(
        s.client.try_unstake(&voter, &200),
        Err(Ok(Error::HasActiveLoan))
    );
}

#[test]
fn unstaking_during_unrevealed_private_commitment_is_rejected() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    s.client.stake(&voter, &200);
    let pid = s.client.propose_treasury_withdrawal(
        &proposer,
        &100,
        &Address::generate(&s.env),
        &soroban_sdk::String::from_str(&s.env, "private"),
        &true,
    );
    let salt = soroban_sdk::BytesN::from_array(&s.env, &[1u8; 32]);
    let hash = crate::privacy::compute_commitment(&s.env, &voter, pid, true, &salt);
    s.client.commit_treasury_vote(&voter, &pid, &hash);
    advance(&s.env, VOTING_PERIOD / 2);
    assert_eq!(
        s.client.try_unstake(&voter, &200),
        Err(Ok(Error::HasActiveLoan))
    );
}

// ==================== #280: base token isolation ====================
#[test]
fn staking_and_unstaking_only_transfers_base_token_ignoring_whitelisted_assets() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    let base_mint = soroban_sdk::token::StellarAssetClient::new(&s.env, &s.token.address);

    let issuer = Address::generate(&s.env);
    let extra_sac = s.env.register_stellar_asset_contract_v2(issuer);
    let extra_token = soroban_sdk::token::Client::new(&s.env, &extra_sac.address());
    let extra_mint = soroban_sdk::token::StellarAssetClient::new(&s.env, &extra_sac.address());

    s.client
        .propose_token_addition(&s.admin, &extra_sac.address());
    advance(&s.env, crate::admin::TIMELOCK_DURATION);
    s.client
        .execute_token_addition(&s.admin, &extra_sac.address());
    assert!(s.client.is_token_whitelisted(&extra_sac.address()));

    base_mint.mint(&member, &50_000);
    extra_mint.mint(&member, &50_000);

    let stake_amount = 20_000;
    let member_base_before = s.token.balance(&member);
    let member_extra_before = extra_token.balance(&member);
    let contract_base_before = s.token.balance(&s.client.address);
    let contract_extra_before = extra_token.balance(&s.client.address);

    s.client.stake(&member, &stake_amount);

    // Staking strictly uses base token
    assert_eq!(s.token.balance(&member), member_base_before - stake_amount);
    assert_eq!(
        s.token.balance(&s.client.address),
        contract_base_before + stake_amount
    );
    assert_eq!(extra_token.balance(&member), member_extra_before);
    assert_eq!(
        extra_token.balance(&s.client.address),
        contract_extra_before
    );
    assert_eq!(extra_token.balance(&s.client.address), 0);

    // Unstaking also strictly returns base token
    s.client.unstake(&member, &stake_amount);

    assert_eq!(s.token.balance(&member), member_base_before);
    assert_eq!(s.token.balance(&s.client.address), contract_base_before);
    assert_eq!(extra_token.balance(&member), member_extra_before);
    assert_eq!(
        extra_token.balance(&s.client.address),
        contract_extra_before
    );
}
