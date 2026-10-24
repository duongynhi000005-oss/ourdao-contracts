use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, String};

use super::common::*;
use crate::privacy::compute_commitment;
use crate::storage::ProposalKind;
use crate::types::ProposalStatus;
use crate::Error;

#[test]
fn commit_reveal_private_treasury_vote() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "secret grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &600, &dest, &reason, &true);

    // Open voting is refused on a private proposal.
    let open = s.client.try_vote_on_treasury_proposal(&v1, &pid, &true);
    assert_eq!(open, Err(Ok(Error::NotAuthorized)));

    let salt1 = BytesN::from_array(&s.env, &[7u8; 32]);
    let salt2 = BytesN::from_array(&s.env, &[9u8; 32]);
    let c1 = compute_commitment(&s.env, &v1, pid, true, &salt1);
    let c2 = compute_commitment(&s.env, &v2, pid, true, &salt2);

    s.client.commit_treasury_vote(&v1, &pid, &c1);
    s.client.commit_treasury_vote(&v2, &pid, &c2);

    // Reveals are valid only after the commitment phase.
    advance(&s.env, VOTING_PERIOD + 1);

    // A reveal that doesn't match the commitment is rejected.
    let bad = s.client.try_reveal_treasury_vote(&v1, &pid, &false, &salt1);
    assert_eq!(bad, Err(Ok(Error::CommitmentMismatch)));

    s.client.reveal_treasury_vote(&v1, &pid, &true, &salt1);
    assert_eq!(
        s.client.get_treasury_proposal(&pid).unwrap().status,
        ProposalStatus::Pending
    );
    s.client.reveal_treasury_vote(&v2, &pid, &true, &salt2);
    assert_eq!(
        s.client.get_treasury_proposal(&pid).unwrap().status,
        ProposalStatus::Executed
    );
    assert_eq!(s.token.balance(&dest), 600);
}

#[test]
fn commit_vote_cannot_be_overwritten() {
    let s = setup(2);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "secret grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &600, &dest, &reason, &true);

    let salt = BytesN::from_array(&s.env, &[7u8; 32]);
    let other = BytesN::from_array(&s.env, &[9u8; 32]);
    let c1 = compute_commitment(&s.env, &voter, pid, true, &salt);
    let c2 = compute_commitment(&s.env, &voter, pid, false, &other);

    s.client.commit_treasury_vote(&voter, &pid, &c1);
    let again = s.client.try_commit_treasury_vote(&voter, &pid, &c2);
    assert_eq!(again, Err(Ok(Error::AlreadyVoted)));

    // The original commitment is still the one that reveals successfully.
    advance(&s.env, VOTING_PERIOD + 1);
    s.client.reveal_treasury_vote(&voter, &pid, &true, &salt);
}

#[test]
fn has_voted_treasury_commit_reveal() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");

    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &100, &dest, &reason, &true);

    // Before commit: not voted.
    assert!(!s.client.has_voted(&ProposalKind::Treasury, &pid, &v1));

    // After commit: voted (commitment counts as a vote for dedup).
    let salt = BytesN::from_array(&s.env, &[1u8; 32]);
    let commitment = compute_commitment(&s.env, &v1, pid, true, &salt);
    s.client.commit_treasury_vote(&v1, &pid, &commitment);
    assert!(s.client.has_voted(&ProposalKind::Treasury, &pid, &v1));
}

// ===========================================================================
// Issue #174: Verify reveal salt / commitment hash validation
// ===========================================================================

#[test]
fn reveal_with_correct_salt_tallies_vote() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "private grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);

    let salt = BytesN::from_array(&s.env, &[42u8; 32]);
    let commitment = compute_commitment(&s.env, &voter, pid, true, &salt);
    s.client.commit_treasury_vote(&voter, &pid, &commitment);

    advance(&s.env, VOTING_PERIOD + 1);

    // Correct salt reveals cleanly.
    s.client.reveal_treasury_vote(&voter, &pid, &true, &salt);
    assert!(s.client.has_voted(&ProposalKind::Treasury, &pid, &voter));
}

#[test]
fn reveal_with_mismatched_salt_returns_commitment_mismatch() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "private grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);

    let real_salt = BytesN::from_array(&s.env, &[1u8; 32]);
    let wrong_salt = BytesN::from_array(&s.env, &[2u8; 32]);
    let commitment = compute_commitment(&s.env, &voter, pid, true, &real_salt);
    s.client.commit_treasury_vote(&voter, &pid, &commitment);

    advance(&s.env, VOTING_PERIOD + 1);

    let res = s
        .client
        .try_reveal_treasury_vote(&voter, &pid, &true, &wrong_salt);
    assert_eq!(res, Err(Ok(Error::CommitmentMismatch)));
}

#[test]
fn reveal_with_wrong_support_returns_commitment_mismatch() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "private grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);

    let salt = BytesN::from_array(&s.env, &[7u8; 32]);
    let commitment = compute_commitment(&s.env, &voter, pid, true, &salt);
    s.client.commit_treasury_vote(&voter, &pid, &commitment);

    advance(&s.env, VOTING_PERIOD + 1);

    // Committed true but reveals false — hash mismatch.
    let res = s
        .client
        .try_reveal_treasury_vote(&voter, &pid, &false, &salt);
    assert_eq!(res, Err(Ok(Error::CommitmentMismatch)));
}

#[test]
fn commitments_bind_voter_and_proposal() {
    let env = soroban_sdk::Env::default();
    let voter = Address::generate(&env);
    let copier = Address::generate(&env);
    let salt = BytesN::from_array(&env, &[7u8; 32]);
    let c = compute_commitment(&env, &voter, 3, true, &salt);
    assert_eq!(c, compute_commitment(&env, &voter, 3, true, &salt));
    assert_ne!(c, compute_commitment(&env, &copier, 3, true, &salt));
    assert_ne!(c, compute_commitment(&env, &voter, 4, true, &salt));
    assert_ne!(c, compute_commitment(&env, &voter, 3, false, &salt));
}

#[test]
fn copied_commitment_cannot_be_revealed_by_another_voter() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let first = s.members.get(1).unwrap();
    let copier = s.members.get(2).unwrap();
    let destination = Address::generate(&s.env);
    let pid = s.client.propose_treasury_withdrawal(
        &proposer,
        &300,
        &destination,
        &String::from_str(&s.env, "private"),
        &true,
    );
    let salt = BytesN::from_array(&s.env, &[13u8; 32]);
    let commitment = compute_commitment(&s.env, &first, pid, true, &salt);
    s.client.commit_treasury_vote(&first, &pid, &commitment);
    s.client.commit_treasury_vote(&copier, &pid, &commitment);
    advance(&s.env, VOTING_PERIOD + 1);
    assert_eq!(
        s.client
            .try_reveal_treasury_vote(&copier, &pid, &true, &salt),
        Err(Ok(Error::CommitmentMismatch)),
    );
    s.client.reveal_treasury_vote(&first, &pid, &true, &salt);
}

// ===========================================================================
// Issue #299: Boundary tests at exactly the commit/reveal deadlines
// ===========================================================================

#[test]
fn test_privacy_commit_deadline_boundaries() {
    let s = setup(4);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let v3 = s.members.get(3).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "private withdrawal");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);

    let salt1 = BytesN::from_array(&s.env, &[11u8; 32]);
    let salt2 = BytesN::from_array(&s.env, &[22u8; 32]);
    let salt3 = BytesN::from_array(&s.env, &[33u8; 32]);
    let c1 = compute_commitment(&s.env, &v1, pid, true, &salt1);
    let c2 = compute_commitment(&s.env, &v2, pid, true, &salt2);
    let c3 = compute_commitment(&s.env, &v3, pid, true, &salt3);

    // 1. Boundary: commit_end - 1
    advance(&s.env, VOTING_PERIOD - 1);
    s.client.commit_treasury_vote(&v1, &pid, &c1);
    let early_reveal = s.client.try_reveal_treasury_vote(&v1, &pid, &true, &salt1);
    assert_eq!(early_reveal, Err(Ok(Error::NotYetRevealed)));

    // 2. Boundary: exactly commit_end
    advance(&s.env, 1);
    s.client.commit_treasury_vote(&v2, &pid, &c2);
    let exact_reveal = s.client.try_reveal_treasury_vote(&v2, &pid, &true, &salt2);
    assert_eq!(exact_reveal, Err(Ok(Error::NotYetRevealed)));

    // 3. Boundary: commit_end + 1
    advance(&s.env, 1);
    let late_commit = s.client.try_commit_treasury_vote(&v3, &pid, &c3);
    assert_eq!(late_commit, Err(Ok(Error::VotingEnded)));

    // Revealed vote succeeds in the reveal phase
    s.client.reveal_treasury_vote(&v1, &pid, &true, &salt1);
    assert!(s.client.has_voted(&ProposalKind::Treasury, &pid, &v1));
}

#[test]
fn test_privacy_reveal_deadline_boundaries() {
    let s = setup(4);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let v3 = s.members.get(3).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "private withdrawal");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);

    let salt1 = BytesN::from_array(&s.env, &[41u8; 32]);
    let salt2 = BytesN::from_array(&s.env, &[42u8; 32]);
    let salt3 = BytesN::from_array(&s.env, &[43u8; 32]);
    let c1 = compute_commitment(&s.env, &v1, pid, true, &salt1);
    let c2 = compute_commitment(&s.env, &v2, pid, true, &salt2);
    let c3 = compute_commitment(&s.env, &v3, pid, true, &salt3);

    // Commit all votes during commit period
    s.client.commit_treasury_vote(&v1, &pid, &c1);
    s.client.commit_treasury_vote(&v2, &pid, &c2);
    s.client.commit_treasury_vote(&v3, &pid, &c3);

    // 1. Boundary: reveal_end - 1
    advance(&s.env, 2 * VOTING_PERIOD - 1);
    s.client.reveal_treasury_vote(&v1, &pid, &true, &salt1);
    assert!(s.client.has_voted(&ProposalKind::Treasury, &pid, &v1));

    // 2. Boundary: exactly reveal_end
    advance(&s.env, 1);
    s.client.reveal_treasury_vote(&v2, &pid, &true, &salt2);
    assert!(s.client.has_voted(&ProposalKind::Treasury, &pid, &v2));

    // 3. Boundary: reveal_end + 1
    advance(&s.env, 1);
    let late_reveal = s.client.try_reveal_treasury_vote(&v3, &pid, &true, &salt3);
    assert_eq!(late_reveal, Err(Ok(Error::VotingEnded)));
}
