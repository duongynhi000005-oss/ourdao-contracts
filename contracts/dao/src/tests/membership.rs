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

// ==================== #92: view and payout agree ====================
#[test]
fn pending_yield_view_matches_claim_rewards_payout() {
    let s = setup(3);
    let borrower = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();

    let pid = s.client.request_loan(&borrower, &1_000, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&v1, &pid, &true);
    s.client.vote_on_loan_proposal(&v2, &pid, &true);
    s.client.repay_loan(&borrower, &0);

    let shown = s.client.get_pending_yield(&v1);
    assert!(shown > 0);
    let before = s.token.balance(&v1);
    let claimed = s.client.claim_rewards(&v1);
    assert_eq!(claimed, shown);
    assert_eq!(s.token.balance(&v1) - before, shown);

    // exit_dao pays share + stake + the same displayed pending yield.
    let shown = s.client.get_pending_yield(&v2);
    let share = s.client.calculate_exit_share(&v2);
    let before = s.token.balance(&v2);
    s.client.exit_dao(&v2);
    assert_eq!(s.token.balance(&v2) - before, share + shown);
}

// ==================== issue #271: per-asset yield accumulator ====================
#[test]
fn loan_in_asset_b_repaid_credits_asset_b_yield_leaving_base_yield_unchanged() {
    let s = setup(3);
    let borrower = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();

    let asset_b = whitelist_second_asset(&s, 50_000);

    // Initial pending yields for both assets are strictly 0
    assert_eq!(s.client.get_pending_yield(&v1), 0);
    assert_eq!(s.client.get_pending_yield(&v2), 0);
    assert_eq!(
        s.client.get_pending_yield_in_asset(&v1, &asset_b.address),
        0
    );
    assert_eq!(
        s.client.get_pending_yield_in_asset(&v2, &asset_b.address),
        0
    );

    // Borrower requests and disburses a loan in asset B
    let pid = s
        .client
        .request_loan_in_asset(&borrower, &4_000, &asset_b.address, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&v1, &pid, &true);
    s.client.vote_on_loan_proposal(&v2, &pid, &true);

    let loan = s.client.get_loan(&pid).unwrap();
    let interest = loan.total_repayment - loan.principal;
    assert!(interest > 0);
    let per_member = interest / 3;

    // Repay loan in asset B
    mint_asset(&s, &asset_b.address, &borrower, loan.total_repayment);
    s.client
        .repay_loan_in_asset(&borrower, &pid, &asset_b.address);

    // Core Invariant 1: Base token pending yield remains strictly 0 (unchanged)
    assert_eq!(s.client.get_pending_yield(&borrower), 0);
    assert_eq!(s.client.get_pending_yield(&v1), 0);
    assert_eq!(s.client.get_pending_yield(&v2), 0);

    // Core Invariant 2: Asset B pending yield increased for all active members
    assert_eq!(
        s.client.get_pending_yield_in_asset(&v1, &asset_b.address),
        per_member
    );
    assert_eq!(
        s.client.get_pending_yield_in_asset(&v2, &asset_b.address),
        per_member
    );
    assert_eq!(
        s.client
            .get_pending_yield_in_asset(&borrower, &asset_b.address),
        per_member
    );

    // Member v1 claims reward in asset B
    let v1_bal_before = asset_b.balance(&v1);
    let claimed = s.client.claim_rewards_in_asset(&v1, &asset_b.address);
    assert_eq!(claimed, per_member);
    assert_eq!(asset_b.balance(&v1), v1_bal_before + per_member);
    assert_eq!(
        s.client.get_pending_yield_in_asset(&v1, &asset_b.address),
        0
    );

    // Base yield still strictly 0
    assert_eq!(s.client.get_pending_yield(&v1), 0);

    // Exit settlement in asset B
    let v2_bal_before = asset_b.balance(&v2);
    s.client.exit_dao(&v2);
    assert_eq!(asset_b.balance(&v2), v2_bal_before + per_member);
    assert_eq!(
        s.client.get_pending_yield_in_asset(&v2, &asset_b.address),
        0
    );
}
