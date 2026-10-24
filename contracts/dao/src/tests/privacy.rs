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
    let c1 = compute_commitment(&s.env, true, &salt1);
    let c2 = compute_commitment(&s.env, true, &salt2);

    s.client.commit_treasury_vote(&v1, &pid, &c1);
    s.client.commit_treasury_vote(&v2, &pid, &c2);

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
    let c1 = compute_commitment(&s.env, true, &salt);
    let c2 = compute_commitment(&s.env, false, &other);

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
    let commitment = compute_commitment(&s.env, true, &salt);
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
    let commitment = compute_commitment(&s.env, true, &salt);
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
    let commitment = compute_commitment(&s.env, true, &real_salt);
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
    let commitment = compute_commitment(&s.env, true, &salt);
    s.client.commit_treasury_vote(&voter, &pid, &commitment);

    advance(&s.env, VOTING_PERIOD + 1);

    // Committed true but reveals false — hash mismatch.
    let res = s
        .client
        .try_reveal_treasury_vote(&voter, &pid, &false, &salt);
    assert_eq!(res, Err(Ok(Error::CommitmentMismatch)));
}

// Known-answer vectors pin the commitment preimage, `[support] ++ salt`, that
// off-chain clients must reproduce. Computed independently with Python:
// hashlib.sha256(bytes([support]) + salt).hexdigest()
#[test]
fn commitment_matches_known_answer_vectors() {
    let env = soroban_sdk::Env::default();
    let cases: [(bool, [u8; 32], &str); 2] = [
        (
            true,
            [7u8; 32],
            "d3907c0247e7e98b72338a00d87244248df71eb313589da290d45adfba44e6d2",
        ),
        (
            false,
            core::array::from_fn(|i| i as u8),
            "699cacdb4c39d8e0bb1223352765a7f7acdc51dec6694f7b54c3d0a47f0cc409",
        ),
    ];
    for (support, salt, hex) in cases {
        let mut expected = [0u8; 32];
        for (i, byte) in expected.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
        }
        let got = compute_commitment(&env, support, &BytesN::from_array(&env, &salt));
        assert_eq!(got.to_array(), expected);
    }
}

// ===========================================================================
// Issue #298: Edge-case tests for commit-reveal voting
// ===========================================================================

#[test]
fn test_reveal_wrong_salt_returns_commitment_mismatch() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "private grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);

    let real_salt = BytesN::from_array(&s.env, &[11u8; 32]);
    let wrong_salt = BytesN::from_array(&s.env, &[22u8; 32]);
    let commitment = compute_commitment(&s.env, true, &real_salt);
    s.client.commit_treasury_vote(&voter, &pid, &commitment);

    advance(&s.env, VOTING_PERIOD + 1);

    // Attempting reveal with wrong salt fails with CommitmentMismatch
    let res = s
        .client
        .try_reveal_treasury_vote(&voter, &pid, &true, &wrong_salt);
    assert_eq!(res, Err(Ok(Error::CommitmentMismatch)));

    // Correct salt still succeeds after failed attempt
    s.client
        .reveal_treasury_vote(&voter, &pid, &true, &real_salt);
    assert!(s.client.has_voted(&ProposalKind::Treasury, &pid, &voter));
}

#[test]
fn test_reveal_after_period_returns_voting_ended() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "private grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);

    let salt = BytesN::from_array(&s.env, &[15u8; 32]);
    let commitment = compute_commitment(&s.env, true, &salt);
    s.client.commit_treasury_vote(&voter, &pid, &commitment);

    // Advance beyond the reveal window (commit_end + voting_period = 2 * VOTING_PERIOD)
    advance(&s.env, 2 * VOTING_PERIOD + 1);

    let res = s
        .client
        .try_reveal_treasury_vote(&voter, &pid, &true, &salt);
    assert_eq!(res, Err(Ok(Error::VotingEnded)));
}

#[test]
fn test_reveal_without_commit_returns_no_commitment() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "private grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);

    // Advance to reveal period without voter having committed
    advance(&s.env, VOTING_PERIOD + 1);

    let salt = BytesN::from_array(&s.env, &[33u8; 32]);
    let res = s
        .client
        .try_reveal_treasury_vote(&voter, &pid, &true, &salt);
    assert_eq!(res, Err(Ok(Error::NoCommitment)));
}

#[test]
fn test_reveal_by_different_address() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let outsider = Address::generate(&s.env);
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "private grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);

    let salt1 = BytesN::from_array(&s.env, &[77u8; 32]);
    let c1 = compute_commitment(&s.env, true, &salt1);
    s.client.commit_treasury_vote(&v1, &pid, &c1);

    advance(&s.env, VOTING_PERIOD + 1);

    // Member v2 did not commit, attempting to reveal returns NoCommitment
    let res_v2 = s.client.try_reveal_treasury_vote(&v2, &pid, &true, &salt1);
    assert_eq!(res_v2, Err(Ok(Error::NoCommitment)));

    // Non-member outsider attempting to reveal returns NotMember
    let res_outsider = s
        .client
        .try_reveal_treasury_vote(&outsider, &pid, &true, &salt1);
    assert_eq!(res_outsider, Err(Ok(Error::NotMember)));

    // Legitimate voter v1 can still reveal successfully
    s.client.reveal_treasury_vote(&v1, &pid, &true, &salt1);
    assert!(s.client.has_voted(&ProposalKind::Treasury, &pid, &v1));
}

#[test]
fn test_reveal_before_period_returns_not_yet_revealed() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "private grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);

    let salt = BytesN::from_array(&s.env, &[55u8; 32]);
    let commitment = compute_commitment(&s.env, true, &salt);
    s.client.commit_treasury_vote(&voter, &pid, &commitment);

    // Still in commit phase (timestamp <= commit_end)
    let res = s
        .client
        .try_reveal_treasury_vote(&voter, &pid, &true, &salt);
    assert_eq!(res, Err(Ok(Error::NotYetRevealed)));
}

#[test]
fn test_reveal_already_revealed_returns_already_revealed() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "private grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &dest, &reason, &true);

    let salt = BytesN::from_array(&s.env, &[66u8; 32]);
    let commitment = compute_commitment(&s.env, true, &salt);
    s.client.commit_treasury_vote(&voter, &pid, &commitment);

    advance(&s.env, VOTING_PERIOD + 1);

    // First reveal succeeds
    s.client.reveal_treasury_vote(&voter, &pid, &true, &salt);

    // Second reveal fails with AlreadyRevealed
    let res = s
        .client
        .try_reveal_treasury_vote(&voter, &pid, &true, &salt);
    assert_eq!(res, Err(Ok(Error::AlreadyRevealed)));
}
