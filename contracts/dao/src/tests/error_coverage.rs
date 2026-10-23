/// Comprehensive error variant coverage tests
/// Each test ensures its corresponding Error variant is reachable

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, String, Vec};

use crate::error::Error;
use crate::types::LoanPolicy;
use crate::{OurDao, OurDaoClient};

use super::common::{advance, policy, setup, DAOBuilder, EDITING, FEE, VOTING_PERIOD};

// AlreadyInitialized = 1
#[test]
fn error_already_initialized() {
    let s = setup(1);
    let token = s.token.address;
    let admin = s.admin.clone();

    let mut admins = Vec::new(&s.env);
    admins.push_back(admin);

    let result = s.client.try_initialize(&admins, &5_100u32, &FEE, &token, &policy());
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

// NotInitialized = 2
#[test]
fn error_not_initialized() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();

    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    let result = client.try_register_member(&admin);
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
}

// InvalidThreshold = 3
#[test]
fn error_invalid_threshold() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_id = sac.address();

    let admin = Address::generate(&env);
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);

    let mut admins = Vec::new(&env);
    admins.push_back(admin);

    let pol = policy();
    let result = client.try_initialize(&admins, &0u32, &FEE, &token_id, &pol);
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
}

// InvalidAmount = 4
#[test]
fn error_invalid_amount() {
    let s = setup(1);
    let result = s.client.try_request_loan(&s.members[0], &0i128, &None);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

// InvalidLoanPolicy = 5
#[test]
fn error_invalid_loan_policy() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_id = sac.address();

    let admin = Address::generate(&env);
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);

    let mut admins = Vec::new(&env);
    admins.push_back(admin);

    let mut pol = policy();
    pol.min_interest_rate = 3_000;
    pol.max_interest_rate = 1_000;

    let result = client.try_initialize(&admins, &5_100u32, &FEE, &token_id, &pol);
    assert_eq!(result, Err(Ok(Error::InvalidLoanPolicy)));
}

// Paused = 6
#[test]
fn error_paused() {
    let s = setup(1);
    s.client.pause(&s.admin);

    let result = s.client.try_register_member(&Address::generate(&s.env));
    assert_eq!(result, Err(Ok(Error::Paused)));
}

// NotPaused = 7
#[test]
fn error_not_paused() {
    let s = setup(1);
    let result = s.client.try_unpause(&s.admin);
    assert_eq!(result, Err(Ok(Error::NotPaused)));
}

// NotAuthorized = 10
#[test]
fn error_not_authorized() {
    let env = soroban_sdk::Env::default();
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_id = sac.address();

    let admin = Address::generate(&env);
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);

    let mut admins = Vec::new(&env);
    admins.push_back(admin);
    client.initialize(&admins, &5_100u32, &FEE, &token_id, &policy());

    let unauthorized = Address::generate(&env);
    let result = client.try_pause(&unauthorized);
    assert_eq!(result, Err(Ok(Error::NotAuthorized)));
}

// NotAdmin = 11
#[test]
fn error_not_admin() {
    let s = setup(2);
    let member = s.members[0].clone();

    let result = s.client.try_pause(&member);
    assert_eq!(result, Err(Ok(Error::NotAdmin)));
}

// NotMember = 12
#[test]
fn error_not_member() {
    let s = setup(0);
    let non_member = Address::generate(&s.env);

    let result = s.client.try_request_loan(&non_member, &100i128, &None);
    assert_eq!(result, Err(Ok(Error::NotMember)));
}

// AlreadyAdmin = 13
#[test]
fn error_already_admin() {
    let s = setup(1);
    let result = s.client.try_add_admin(&s.admin, &s.admin);
    assert_eq!(result, Err(Ok(Error::AlreadyAdmin)));
}

// AlreadyMember = 14
#[test]
fn error_already_member() {
    let s = setup(1);
    let member = s.members[0].clone();
    let result = s.client.try_register_member(&member);
    assert_eq!(result, Err(Ok(Error::AlreadyMember)));
}

// CannotRemoveLastAdmin = 15
#[test]
fn error_cannot_remove_last_admin() {
    let s = setup(0);
    let result = s.client.try_remove_admin(&s.admin, &s.admin);
    assert_eq!(result, Err(Ok(Error::CannotRemoveLastAdmin)));
}

// MemberNotActive = 20
#[test]
fn error_member_not_active() {
    let s = setup(1);
    let member = s.members[0].clone();

    s.client.exit_dao(&member);

    let result = s.client.try_request_loan(&member, &100i128, &None);
    assert_eq!(result, Err(Ok(Error::MemberNotActive)));
}

// HasActiveLoan = 21
#[test]
fn error_has_active_loan() {
    let s = setup(2);
    let borrower = s.members[0].clone();
    let voter = s.members[1].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&voter, &proposal_id, &true);

    advance(&s.env, VOTING_PERIOD + 1);
    s.client.disburse_approved_loan(&borrower, &proposal_id);

    let result = s.client.try_request_loan(&borrower, &100i128, &None);
    assert_eq!(result, Err(Ok(Error::HasActiveLoan)));
}

// ProposalNotFound = 30
#[test]
fn error_proposal_not_found() {
    let s = setup(1);
    let result = s.client.try_vote_on_loan_proposal(&s.members[0], &9999u32, &true);
    assert_eq!(result, Err(Ok(Error::ProposalNotFound)));
}

// NotBorrower = 31
#[test]
fn error_not_borrower() {
    let s = setup(2);
    let borrower = s.members[0].clone();
    let non_borrower = s.members[1].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    let result = s.client.try_cancel_loan_proposal(&non_borrower, &proposal_id);
    assert_eq!(result, Err(Ok(Error::NotBorrower)));
}

// NotInEditingPhase = 32
#[test]
fn error_not_in_editing_phase() {
    let s = setup(1);
    let borrower = s.members[0].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    advance(&s.env, EDITING + 1);
    let result = s.client.try_edit_loan_proposal(&borrower, &proposal_id, &String::from_str(&s.env, "new_cid"));
    assert_eq!(result, Err(Ok(Error::NotInEditingPhase)));
}

// NotInVotingPhase = 33
#[test]
fn error_not_in_voting_phase() {
    let s = setup(1);
    let borrower = s.members[0].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    advance(&s.env, VOTING_PERIOD + EDITING + 1);

    let result = s.client.try_vote_on_loan_proposal(&borrower, &proposal_id, &true);
    assert_eq!(result, Err(Ok(Error::NotInVotingPhase)));
}

// VotingEnded = 34
#[test]
fn error_voting_ended() {
    let s = setup(2);
    let borrower = s.members[0].clone();
    let voter = s.members[1].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    advance(&s.env, VOTING_PERIOD + EDITING + 1);

    let result = s.client.try_vote_on_loan_proposal(&voter, &proposal_id, &true);
    assert_eq!(result, Err(Ok(Error::VotingEnded)));
}

// AlreadyVoted = 35
#[test]
fn error_already_voted() {
    let s = setup(2);
    let borrower = s.members[0].clone();
    let voter = s.members[1].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&voter, &proposal_id, &true);

    let result = s.client.try_vote_on_loan_proposal(&voter, &proposal_id, &false);
    assert_eq!(result, Err(Ok(Error::AlreadyVoted)));
}

// NotEligibleForLoan = 36
#[test]
fn error_not_eligible_for_loan() {
    let policy_with_min_duration = LoanPolicy {
        min_membership_duration: 1000u64,
        ..policy()
    };

    let env = soroban_sdk::Env::default();
    env.mock_all_auths();
    let s = DAOBuilder::new()
        .with_members(1)
        .with_policy(policy_with_min_duration)
        .with_env(env)
        .build();

    let member = s.members[0].clone();
    let result = s.client.try_request_loan(&member, &100i128, &None);
    assert_eq!(result, Err(Ok(Error::NotEligibleForLoan)));
}

// CooldownActive = 37
#[test]
fn error_cooldown_active() {
    let cooldown_policy = LoanPolicy {
        cooldown_period: 1000u64,
        ..policy()
    };

    let env = soroban_sdk::Env::default();
    env.mock_all_auths();
    let s = DAOBuilder::new()
        .with_members(2)
        .with_policy(cooldown_policy)
        .with_env(env)
        .build();

    let borrower = s.members[0].clone();
    let voter = s.members[1].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&voter, &proposal_id, &true);

    advance(&s.env, VOTING_PERIOD + 1);
    s.client.disburse_approved_loan(&borrower, &proposal_id);

    let result = s.client.try_request_loan(&borrower, &100i128, &None);
    assert_eq!(result, Err(Ok(Error::CooldownActive)));
}

// LoanNotFound = 38
#[test]
fn error_loan_not_found() {
    let s = setup(1);
    let result = s.client.try_repay_loan(&s.members[0], &9999u32);
    assert_eq!(result, Err(Ok(Error::LoanNotFound)));
}

// LoanNotActive = 39
#[test]
fn error_loan_not_active() {
    let s = setup(2);
    let borrower = s.members[0].clone();
    let voter = s.members[1].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&voter, &proposal_id, &true);
    advance(&s.env, VOTING_PERIOD + 1);
    s.client.disburse_approved_loan(&borrower, &proposal_id);

    s.client.repay_loan(&borrower, &proposal_id);

    let result = s.client.try_repay_loan(&borrower, &proposal_id);
    assert_eq!(result, Err(Ok(Error::LoanNotActive)));
}

// ExceedsTreasuryRatio = 40
#[test]
fn error_exceeds_treasury_ratio() {
    let s = setup(1);
    let borrower = s.members[0].clone();

    let result = s.client.try_request_loan(&borrower, &1_000_000_000i128, &None);
    assert_eq!(result, Err(Ok(Error::ExceedsTreasuryRatio)));
}

// InsufficientTreasury = 41
#[test]
fn error_insufficient_treasury() {
    let s = setup(2);
    let borrower = s.members[0].clone();
    let voter = s.members[1].clone();

    let proposal_id = s.client.request_loan(&borrower, &5_100_000i128, &None);

    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&voter, &proposal_id, &true);
    advance(&s.env, VOTING_PERIOD + 1);

    let result = s.client.try_disburse_approved_loan(&borrower, &proposal_id);
    assert_eq!(result, Err(Ok(Error::InsufficientTreasury)));
}

// LoanNotOverdue = 42
#[test]
fn error_loan_not_overdue() {
    let s = setup(2);
    let borrower = s.members[0].clone();
    let voter = s.members[1].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&voter, &proposal_id, &true);
    advance(&s.env, VOTING_PERIOD + 1);
    s.client.disburse_approved_loan(&borrower, &proposal_id);

    let result = s.client.try_mark_loan_defaulted(&s.admin, &proposal_id);
    assert_eq!(result, Err(Ok(Error::LoanNotOverdue)));
}

// TreasuryProposalNotFound = 50
#[test]
fn error_treasury_proposal_not_found() {
    let s = setup(1);
    let newcomer = Address::generate(&s.env);
    let result = s.client.try_vote_on_treasury_proposal(&s.members[0], &9999u32, &true);
    assert_eq!(result, Err(Ok(Error::TreasuryProposalNotFound)));
}

// NameTaken = 60
#[test]
fn error_name_taken() {
    let s = setup(1);
    let member = s.members[0].clone();

    s.client.register_name(&member, &String::from_str(&s.env, "test_name"));

    let result = s.client.try_register_name(&member, &String::from_str(&s.env, "test_name"));
    assert_eq!(result, Err(Ok(Error::NameTaken)));
}

// NameNotFound = 61
#[test]
fn error_name_not_found() {
    let s = setup(1);
    let result = s.client.try_resolve_name(&String::from_str(&s.env, "nonexistent"));
    assert_eq!(result, Err(Ok(Error::NameNotFound)));
}

// NoStake = 62
#[test]
fn error_no_stake() {
    let s = setup(1);
    let member = s.members[0].clone();

    let result = s.client.try_claim_stake_reward(&member);
    assert_eq!(result, Err(Ok(Error::NoStake)));
}

// InsufficientStake = 63
#[test]
fn error_insufficient_stake() {
    let s = setup(1);
    let member = s.members[0].clone();

    s.client.stake_voting_weight(&member, &1i128);

    let result = s.client.try_stake_voting_weight(&member, &-2i128);
    assert_eq!(result, Err(Ok(Error::InsufficientStake)));
}

// NoCommitment = 64
#[test]
fn error_no_commitment() {
    let s = setup(1);
    let result = s.client.try_reveal_commitment(&s.members[0], &String::from_str(&s.env, "secret"));
    assert_eq!(result, Err(Ok(Error::NoCommitment)));
}

// CommitmentMismatch = 65
#[test]
fn error_commitment_mismatch() {
    let s = setup(1);
    let member = s.members[0].clone();

    let hash = soroban_sdk::crypto::Sha256::hash(&s.env, &String::from_str(&s.env, "secret").into());
    s.client.commit_to_vote(&member, &hash);

    let result = s.client.try_reveal_commitment(&member, &String::from_str(&s.env, "wrong_secret"));
    assert_eq!(result, Err(Ok(Error::CommitmentMismatch)));
}

// AlreadyRevealed = 66
#[test]
fn error_already_revealed() {
    let s = setup(1);
    let member = s.members[0].clone();

    let hash = soroban_sdk::crypto::Sha256::hash(&s.env, &String::from_str(&s.env, "secret").into());
    s.client.commit_to_vote(&member, &hash);
    s.client.reveal_commitment(&member, &String::from_str(&s.env, "secret"));

    let result = s.client.try_reveal_commitment(&member, &String::from_str(&s.env, "secret"));
    assert_eq!(result, Err(Ok(Error::AlreadyRevealed)));
}

// NothingToClaim = 67
#[test]
fn error_nothing_to_claim() {
    let s = setup(1);
    let result = s.client.try_claim_stake_reward(&s.members[0]);
    assert_eq!(result, Err(Ok(Error::NothingToClaim)));
}

// ProposalNotExpired = 70
#[test]
fn error_proposal_not_expired() {
    let s = setup(1);
    let borrower = s.members[0].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    let result = s.client.try_expire_loan_proposal(&borrower, &proposal_id);
    assert_eq!(result, Err(Ok(Error::ProposalNotExpired)));
}

// InvalidName = 71
#[test]
fn error_invalid_name() {
    let s = setup(1);
    let result = s.client.try_register_name(&s.members[0], &String::from_str(&s.env, ""));
    assert_eq!(result, Err(Ok(Error::InvalidName)));
}

// NotYetRevealed = 72
#[test]
fn error_not_yet_revealed() {
    let s = setup(1);
    let member = s.members[0].clone();

    let hash = soroban_sdk::crypto::Sha256::hash(&s.env, &String::from_str(&s.env, "secret").into());
    s.client.commit_to_vote(&member, &hash);

    let result = s.client.try_claim_stake_reward(&member);
    assert_eq!(result, Err(Ok(Error::NotYetRevealed)));
}

// NotProposalOwner = 73
#[test]
fn error_not_proposal_owner() {
    let s = setup(2);
    let borrower = s.members[0].clone();
    let other = s.members[1].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    let result = s.client.try_edit_loan_proposal(&other, &proposal_id, &String::from_str(&s.env, "new_cid"));
    assert_eq!(result, Err(Ok(Error::NotProposalOwner)));
}

// DocumentTooLarge = 74
#[test]
fn error_document_too_large() {
    let s = setup(1);
    let member = s.members[0].clone();

    let large_doc = String::from_str(&s.env, &"x".repeat(100_000));

    let result = s.client.try_request_loan(&member, &100i128, &Some(large_doc));
    assert_eq!(result, Err(Ok(Error::DocumentTooLarge)));
}

// InvalidToken = 75
#[test]
fn error_invalid_token() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);

    let invalid_token = Address::generate(&env);

    let mut admins = Vec::new(&env);
    admins.push_back(admin);

    let result = client.try_initialize(&admins, &5_100u32, &FEE, &invalid_token, &policy());
    assert_eq!(result, Err(Ok(Error::InvalidToken)));
}

// TimelockNotExpired = 76
#[test]
fn error_timelock_not_expired() {
    let s = setup(0);

    let new_policy = policy();
    s.client.propose_policy_update(&s.admin, &new_policy);

    let result = s.client.try_execute_policy_update(&s.admin);
    assert_eq!(result, Err(Ok(Error::TimelockNotExpired)));
}

// NoPendingPolicy = 77
#[test]
fn error_no_pending_policy() {
    let s = setup(0);

    let result = s.client.try_execute_policy_update(&s.admin);
    assert_eq!(result, Err(Ok(Error::NoPendingPolicy)));
}

// InvalidMetadataCid = 78
#[test]
fn error_invalid_metadata_cid() {
    let s = setup(1);
    let result = s.client.try_request_loan(&s.members[0], &100i128, &Some(String::from_str(&s.env, "invalid")));
    assert_eq!(result, Err(Ok(Error::InvalidMetadataCid)));
}

// InvalidDelegation = 79
#[test]
fn error_invalid_delegation() {
    let s = setup(1);
    let member = s.members[0].clone();

    let result = s.client.try_delegate_vote(&member, &member);
    assert_eq!(result, Err(Ok(Error::InvalidDelegation)));
}

// InvalidProposal = 80
#[test]
fn error_invalid_proposal() {
    let s = setup(1);
    let result = s.client.try_request_loan(&s.members[0], &100i128, &Some(String::from_str(&s.env, "ipfs://test")));
    assert_eq!(result, Err(Ok(Error::InvalidProposal)));
}

// AlreadyPauser = 81
#[test]
fn error_already_pauser() {
    let s = setup(0);
    s.client.set_pauser(&s.admin, &s.admin);

    let result = s.client.try_set_pauser(&s.admin, &s.admin);
    assert_eq!(result, Err(Ok(Error::AlreadyPauser)));
}

// NotPauser = 82
#[test]
fn error_not_pauser() {
    let s = setup(1);
    let non_pauser = s.members[0].clone();

    let result = s.client.try_revoke_pauser(&s.admin, &non_pauser);
    assert_eq!(result, Err(Ok(Error::NotPauser)));
}

// TokenNotWhitelisted = 83
#[test]
fn error_token_not_whitelisted() {
    let s = setup(1);
    let member = s.members[0].clone();
    let unwhitelisted_token = Address::generate(&s.env);

    let result = s.client.try_repay_loan_in_asset(&member, &0u32, &unwhitelisted_token);
    assert_eq!(result, Err(Ok(Error::TokenNotWhitelisted)));
}

// TokenAlreadyWhitelisted = 84
#[test]
fn error_token_already_whitelisted() {
    let s = setup(0);
    let base_token = s.token.address;

    let result = s.client.try_add_token_to_whitelist(&s.admin, &base_token);
    assert_eq!(result, Err(Ok(Error::TokenAlreadyWhitelisted)));
}

// NoPendingTokenAddition = 85
#[test]
fn error_no_pending_token_addition() {
    let s = setup(0);
    let token = Address::generate(&s.env);

    let result = s.client.try_execute_token_addition(&s.admin, &token);
    assert_eq!(result, Err(Ok(Error::NoPendingTokenAddition)));
}

// AssetMismatch = 86
#[test]
fn error_asset_mismatch() {
    let s = setup(2);
    let borrower = s.members[0].clone();
    let voter = s.members[1].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&voter, &proposal_id, &true);
    advance(&s.env, VOTING_PERIOD + 1);
    s.client.disburse_approved_loan(&borrower, &proposal_id);

    let wrong_token = Address::generate(&s.env);

    let result = s.client.try_repay_loan_in_asset(&borrower, &proposal_id, &wrong_token);
    assert_eq!(result, Err(Ok(Error::AssetMismatch)));
}

// CannotRemoveBaseToken = 87
#[test]
fn error_cannot_remove_base_token() {
    let s = setup(0);
    let base_token = s.token.address;

    let result = s.client.try_remove_token(&s.admin, &base_token);
    assert_eq!(result, Err(Ok(Error::CannotRemoveBaseToken)));
}

// SeedingLocked = 88
#[test]
fn error_seeding_locked() {
    let s = setup(0);
    s.client.lock_seeding(&s.admin);

    let member = Address::generate(&s.env);
    let result = s.client.try_register_member(&member);
    assert_eq!(result, Err(Ok(Error::SeedingLocked)));
}

// SeedingAlreadyLocked = 89
#[test]
fn error_seeding_already_locked() {
    let s = setup(0);
    s.client.lock_seeding(&s.admin);

    let result = s.client.try_lock_seeding(&s.admin);
    assert_eq!(result, Err(Ok(Error::SeedingAlreadyLocked)));
}

// TooManyOpenProposals = 90
#[test]
fn error_too_many_open_proposals() {
    let s = setup(10);
    let borrower = s.members[0].clone();

    for i in 0..10 {
        let cid = format!("QmVeryLongMetadataHashForTest{:02}", i);
        let _ = s.client.request_loan(&borrower, &100i128, &Some(String::from_str(&s.env, &cid)));
    }

    let result = s.client.try_request_loan(&borrower, &100i128, &Some(String::from_str(&s.env, "QmOverflow")));
    assert_eq!(result, Err(Ok(Error::TooManyOpenProposals)));
}

// BorrowerCannotVote = 91
#[test]
fn error_borrower_cannot_vote() {
    let s = setup(2);
    let borrower = s.members[0].clone();

    let proposal_id = s.client.request_loan(&borrower, &100i128, &None);

    advance(&s.env, EDITING + 1);

    let result = s.client.try_vote_on_loan_proposal(&borrower, &proposal_id, &true);
    assert_eq!(result, Err(Ok(Error::BorrowerCannotVote)));
}


// MaxWhitelistedTokensReached = 92
#[test]
fn error_max_whitelisted_tokens_reached() {
    let s = setup(1);
    for _ in 0..9 {
        let issuer = Address::generate(&s.env);
        let extra = s.env.register_stellar_asset_contract_v2(issuer).address();
        s.client.propose_token_addition(&s.admin, &extra);
        advance(&s.env, crate::admin::TIMELOCK_DURATION);
        s.client.execute_token_addition(&s.admin, &extra);
    }
    let overflow = Address::generate(&s.env);
    let sac = s.env.register_stellar_asset_contract_v2(overflow).address();
    let result = s.client.try_propose_token_addition(&s.admin, &sac);
    assert_eq!(result, Err(Ok(Error::MaxWhitelistedTokensReached)));
}
