extern crate std;

use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{Address, Env, String};

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

// ==================== issue #285: single-token / multi-token equivalence ====================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TreasuryPath {
    SingleToken,
    MultiToken,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedEventRecord {
    topic: std::string::String,
    topics_debug: std::string::String,
    data_debug: std::string::String,
}

fn capture_events_by_topic(env: &Env, topic_name: &str) -> std::vec::Vec<CapturedEventRecord> {
    let all = env.events().all();
    let mut matching = std::vec::Vec::new();
    for e in all.events().iter() {
        let ContractEventBody::V0(body) = &e.body;
        if let Some(ScVal::Symbol(sym)) = body.topics.first() {
            if sym.0.to_utf8_string_lossy() == topic_name {
                matching.push(CapturedEventRecord {
                    topic: sym.0.to_utf8_string_lossy().into(),
                    topics_debug: std::format!("{:?}", body.topics),
                    data_debug: std::format!("{:?}", body.data),
                });
            }
        }
    }
    matching
}

struct TreasuryRunResult {
    pid: u32,
    proposal: crate::types::TreasuryProposal,
    prop_events: std::vec::Vec<CapturedEventRecord>,
    dest_balance_after_exec: i128,
    treasury_balance_after_exec: i128,
    proposal_after_exec: crate::types::TreasuryProposal,
}

fn execute_treasury_lifecycle(
    path: TreasuryPath,
    amount: i128,
    private: bool,
) -> TreasuryRunResult {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant-funds");

    // Drain initial setup events
    let _ = s.env.events().all();

    let pid = match path {
        TreasuryPath::SingleToken => s
            .client
            .propose_treasury_withdrawal(&proposer, &amount, &dest, &reason, &private),
        TreasuryPath::MultiToken => s.client.propose_withdrawal_in_asset(
            &proposer,
            &amount,
            &s.token.address,
            &dest,
            &reason,
            &private,
        ),
    };

    let prop_events = capture_events_by_topic(&s.env, "tre_prop");
    assert_eq!(
        prop_events.len(),
        1,
        "exactly 1 tre_prop event must be emitted"
    );
    assert!(
        !emitted(&s.env, "tre_asset"),
        "tre_asset must not be emitted for base token treasury withdrawals"
    );

    let proposal = s.client.get_treasury_proposal(&pid).unwrap();

    // If not private, vote to execute
    if !private {
        s.client.vote_on_treasury_proposal(&v1, &pid, &true);
        s.client.vote_on_treasury_proposal(&v2, &pid, &true);
    }

    let dest_balance_after_exec = s.token.balance(&dest);
    let treasury_balance_after_exec = s.client.get_treasury_balance();
    let proposal_after_exec = s.client.get_treasury_proposal(&pid).unwrap();

    TreasuryRunResult {
        pid,
        proposal,
        prop_events,
        dest_balance_after_exec,
        treasury_balance_after_exec,
        proposal_after_exec,
    }
}

#[test]
fn parametrized_treasury_withdrawal_parity() {
    for amount in [50, 500, 1_000] {
        let res_single = execute_treasury_lifecycle(TreasuryPath::SingleToken, amount, false);
        let res_multi = execute_treasury_lifecycle(TreasuryPath::MultiToken, amount, false);

        assert_eq!(res_single.pid, res_multi.pid);
        assert_eq!(res_single.proposal.amount, res_multi.proposal.amount);
        assert_eq!(res_single.proposal.reason, res_multi.proposal.reason);
        assert_eq!(res_single.proposal.status, res_multi.proposal.status);
        assert_eq!(res_single.proposal.private, res_multi.proposal.private);
        assert_eq!(
            res_single.proposal.treasury_threshold,
            res_multi.proposal.treasury_threshold
        );

        assert_eq!(res_single.prop_events, res_multi.prop_events);

        assert_eq!(
            res_single.dest_balance_after_exec,
            res_multi.dest_balance_after_exec
        );
        assert_eq!(res_single.dest_balance_after_exec, amount);
        assert_eq!(
            res_single.treasury_balance_after_exec,
            res_multi.treasury_balance_after_exec
        );

        assert_eq!(
            res_single.proposal_after_exec.status,
            ProposalStatus::Executed
        );
        assert_eq!(
            res_multi.proposal_after_exec.status,
            ProposalStatus::Executed
        );
        assert_eq!(
            res_single.proposal_after_exec.for_votes,
            res_multi.proposal_after_exec.for_votes
        );
        assert_eq!(
            res_single.proposal_after_exec.votes_cast,
            res_multi.proposal_after_exec.votes_cast
        );
    }
}

#[test]
fn treasury_withdrawal_private_proposal_parity() {
    let res_single = execute_treasury_lifecycle(TreasuryPath::SingleToken, 300, true);
    let res_multi = execute_treasury_lifecycle(TreasuryPath::MultiToken, 300, true);

    assert_eq!(res_single.proposal.private, true);
    assert_eq!(res_multi.proposal.private, true);
    assert_eq!(res_single.proposal.status, ProposalStatus::Pending);
    assert_eq!(res_multi.proposal.status, ProposalStatus::Pending);
    assert_eq!(res_single.prop_events, res_multi.prop_events);

    // Direct open voting on private proposals fails on both paths
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "private-grant");

    let pid_single = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);
    let pid_multi = s.client.propose_withdrawal_in_asset(
        &proposer,
        &300,
        &s.token.address,
        &dest,
        &reason,
        &true,
    );

    assert_eq!(
        s.client
            .try_vote_on_treasury_proposal(&voter, &pid_single, &true),
        Err(Ok(Error::NotAuthorized))
    );
    assert_eq!(
        s.client
            .try_vote_on_treasury_proposal(&voter, &pid_multi, &true),
        Err(Ok(Error::NotAuthorized))
    );
}

#[test]
fn treasury_withdrawal_error_parity_across_entrypoints() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let non_member = Address::generate(&s.env);
    let dest = Address::generate(&s.env);
    let self_dest = s.client.address.clone();
    let valid_reason = String::from_str(&s.env, "valid reason");
    let empty_reason = String::from_str(&s.env, "");
    let long_reason_str = "x".repeat(257);
    let long_reason = String::from_str(&s.env, &long_reason_str);

    // 1. Zero amount
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&proposer, &0, &dest, &valid_reason, &false),
        s.client.try_propose_withdrawal_in_asset(
            &proposer,
            &0,
            &s.token.address,
            &dest,
            &valid_reason,
            &false
        )
    );
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&proposer, &0, &dest, &valid_reason, &false),
        Err(Ok(Error::InvalidAmount))
    );

    // 2. Negative amount
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&proposer, &-100, &dest, &valid_reason, &false),
        s.client.try_propose_withdrawal_in_asset(
            &proposer,
            &-100,
            &s.token.address,
            &dest,
            &valid_reason,
            &false
        )
    );
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&proposer, &-100, &dest, &valid_reason, &false),
        Err(Ok(Error::InvalidAmount))
    );

    // 3. Self destination (destination == contract)
    assert_eq!(
        s.client.try_propose_treasury_withdrawal(
            &proposer,
            &100,
            &self_dest,
            &valid_reason,
            &false
        ),
        s.client.try_propose_withdrawal_in_asset(
            &proposer,
            &100,
            &s.token.address,
            &self_dest,
            &valid_reason,
            &false
        )
    );
    assert_eq!(
        s.client.try_propose_treasury_withdrawal(
            &proposer,
            &100,
            &self_dest,
            &valid_reason,
            &false
        ),
        Err(Ok(Error::InvalidProposal))
    );

    // 4. Empty reason
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&proposer, &100, &dest, &empty_reason, &false),
        s.client.try_propose_withdrawal_in_asset(
            &proposer,
            &100,
            &s.token.address,
            &dest,
            &empty_reason,
            &false
        )
    );
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&proposer, &100, &dest, &empty_reason, &false),
        Err(Ok(Error::InvalidProposal))
    );

    // 5. Oversized reason (> 256 bytes)
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&proposer, &100, &dest, &long_reason, &false),
        s.client.try_propose_withdrawal_in_asset(
            &proposer,
            &100,
            &s.token.address,
            &dest,
            &long_reason,
            &false
        )
    );
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&proposer, &100, &dest, &long_reason, &false),
        Err(Ok(Error::InvalidProposal))
    );

    // 6. Insufficient treasury (treasury balance is 3,000)
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&proposer, &5_000, &dest, &valid_reason, &false),
        s.client.try_propose_withdrawal_in_asset(
            &proposer,
            &5_000,
            &s.token.address,
            &dest,
            &valid_reason,
            &false
        )
    );
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&proposer, &5_000, &dest, &valid_reason, &false),
        Err(Ok(Error::InsufficientTreasury))
    );

    // 7. Non-member proposer
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&non_member, &100, &dest, &valid_reason, &false),
        s.client.try_propose_withdrawal_in_asset(
            &non_member,
            &100,
            &s.token.address,
            &dest,
            &valid_reason,
            &false
        )
    );
    assert_eq!(
        s.client
            .try_propose_treasury_withdrawal(&non_member, &100, &dest, &valid_reason, &false),
        Err(Ok(Error::NotMember))
    );
}
