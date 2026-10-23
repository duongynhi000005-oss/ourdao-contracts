use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Map, String};

use super::common::*;
use crate::types::ProposalStatus;
use crate::Error;

#[test]
fn treasury_withdrawal_open_vote() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &600, &dest, &reason, &false);

    s.client.vote_on_treasury_proposal(&v1, &pid, &true);
    let mid = s.client.get_treasury_proposal(&pid).unwrap();
    assert_eq!(mid.status, ProposalStatus::Pending); // 1 vote not enough (needs 2)

    s.client.vote_on_treasury_proposal(&v2, &pid, &true);
    let done = s.client.get_treasury_proposal(&pid).unwrap();
    assert_eq!(done.status, ProposalStatus::Executed);
    assert_eq!(s.token.balance(&dest), 600);
}

#[test]
fn approved_but_unfundable_treasury_withdrawal_waits_then_executes_after_refill() {
    // 4 members => treasury 4000, 3 of 4 votes required.
    let s = setup(4);
    let p = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");

    // Two withdrawals that each fit today but not both together.
    let first = s
        .client
        .propose_treasury_withdrawal(&p, &2_000, &dest, &reason, &false);
    let second = s
        .client
        .propose_treasury_withdrawal(&p, &2_500, &dest, &reason, &false);

    for voter in [&p, &v1, &v2] {
        s.client.vote_on_treasury_proposal(voter, &first, &true);
    }
    assert_eq!(s.client.get_treasury_balance(), 2_000);
    assert_eq!(s.token.balance(&dest), 2_000);

    s.client.vote_on_treasury_proposal(&p, &second, &true);
    s.client.vote_on_treasury_proposal(&v1, &second, &true);
    s.client.vote_on_treasury_proposal(&v2, &second, &true); // approved, but only 2000 left
    assert!(emitted(&s.env, "tre_wait"));

    let prop = s.client.get_treasury_proposal(&second).unwrap();
    assert_eq!(prop.status, ProposalStatus::ApprovedPendingDisbursement);
    assert_eq!(s.token.balance(&dest), 2_000); // nothing more moved
    assert_eq!(s.client.get_treasury_balance(), 2_000);
    assert_eq!(
        s.client.try_execute_treasury_proposal(&second),
        Err(Ok(Error::InsufficientTreasury))
    );

    refill_treasury(&s);
    refill_treasury(&s);
    s.client.execute_treasury_proposal(&second);

    let prop = s.client.get_treasury_proposal(&second).unwrap();
    assert_eq!(prop.status, ProposalStatus::Executed);
    assert_eq!(s.token.balance(&dest), 4_500);
}

#[test]
fn rejected_treasury_transfer_rolls_back_approval_vote_and_execution_state() {
    let s = rejecting_setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter_one = s.members.get(1).unwrap();
    let voter_two = s.members.get(2).unwrap();
    let destination = Address::generate(&s.env);

    let proposal_id = s.client.propose_treasury_withdrawal(
        &proposer,
        &500,
        &destination,
        &String::from_str(&s.env, "rollback test"),
        &false,
    );

    s.client
        .vote_on_treasury_proposal(&voter_one, &proposal_id, &true);
    let before = s.client.get_treasury_proposal(&proposal_id).unwrap();
    assert_eq!(before.status, ProposalStatus::Pending);
    assert_eq!(before.for_votes, 1);
    assert_eq!(before.votes_cast, 1);

    s.token.set_reject_transfers(&true);
    let result = s
        .client
        .try_vote_on_treasury_proposal(&voter_two, &proposal_id, &true);
    assert!(result.is_err());

    let after = s.client.get_treasury_proposal(&proposal_id).unwrap();
    assert_eq!(after, before, "proposal changes must roll back");
    let has_vote = s.env.as_contract(&s.client.address, || {
        crate::storage::has_treasury_voted(&s.env, proposal_id, &voter_two)
    });
    assert!(
        !has_vote,
        "vote marker must not survive a rejected execution transfer"
    );
    assert_eq!(s.token.balance(&destination), 0);
}

// Issue #172: Treasury withdrawal must account for reserved loan commitments
#[test]
fn treasury_withdrawal_blocked_by_approved_pending_loan() {
    // 4 members => treasury 4000.
    let s = setup(4);
    let borrower = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let v3 = s.members.get(3).unwrap();
    let dest = Address::generate(&s.env);

    // Drain treasury so it can't cover the loan immediately, forcing it into
    // ApprovedPendingDisbursement when votes pass.
    let drain_dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "drain");
    let drain_id = s
        .client
        .propose_treasury_withdrawal(&v1, &2_500, &drain_dest, &reason, &false);
    s.client.vote_on_treasury_proposal(&v1, &drain_id, &true);
    s.client.vote_on_treasury_proposal(&v2, &drain_id, &true);
    s.client.vote_on_treasury_proposal(&v3, &drain_id, &true);
    // treasury is now 1500

    // Refill so the loan request passes the treasury-ratio check.
    refill_treasury(&s); // +1000 => treasury 2500
    let pid = s.client.request_loan(&borrower, &1_000, &None);
    advance(&s.env, EDITING + 1);

    // Drain again so the loan approval parks in ApprovedPendingDisbursement.
    let drain2_dest = Address::generate(&s.env);
    let drain2_id =
        s.client
            .propose_treasury_withdrawal(&v1, &2_000, &drain2_dest, &reason, &false);
    s.client.vote_on_treasury_proposal(&v1, &drain2_id, &true);
    s.client.vote_on_treasury_proposal(&v2, &drain2_id, &true);
    s.client.vote_on_treasury_proposal(&v3, &drain2_id, &true);
    // treasury is now 500

    s.client.vote_on_loan_proposal(&v1, &pid, &true);
    s.client.vote_on_loan_proposal(&v2, &pid, &true);
    s.client.vote_on_loan_proposal(&v3, &pid, &true);
    let prop = s.client.get_loan_proposal(&pid).unwrap();
    assert_eq!(prop.status, ProposalStatus::ApprovedPendingDisbursement);

    // treasury=500, committed=1000 => available=-500: any withdrawal is rejected.
    let reason2 = String::from_str(&s.env, "withdrawal attempt");
    let blocked = s
        .client
        .try_propose_treasury_withdrawal(&v1, &1, &dest, &reason2, &false);
    assert_eq!(blocked, Err(Ok(Error::InsufficientTreasury)));

    // Refill enough to cover the committed loan with room to spare.
    refill_treasury(&s); // +1000 => treasury 1500, committed 1000, available 500
    refill_treasury(&s); // +1000 => treasury 2500, committed 1000, available 1500

    // A withdrawal within available amount succeeds.
    let ok_pid = s
        .client
        .propose_treasury_withdrawal(&v1, &500, &dest, &reason2, &false);
    assert!(s.client.get_treasury_proposal(&ok_pid).is_some());

    // A withdrawal that would eat into the committed amount is still rejected.
    let too_much = s
        .client
        .try_propose_treasury_withdrawal(&v2, &1_600, &dest, &reason2, &false);
    assert_eq!(too_much, Err(Ok(Error::InsufficientTreasury)));
}

// ==================== issue #236: withdrawals in a second asset ====================

#[test]
fn withdrawal_in_second_asset_pays_out_that_asset() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");
    let asset = whitelist_second_asset(&s, 5_000);
    let base_treasury = s.client.get_treasury_balance();

    // More than the base treasury holds, but covered by the second asset.
    let pid = s.client.propose_withdrawal_in_asset(
        &proposer,
        &4_000,
        &asset.address,
        &dest,
        &reason,
        &false,
    );
    s.client.vote_on_treasury_proposal(&v1, &pid, &true);
    s.client.vote_on_treasury_proposal(&v2, &pid, &true);

    let done = s.client.get_treasury_proposal(&pid).unwrap();
    assert_eq!(done.status, ProposalStatus::Executed);
    assert_eq!(asset.balance(&dest), 4_000);
    assert_eq!(s.token.balance(&dest), 0);
    assert_eq!(
        s.client.get_treasury_balance_in_asset(&asset.address),
        1_000
    );
    assert_eq!(s.client.get_treasury_balance(), base_treasury);
}

#[test]
fn withdrawal_in_asset_rejects_unlisted_or_unfunded_asset() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");
    let issuer = Address::generate(&s.env);
    let unknown = s.env.register_stellar_asset_contract_v2(issuer).address();

    let res = s
        .client
        .try_propose_withdrawal_in_asset(&proposer, &100, &unknown, &dest, &reason, &false);
    assert_eq!(res, Err(Ok(Error::TokenNotWhitelisted)));

    let asset = whitelist_second_asset(&s, 500);
    let res = s.client.try_propose_withdrawal_in_asset(
        &proposer,
        &501,
        &asset.address,
        &dest,
        &reason,
        &false,
    );
    assert_eq!(res, Err(Ok(Error::InsufficientTreasury)));
}

// ==================== issue #185: cancel during editing (migrated from test.rs) ====================

#[test]
fn cancel_treasury_proposal_emits_event_and_sets_cancelled_status() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");

    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &600, &dest, &reason, &false);
    let prop = s.client.get_treasury_proposal(&pid).unwrap();
    assert_eq!(prop.status, ProposalStatus::Pending);

    s.client.cancel_treasury_proposal(&proposer, &pid);
    // Events only cover the latest invocation, so check before reading state.
    assert_prop_cancelled_event(&s.env, pid);

    let prop = s.client.get_treasury_proposal(&pid).unwrap();
    assert_eq!(prop.status, ProposalStatus::Cancelled);
}

// `tally` writes the proposal once, after execution is attempted (#357): the
// final status and vote counts must still be persisted on both outcomes.
#[test]
fn tally_persists_status_and_votes_whether_executed_or_waiting() {
    let s = setup(4); // treasury 4000, 3 of 4 votes required
    let p = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");

    let paid = s
        .client
        .propose_treasury_withdrawal(&p, &2_000, &dest, &reason, &false);
    let parked = s
        .client
        .propose_treasury_withdrawal(&p, &2_500, &dest, &reason, &false);
    for voter in [&p, &v1, &v2] {
        s.client.vote_on_treasury_proposal(voter, &paid, &true);
        s.client.vote_on_treasury_proposal(voter, &parked, &true);
    }

    let executed = s.client.get_treasury_proposal(&paid).unwrap();
    assert_eq!(executed.status, ProposalStatus::Executed);
    assert_eq!((executed.for_votes, executed.votes_cast), (3, 3));

    let waiting = s.client.get_treasury_proposal(&parked).unwrap();
    assert_eq!(waiting.status, ProposalStatus::ApprovedPendingDisbursement);
    assert_eq!((waiting.for_votes, waiting.votes_cast), (3, 3));
    assert_eq!(s.token.balance(&dest), 2_000);
}

#[test]
fn propose_treasury_withdrawal_rejects_self_destination() {
    let s = setup(2);
    let proposer = s.members.get(0).unwrap();
    let self_dest = s.client.address.clone();
    let reason = String::from_str(&s.env, "grant to self");

    let res = s
        .client
        .try_propose_treasury_withdrawal(&proposer, &500, &self_dest, &reason, &false);
    assert_eq!(res, Err(Ok(Error::InvalidProposal)));

    let res_asset = s.client.try_propose_withdrawal_in_asset(
        &proposer,
        &500,
        &s.token.address,
        &self_dest,
        &reason,
        &false,
    );
    assert_eq!(res_asset, Err(Ok(Error::InvalidProposal)));
}

// ==================== issue #281: get_treasury_balances view ====================

#[test]
fn get_treasury_balances_initial_base_token_only() {
    let s = setup(3);
    let base = s.token.address.clone();
    let expected_base_balance = s.client.get_treasury_balance();

    let balances: Map<Address, i128> = s.client.get_treasury_balances();
    assert_eq!(balances.len(), 1);
    assert_eq!(balances.get(base), Some(expected_base_balance));
}

#[test]
fn get_treasury_balances_returns_all_assets_and_tracks_disbursements() {
    let s = setup(3);
    let base = s.token.address.clone();
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);

    // 1. Initial state: base token only
    let base_initial = s.client.get_treasury_balance();
    let initial_map = s.client.get_treasury_balances();
    assert_eq!(initial_map.len(), 1);
    assert_eq!(initial_map.get(base.clone()), Some(base_initial));

    // 2. Whitelist second asset (asset_b) with 5,000 funding
    let asset_b = whitelist_second_asset(&s, 5_000);
    // 3. Whitelist third asset (asset_c) with 2,500 funding
    let asset_c = whitelist_second_asset(&s, 2_500);

    let multi_map = s.client.get_treasury_balances();
    assert_eq!(multi_map.len(), 3);
    assert_eq!(multi_map.get(base.clone()), Some(base_initial));
    assert_eq!(multi_map.get(asset_b.address.clone()), Some(5_000));
    assert_eq!(multi_map.get(asset_c.address.clone()), Some(2_500));

    // Verify consistency with individual views
    assert_eq!(
        multi_map.get(base.clone()),
        Some(s.client.get_treasury_balance())
    );
    assert_eq!(
        multi_map.get(asset_b.address.clone()),
        Some(s.client.get_treasury_balance_in_asset(&asset_b.address))
    );
    assert_eq!(
        multi_map.get(asset_c.address.clone()),
        Some(s.client.get_treasury_balance_in_asset(&asset_c.address))
    );

    // 4. Disburse 3,000 from asset_b
    let reason = String::from_str(&s.env, "grant");
    let pid_b = s.client.propose_withdrawal_in_asset(
        &proposer,
        &3_000,
        &asset_b.address,
        &dest,
        &reason,
        &false,
    );
    s.client.vote_on_treasury_proposal(&v1, &pid_b, &true);
    s.client.vote_on_treasury_proposal(&v2, &pid_b, &true);

    let after_b_map = s.client.get_treasury_balances();
    assert_eq!(after_b_map.len(), 3);
    assert_eq!(after_b_map.get(base.clone()), Some(base_initial));
    assert_eq!(after_b_map.get(asset_b.address.clone()), Some(2_000));
    assert_eq!(after_b_map.get(asset_c.address.clone()), Some(2_500));

    // 5. Disburse 500 from base token
    let pid_base = s
        .client
        .propose_treasury_withdrawal(&proposer, &500, &dest, &reason, &false);
    s.client.vote_on_treasury_proposal(&v1, &pid_base, &true);
    s.client.vote_on_treasury_proposal(&v2, &pid_base, &true);

    let final_map = s.client.get_treasury_balances();
    assert_eq!(final_map.len(), 3);
    assert_eq!(final_map.get(base.clone()), Some(base_initial - 500));
    assert_eq!(final_map.get(asset_b.address.clone()), Some(2_000));
    assert_eq!(final_map.get(asset_c.address.clone()), Some(2_500));
}
