use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, String};

use super::common::*;
use crate::types::MemberStatus;
use crate::Error;

#[test]
fn init_and_membership() {
    let s = setup(3);
    assert_eq!(s.client.get_total_members(), 3);
    assert_eq!(s.client.get_active_members(), 3);
    assert_eq!(s.client.get_treasury_balance(), 3 * FEE);
    assert!(s.client.is_member(&s.members.get(0).unwrap()));
    assert!(s.client.is_admin(&s.admin));
    assert_eq!(s.client.get_consensus_threshold(), 5_100);

    let m = s.client.get_member(&s.members.get(0).unwrap()).unwrap();
    assert_eq!(m.status, MemberStatus::ActiveMember);
    assert_eq!(m.contribution, FEE);
}

#[test]
fn double_join_rejected() {
    let s = setup(1);
    let m = s.members.get(0).unwrap();
    let res = s.client.try_register_member(&m);
    assert_eq!(res, Err(Ok(Error::AlreadyMember)));
}

#[test]
fn exit_returns_share() {
    let s = setup(2);
    let m = s.members.get(0).unwrap();
    let before = s.token.balance(&m);
    let share = s.client.calculate_exit_share(&m);
    assert!(share > 0);
    s.client.exit_dao(&m);
    assert_eq!(s.token.balance(&m), before + share);
    assert_eq!(s.client.get_active_members(), 1);
    assert!(!s.client.is_member(&m));
}

// ==================== issue #341: get_delegate view ====================
#[test]
fn get_delegate_reports_delegation_and_none_otherwise() {
    let s = setup(2);
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();

    assert_eq!(s.client.get_delegate(&a), None);

    s.client.delegate_vote(&a, &b);
    assert_eq!(s.client.get_delegate(&a), Some(b.clone()));
    // Delegation is one-way: the delegatee has not delegated anything.
    assert_eq!(s.client.get_delegate(&b), None);
}

// ==================== issue #4: yield accumulator ====================
#[test]
fn yield_accumulator_join_claim_exit_rejoin() {
    let s = setup(3);
    let borrower = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();

    // Get a loan approved and repaid so interest is distributed.
    let pid = s.client.request_loan(&borrower, &1_000, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&v1, &pid, &true);
    s.client.vote_on_loan_proposal(&v2, &pid, &true);
    s.client.repay_loan(&borrower, &0);

    let loan = s.client.get_loan(&0).unwrap();
    let interest = loan.total_repayment - loan.principal;
    let per_member = interest / 3;

    // Both non-borrower members can claim their share.
    assert_eq!(s.client.get_pending_yield(&v1), per_member);
    assert_eq!(s.client.get_pending_yield(&v2), per_member);
    s.client.claim_rewards(&v1);
    assert_eq!(s.client.get_pending_yield(&v1), 0);

    // v1 exits â€” their snapshot is settled so they don't double-claim.
    s.client.exit_dao(&v1);
    assert_eq!(s.client.get_pending_yield(&v1), 0);

    // v1 rejoins â€” snapshot is set to current accumulator, earning nothing
    // from interest that accrued before they rejoined.
    s.client.register_member(&v1);
    assert_eq!(s.client.get_pending_yield(&v1), 0);
}

// ==================== issue: total_contributions ====================
#[test]
fn ten_join_five_exit_pays_remaining_members_full_share() {
    let s = setup(10);

    for i in 0..5u32 {
        let m = s.members.get(i).unwrap();
        assert_eq!(s.client.calculate_exit_share(&m), FEE);
        s.client.exit_dao(&m);
    }

    assert_eq!(s.client.get_total_members(), 10);
    assert_eq!(s.client.get_active_members(), 5);
    assert_eq!(s.client.get_treasury_balance(), 5 * FEE);

    for i in 5..10u32 {
        let m = s.members.get(i).unwrap();
        assert_eq!(s.client.calculate_exit_share(&m), FEE);
    }
}

#[test]
fn rejoin_after_exit_is_counted_once_not_twice() {
    let s = setup(3);
    let m = s.members.get(0).unwrap();

    s.client.exit_dao(&m);
    assert_eq!(s.client.get_active_members(), 2);

    s.client.register_member(&m);
    assert_eq!(s.client.get_total_members(), 3);
    assert_eq!(s.client.get_active_members(), 3);
    assert_eq!(s.client.calculate_exit_share(&m), FEE);
}

#[test]
fn mixed_joins_exits_defaults_never_strand_value() {
    let s = setup(3);
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();
    let c = s.members.get(2).unwrap();

    let pid = s.client.request_loan(&a, &500, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&b, &pid, &true);
    s.client.vote_on_loan_proposal(&c, &pid, &true);

    s.client.exit_dao(&c);
    let b_share_before_default = s.client.calculate_exit_share(&b);

    advance(&s.env, LOAN_DURATION + 1);
    s.client.mark_loan_defaulted(&pid);

    let b_share_after_default = s.client.calculate_exit_share(&b);
    assert!(b_share_after_default > b_share_before_default);

    s.client.exit_dao(&b);
    s.client.register_member(&c);

    assert_eq!(s.client.get_total_members(), 3);
    assert_eq!(s.client.get_active_members(), 2);

    let mut sum = 0i128;
    for i in 0..3u32 {
        let m = s.members.get(i).unwrap();
        if s.client.is_member(&m) {
            sum += s.client.calculate_exit_share(&m);
        }
    }
    assert!(sum <= s.client.get_treasury_balance());
}

#[test]
fn bench_exit_dao_scaling() {
    // #138: Measure cost of exit_dao at different sizes
    for size in [10, 100, 1000] {
        let s = setup(size);
        let m = s.members.get(0).unwrap();

        s.env.cost_estimate().budget().reset_unlimited();
        let cpu_before = s.env.cost_estimate().budget().cpu_instruction_cost();
        s.client.exit_dao(&m);
        let cpu_after = s.env.cost_estimate().budget().cpu_instruction_cost();

        let cost = cpu_after - cpu_before;

        // Fail if cost scales poorly (O(n) check). Keep this no_std-compatible
        // instead of printing from the contract crate.
        assert!(
            size != 1000 || cost <= 50_000_000,
            "exit_dao cost exceeds O(1) bound"
        );
    }
}

// ==================== issue #304: rejoin resets state ====================
#[test]
fn rejoin_after_exit_resets_share_stake_yield_snapshot_and_name() {
    let s = setup(3);
    let m = s.members.get(0).unwrap();
    let borrower = s.members.get(1).unwrap();
    let voter = s.members.get(2).unwrap();

    // Register a name for the member.
    let name = String::from_str(&s.env, "alice");
    s.client.register_name(&m, &name);
    assert_eq!(s.client.name_of(&m), Some(name.clone()));

    // Member stakes.
    s.client.stake(&m, &500);
    assert_eq!(s.client.get_stake(&m), 500);

    // Generate yield by creating a loan and having it repaid.
    let pid = s.client.request_loan(&borrower, &1_000, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&voter, &pid, &true);
    s.client.vote_on_loan_proposal(&m, &pid, &true);
    s.client.repay_loan(&borrower, &0);

    let loan = s.client.get_loan(&0).unwrap();
    let interest = loan.total_repayment - loan.principal;
    let per_member_yield = interest / 3;
    assert!(per_member_yield > 0);

    // Member claims their yield.
    s.client.claim_rewards(&m);

    let pending_before_exit = s.client.get_pending_yield(&m);
    assert_eq!(pending_before_exit, 0);

    // Member exits the DAO.
    s.client.exit_dao(&m);
    assert!(!s.client.is_member(&m));
    assert_eq!(s.client.get_stake(&m), 0);
    assert_eq!(s.client.get_pending_yield(&m), 0);
    // After exiting, the name should be freed.
    assert_eq!(s.client.name_of(&m), None);
    assert_eq!(s.client.resolve_name(&name), None);

    // Member rejoins.
    s.client.register_member(&m);
    assert!(s.client.is_member(&m));

    // Verify all state is reset:
    // - Stake is zero.
    assert_eq!(s.client.get_stake(&m), 0);
    // - Yield snapshot is reset to current accumulator (so pending is zero).
    assert_eq!(s.client.get_pending_yield(&m), 0);
    // - Name is freed and can be re-registered.
    assert_eq!(s.client.name_of(&m), None);
    s.client.register_name(&m, &name);
    assert_eq!(s.client.name_of(&m), Some(name));
}

#[test]
fn rejected_register_member_transfer_rolls_back_all_membership_state() {
    let s = rejecting_setup(0);
    let member = Address::generate(&s.env);
    s.token.mint(&member, &MINT);
    s.token.set_reject_transfers(&true);

    let result = s.client.try_register_member(&member);
    assert!(result.is_err());

    assert_eq!(s.client.get_total_members(), 0);
    assert_eq!(s.client.get_active_members(), 0);
    assert!(!s.client.is_member(&member));
    assert!(s.client.get_member(&member).is_none());
    assert_eq!(s.token.balance(&s.client.address), 0);
}

// ===========================================================================
// Issue #176: Handle token transfer failure cleanly in exit_dao
// ===========================================================================

#[test]
fn exit_dao_returns_error_when_contract_balance_insufficient() {
    let s = rejecting_setup(2);
    let member = s.members.get(0).unwrap();

    // Reject all outgoing transfers to simulate a depleted/frozen treasury.
    s.token.set_reject_transfers(&true);

    let res = s.client.try_exit_dao(&member);
    assert!(
        res.is_err(),
        "exit_dao must fail cleanly when contract cannot pay out"
    );
}

#[test]
fn exit_dao_succeeds_when_treasury_funded() {
    let s = setup(2);
    let member = s.members.get(0).unwrap();

    let share = s.client.calculate_exit_share(&member);
    assert!(share > 0);
    s.client.exit_dao(&member);
    assert!(!s.client.is_member(&member));
}

// ==================== issue #353: maintained ActiveContributions ====================
fn active_contributions(s: &Setup) -> i128 {
    s.env.as_contract(&s.client.address, || {
        crate::storage::get_active_contributions(&s.env)
    })
}

/// Recomputes the total the old O(n) way, to cross-check the running total.
fn scanned_active_contributions(s: &Setup) -> i128 {
    s.env.as_contract(&s.client.address, || {
        crate::storage::get_members(&s.env)
            .iter()
            .filter_map(|a| crate::storage::get_member(&s.env, &a))
            .filter(|m| m.status == MemberStatus::ActiveMember)
            .map(|m| m.contribution)
            .sum()
    })
}

#[test]
fn active_contributions_tracks_join_exit_and_rejoin() {
    let s = setup(4);
    assert_eq!(active_contributions(&s), 4 * FEE);

    let m = s.members.get(0).unwrap();
    s.client.exit_dao(&m);
    assert_eq!(active_contributions(&s), 3 * FEE);
    assert_eq!(active_contributions(&s), scanned_active_contributions(&s));

    s.client.register_member(&m);
    assert_eq!(active_contributions(&s), 4 * FEE);
    assert_eq!(active_contributions(&s), scanned_active_contributions(&s));
}

#[test]
fn active_contributions_matches_scan_after_default_penalty() {
    let s = setup(3);
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();
    let c = s.members.get(2).unwrap();

    let pid = s.client.request_loan(&a, &500, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&b, &pid, &true);
    s.client.vote_on_loan_proposal(&c, &pid, &true);

    advance(&s.env, LOAN_DURATION + 1);
    s.client.mark_loan_defaulted(&pid);

    let total = active_contributions(&s);
    assert!(total < 3 * FEE, "penalty must reduce the running total");
    assert_eq!(total, scanned_active_contributions(&s));
}

// ==================== issue #340: get_member_overview view ====================
#[test]
fn get_member_overview_aggregates_member_state() {
    let s = setup(3);
    let borrower = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();

    s.client
        .register_name(&borrower, &String::from_str(&s.env, "alice"));
    s.client.delegate_vote(&borrower, &v1);

    let pid = s.client.request_loan(&borrower, &1_000, &None);
    advance(&s.env, EDITING + 1);
    // With borrower's delegation, v1 has 2 votes which immediately approves the proposal.
    s.client.vote_on_loan_proposal(&v1, &pid, &true);
    s.client.repay_loan(&borrower, &0);
    s.client.stake(&borrower, &500);

    let loan = s.client.get_loan(&0).unwrap();
    let per_member = (loan.total_repayment - loan.principal) / 3;

    let overview = s.client.get_member_overview(&borrower);
    assert_eq!(overview.member, s.client.get_member(&borrower).unwrap());
    assert_eq!(overview.member.status, MemberStatus::ActiveMember);
    assert_eq!(overview.stake, 500);
    assert_eq!(overview.pending_yield, per_member);
    assert_eq!(
        overview.loan_stats,
        s.client.get_member_loan_stats(&borrower)
    );
    assert_eq!(overview.loan_stats.total_loans, 1);
    assert_eq!(overview.loan_stats.repaid_loans, 1);
    assert_eq!(overview.name, Some(String::from_str(&s.env, "alice")));
    assert_eq!(overview.delegate, Some(v1.clone()));

    // An exited member keeps their record, reported as Inactive.
    s.client.exit_dao(&v2);
    assert_eq!(
        s.client.get_member_overview(&v2).member.status,
        MemberStatus::Inactive
    );

    let stranger = Address::generate(&s.env);
    assert_eq!(
        s.client.try_get_member_overview(&stranger),
        Err(Ok(Error::NotMember))
    );
}

// ==================== issue #307: tests for delegate_vote & delegation ====================

#[test]
fn delegation_re_delegation_updates_voting_weights() {
    let s = setup(3);
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();
    let c = s.members.get(2).unwrap();

    // Directly set delegation in storage to test delegation weight transitions.
    s.env.as_contract(&s.client.address, || {
        crate::storage::set_delegation(&s.env, &a, &b);
    });

    let weight_a = s
        .env
        .as_contract(&s.client.address, || crate::util::voting_weight(&s.env, &a));
    let weight_b = s
        .env
        .as_contract(&s.client.address, || crate::util::voting_weight(&s.env, &b));
    let weight_c = s
        .env
        .as_contract(&s.client.address, || crate::util::voting_weight(&s.env, &c));
    assert_eq!(weight_a, 0, "delegator vote weight must be 0");
    assert_eq!(weight_b, 2, "delegatee b receives a's weight");
    assert_eq!(weight_c, 1, "c remains at base weight");

    // Re-delegation from b to c
    s.env.as_contract(&s.client.address, || {
        crate::storage::set_delegation(&s.env, &a, &c);
    });

    let weight_b_after = s
        .env
        .as_contract(&s.client.address, || crate::util::voting_weight(&s.env, &b));
    let weight_c_after = s
        .env
        .as_contract(&s.client.address, || crate::util::voting_weight(&s.env, &c));
    assert_eq!(weight_b_after, 1, "b loses a's weight upon re-delegation");
    assert_eq!(weight_c_after, 2, "c now receives a's weight");
}

#[test]
fn delegation_loop_a_to_b_to_a_deadlocks_voting_weight() {
    let s = setup(2);
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();

    // A delegates to B, B delegates to A
    s.env.as_contract(&s.client.address, || {
        crate::storage::set_delegation(&s.env, &a, &b);
        crate::storage::set_delegation(&s.env, &b, &a);
    });

    let weight_a = s
        .env
        .as_contract(&s.client.address, || crate::util::voting_weight(&s.env, &a));
    let weight_b = s
        .env
        .as_contract(&s.client.address, || crate::util::voting_weight(&s.env, &b));

    // In a mutual delegation loop, neither member can cast any votes (both weights are 0).
    assert_eq!(weight_a, 0);
    assert_eq!(weight_b, 0);
}

#[test]
fn delegated_weight_used_in_votes() {
    let s = setup(3);
    let borrower = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();

    // v2 delegates to v1
    s.env.as_contract(&s.client.address, || {
        crate::storage::set_delegation(&s.env, &v2, &v1);
    });

    let pid = s.client.request_loan(&borrower, &1_000, &None);
    advance(&s.env, EDITING + 1);

    // v1 votes: with v2's delegation, v1 has 2 votes, reaching 51% threshold immediately!
    s.client.vote_on_loan_proposal(&v1, &pid, &true);

    let prop = s.client.get_loan_proposal(&pid).unwrap();
    assert_eq!(prop.for_votes, 2);
    assert_eq!(prop.status, crate::types::ProposalStatus::Approved);
}

#[test]
fn delegation_exited_member_clears_delegation_and_does_not_leak_weight() {
    let s = setup(3);
    let _borrower = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();

    // v2 delegates to v1
    s.client.delegate_vote(&v2, &v1);
    assert_eq!(s.client.get_delegate(&v2), Some(v1.clone()));

    let weight_v1_before = s.env.as_contract(&s.client.address, || {
        crate::util::voting_weight(&s.env, &v1)
    });
    assert_eq!(weight_v1_before, 2);

    // v2 exits the DAO
    s.client.exit_dao(&v2);
    assert_eq!(s.client.get_active_members(), 2);

    // After exit, v2's delegation is cleared and no weight leaks to v1 (#409)
    assert_eq!(s.client.get_delegate(&v2), None);
    let weight_v1_after = s.env.as_contract(&s.client.address, || {
        crate::util::voting_weight(&s.env, &v1)
    });
    assert_eq!(
        weight_v1_after, 1,
        "voting_weight must not leak weight from inactive/exited members"
    );
}

#[test]
fn delegate_vote_rejects_circular_delegation() {
    let s = setup(3);
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();
    let c = s.members.get(2).unwrap();

    // A delegates to B
    s.client.delegate_vote(&a, &b);
    assert_eq!(s.client.get_delegate(&a), Some(b.clone()));

    // Mutual A -> B -> A must fail (#409)
    let res = s.client.try_delegate_vote(&b, &a);
    assert_eq!(res, Err(Ok(Error::InvalidDelegation)));

    // B delegates to C: A -> B -> C
    s.client.delegate_vote(&b, &c);
    assert_eq!(s.client.get_delegate(&b), Some(c.clone()));

    // C tries to delegate to A (A -> B -> C -> A cycle): must fail (#409)
    let res2 = s.client.try_delegate_vote(&c, &a);
    assert_eq!(res2, Err(Ok(Error::InvalidDelegation)));
}

#[test]
fn delegate_vote_unilateral_succeeds_without_delegatee_auth() {
    let s = setup(2);
    let delegator = s.members.get(0).unwrap();
    let delegatee = s.members.get(1).unwrap();

    // Unilateral delegation should succeed with single delegator auth (#409)
    s.client.delegate_vote(&delegator, &delegatee);
    assert_eq!(s.client.get_delegate(&delegator), Some(delegatee.clone()));
}

// ==================== #280: base token isolation ====================
#[test]
fn register_member_only_transfers_base_token_ignoring_whitelisted_assets() {
    let s = setup(1);
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

    let applicant = Address::generate(&s.env);
    base_mint.mint(&applicant, &10_000);
    extra_mint.mint(&applicant, &10_000);

    let applicant_base_before = s.token.balance(&applicant);
    let applicant_extra_before = extra_token.balance(&applicant);
    let contract_base_before = s.token.balance(&s.client.address);
    let contract_extra_before = extra_token.balance(&s.client.address);

    s.client.register_member(&applicant);

    // Only the base token balance changed: fee was deducted from applicant and credited to contract.
    assert_eq!(s.token.balance(&applicant), applicant_base_before - FEE);
    assert_eq!(
        s.token.balance(&s.client.address),
        contract_base_before + FEE
    );

    // The whitelisted non-base asset was completely untouched.
    assert_eq!(extra_token.balance(&applicant), applicant_extra_before);
    assert_eq!(
        extra_token.balance(&s.client.address),
        contract_extra_before
    );
    assert_eq!(extra_token.balance(&s.client.address), 0);
}
