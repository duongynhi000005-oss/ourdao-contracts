use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, BytesN, Env, String, Symbol, Vec};

use super::common::*;
use crate::admin::TIMELOCK_DURATION;
use crate::storage::ProposalKind;
use crate::{Error, OurDao, OurDaoClient};

#[test]
fn initialize_rejects_duplicate_admins() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin.clone());

    let res = client.try_initialize(&admins, &5_100u32, &FEE, &token_id, &policy());
    assert_eq!(res, Err(Ok(Error::AlreadyAdmin)));
}

#[test]
fn initialize_rejects_invalid_scalar_parameters() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let contract = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract);
    let admin = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin);

    assert_eq!(
        client.try_initialize(&admins, &0, &FEE, &token, &policy()),
        Err(Ok(Error::InvalidThreshold))
    );
    assert_eq!(
        client.try_initialize(&admins, &10_001, &FEE, &token, &policy()),
        Err(Ok(Error::InvalidThreshold))
    );
    assert_eq!(
        client.try_initialize(&admins, &5_100, &0, &token, &policy()),
        Err(Ok(Error::InvalidAmount))
    );
    let mut invalid_policy = policy();
    invalid_policy.max_loan_duration = 0;
    assert_eq!(
        client.try_initialize(&admins, &5_100, &FEE, &token, &invalid_policy),
        Err(Ok(Error::InvalidLoanPolicy))
    );
}

#[test]
fn pause_blocks_all_mutating_entrypoints() {
    // This test explicitly checks every mutating entrypoint to ensure pause()
    // prevents state changes. Every public entrypoint that mutates contract state
    // must be listed below with an assertion. If you add a new mutating entrypoint
    // without a pause decision, this test will fail — the property being protected
    // (an emergency stop actually stops everything) is too critical to check by
    // hand-written list.
    //
    // Entrypoints deliberately callable while paused:
    // - None. All state-changing operations should be blocked by pause().
    // - Note: repayment was discussed but decided to be pause-gated to maintain
    //   consistent emergency stop semantics (see issue #52).

    let s = setup(3);
    let borrower = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let staker = s.members.get(2).unwrap();
    let newcomer = Address::generate(&s.env);

    // Set up some proposals to vote on / interact with
    let loan_pid = s.client.request_loan(&borrower, &500, &None);
    advance(&s.env, EDITING + 1);

    let treasury_pid = s.client.propose_treasury_withdrawal(
        &borrower,
        &100,
        &newcomer,
        &String::from_str(&s.env, "test"),
        &false,
    );

    // Pause the contract
    s.client.pause(&s.admin);
    assert!(s.client.is_paused());

    // ==================== Membership ====================
    let res = s.client.try_register_member(&newcomer);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "register_member should be pause-gated"
    );

    let res = s.client.try_exit_dao(&borrower);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "exit_dao should be pause-gated"
    );

    let res = s.client.try_claim_rewards(&borrower);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "claim_rewards should be pause-gated"
    );

    // ==================== Loans ====================
    let res = s.client.try_request_loan(&borrower, &1000, &None);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "request_loan should be pause-gated"
    );

    let res = s.client.try_edit_loan_proposal(&borrower, &loan_pid, &600);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "edit_loan_proposal should be pause-gated"
    );

    let res = s.client.try_vote_on_loan_proposal(&voter, &loan_pid, &true);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "vote_on_loan_proposal should be pause-gated"
    );

    let res = s.client.try_disburse_approved_loan(&loan_pid);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "disburse_approved_loan should be pause-gated"
    );

    let res = s.client.try_repay_loan(&borrower, &0);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "repay_loan should be pause-gated"
    );

    let res = s.client.try_repay_loan_partial(&borrower, &0, &100);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "repay_loan_partial should be pause-gated"
    );

    let res = s.client.try_mark_loan_defaulted(&0);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "mark_loan_defaulted should be pause-gated"
    );

    let res = s.client.try_expire_loan_proposal(&0);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "expire_loan_proposal should be pause-gated"
    );

    // ==================== Treasury ====================
    let res = s.client.try_propose_treasury_withdrawal(
        &borrower,
        &100,
        &newcomer,
        &String::from_str(&s.env, "test"),
        &false,
    );
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "propose_treasury_withdrawal should be pause-gated"
    );

    let res = s
        .client
        .try_vote_on_treasury_proposal(&voter, &treasury_pid, &true);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "vote_on_treasury_proposal should be pause-gated"
    );

    let res = s.client.try_expire_treasury_proposal(&0);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "expire_treasury_proposal should be pause-gated"
    );

    let res = s.client.try_execute_treasury_proposal(&0);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "execute_treasury_proposal should be pause-gated"
    );

    // ==================== Staking ====================
    let res = s.client.try_stake(&staker, &100);
    assert_eq!(res, Err(Ok(Error::Paused)), "stake should be pause-gated");

    let res = s.client.try_unstake(&staker, &100);
    assert_eq!(res, Err(Ok(Error::Paused)), "unstake should be pause-gated");

    // ==================== Registry ====================
    let res = s
        .client
        .try_register_name(&borrower, &String::from_str(&s.env, "test"));
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "register_name should be pause-gated"
    );

    // ==================== Privacy (commit-reveal voting) ====================
    let commitment = BytesN::from_array(&s.env, &[0u8; 32]);
    let res = s
        .client
        .try_commit_treasury_vote(&voter, &treasury_pid, &commitment);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "commit_treasury_vote should be pause-gated"
    );

    let salt = BytesN::from_array(&s.env, &[0u8; 32]);
    let res = s
        .client
        .try_reveal_treasury_vote(&voter, &treasury_pid, &true, &salt);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "reveal_treasury_vote should be pause-gated"
    );

    // ==================== Docs (content-hash metadata) ====================
    let cid = Bytes::from_slice(&s.env, &[1, 2, 3]);
    let res = s
        .client
        .try_attach_document(&borrower, &ProposalKind::Loan, &loan_pid, &cid);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "attach_document should be pause-gated"
    );

    // Verify unpause works
    s.client.unpause(&s.admin);
    assert!(!s.client.is_paused());
}

#[test]
fn only_admin_can_unpause_even_when_a_dedicated_pauser_exists() {
    let s = setup(1);
    let pauser = Address::generate(&s.env);
    let outsider = Address::generate(&s.env);

    s.client.set_pauser(&s.admin, &pauser);
    s.client.pause(&pauser);
    assert!(s.client.is_paused());

    // The emergency pauser may stop the DAO but must not be able to resume it.
    assert_eq!(s.client.try_unpause(&pauser), Err(Ok(Error::NotAdmin)));
    assert!(
        s.client.is_paused(),
        "unauthorised unpause must not change state"
    );

    // An unrelated address must likewise be rejected without clearing the pause.
    assert_eq!(s.client.try_unpause(&outsider), Err(Ok(Error::NotAdmin)));
    assert!(s.client.is_paused());

    // Only the authenticated admin may resume normal operation.
    s.client.unpause(&s.admin);
    assert!(!s.client.is_paused());
    assert_eq!(s.client.try_unpause(&s.admin), Err(Ok(Error::NotPaused)));
}

#[test]
fn only_admin_governs() {
    let s = setup(1);
    let intruder = s.members.get(0).unwrap();
    let res = s.client.try_set_consensus_threshold(&intruder, &7_000);
    assert_eq!(res, Err(Ok(Error::NotAdmin)));

    s.client.set_consensus_threshold(&s.admin, &7_000);
    assert_eq!(s.client.get_consensus_threshold(), 7_000);
}

#[test]
fn set_consensus_threshold_rejects_out_of_range() {
    let s = setup(1);
    assert_eq!(
        s.client.try_set_consensus_threshold(&s.admin, &0),
        Err(Ok(Error::InvalidThreshold))
    );
    assert_eq!(
        s.client.try_set_consensus_threshold(&s.admin, &10_001),
        Err(Ok(Error::InvalidThreshold))
    );
}

#[test]
fn cannot_remove_last_admin() {
    let s = setup(0);
    let res = s.client.try_remove_admin(&s.admin, &s.admin);
    assert_eq!(res, Err(Ok(Error::CannotRemoveLastAdmin)));
}

// initialize token validation (#115)
fn init_with_token(env: &Env, token: &Address) -> Result<(), Error> {
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(env, &contract_id);
    let mut admins = Vec::new(env);
    admins.push_back(Address::generate(env));
    match client.try_initialize(&admins, &5_100u32, &FEE, token, &policy()) {
        Ok(_) => Ok(()),
        Err(Ok(e)) => Err(e),
        Err(Err(_)) => panic!("unexpected host error"),
    }
}

#[test]
fn initialize_rejects_account_address_as_token() {
    let env = Env::default();
    env.mock_all_auths();
    let not_a_contract = Address::generate(&env);
    assert_eq!(
        init_with_token(&env, &not_a_contract),
        Err(Error::InvalidToken)
    );
}

#[test]
fn initialize_rejects_contract_that_is_not_a_token() {
    let env = Env::default();
    env.mock_all_auths();
    // A real contract, but not a token: it has no `balance` entrypoint.
    let other = env.register(OurDao, ());
    assert_eq!(init_with_token(&env, &other), Err(Error::InvalidToken));
}

#[test]
fn initialize_accepts_a_real_token() {
    let env = Env::default();
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    assert_eq!(init_with_token(&env, &sac.address()), Ok(()));
}

// Issue #191: Configurable quorum threshold parameter in LoanPolicy
#[test]
fn validate_policy_rejects_quorum_bps_above_basis_points() {
    let mut p = policy();
    p.quorum_bps = 10_001; // > 10_000 BASIS_POINTS
    let s = setup(1);
    let res = s.client.try_propose_policy_update(&s.admin, &p);
    assert_eq!(res, Err(Ok(Error::InvalidLoanPolicy)));
}

// Issue #320: Validate quorum_bps within 1..=10000 on policy updates
#[test]
fn validate_policy_rejects_zero_quorum_bps() {
    let mut p = policy();
    p.quorum_bps = 0;
    let s = setup(1);
    let res = s.client.try_propose_policy_update(&s.admin, &p);
    assert_eq!(res, Err(Ok(Error::InvalidLoanPolicy)));
}

/// Every bound enforced by `admin::validate_policy` is exercised here. This
/// keeps policy validation from silently regressing when a new field is added
/// or an existing comparison is changed.
#[test]
fn propose_policy_update_rejects_every_invalid_bound() {
    let cases: [fn(&mut crate::types::LoanPolicy); 19] = [
        |p| p.membership_contribution = 0,
        |p| p.max_loan_duration = 0,
        |p| p.min_interest_rate = 10_001,
        |p| p.max_interest_rate = 10_001,
        |p| {
            p.min_interest_rate = p.max_interest_rate + 1;
        },
        |p| p.max_loan_to_treasury_ratio = 0,
        |p| p.max_loan_to_treasury_ratio = 10_001,
        |p| p.default_penalty_bps = 10_001,
        |p| p.editing_period = 0,
        |p| p.voting_period = 0,
        |p| p.editing_period = 30 * 24 * 60 * 60 + 1,
        |p| p.voting_period = 30 * 24 * 60 * 60 + 1,
        |p| p.min_membership_duration = 30 * 24 * 60 * 60 + 1,
        |p| p.cooldown_period = 30 * 24 * 60 * 60 + 1,
        |p| p.default_grace_period = 30 * 24 * 60 * 60 + 1,
        |p| p.treasury_threshold = 0,
        |p| p.treasury_threshold = 10_001,
        |p| p.quorum_bps = 0,
        |p| p.quorum_bps = 10_001,
    ];
    for mutate in cases {
        let s = setup(1);
        let mut invalid = policy();
        invalid.quorum_bps = 5_000;
        mutate(&mut invalid);
        assert_eq!(
            s.client.try_propose_policy_update(&s.admin, &invalid),
            Err(Ok(Error::InvalidLoanPolicy))
        );
    }
}

// Issue #192: Timelock delay for administrative policy changes
#[test]
fn policy_update_timelock_enforced() {
    let s = setup(3);
    let non_admin = s.members.get(0).unwrap();

    let mut new_p = policy();
    new_p.quorum_bps = 5_000;
    new_p.max_loan_duration = 60 * 24 * 60 * 60; // change duration
    new_p.quorum_bps = 5_100;

    // Non-admin cannot propose policy update
    let err = s.client.try_propose_policy_update(&non_admin, &new_p);
    assert_eq!(err, Err(Ok(Error::NotAdmin)));

    // Admin proposes update
    s.client.propose_policy_update(&s.admin, &new_p);

    let pending = s.client.get_pending_policy_update().unwrap();
    assert_eq!(pending.policy.max_loan_duration, 60 * 24 * 60 * 60);
    assert_eq!(
        pending.execution_time,
        pending.proposed_at + TIMELOCK_DURATION
    );

    // Attempting to execute immediately fails with TimelockNotExpired
    let early = s.client.try_execute_policy_update(&s.admin);
    assert_eq!(early, Err(Ok(Error::TimelockNotExpired)));

    // Advance 47 hours (not yet 48 hours)
    advance(&s.env, TIMELOCK_DURATION - 3600);
    let early2 = s.client.try_execute_policy_update(&s.admin);
    assert_eq!(early2, Err(Ok(Error::TimelockNotExpired)));

    // Non-admin cannot execute even after timelock
    advance(&s.env, 3601);
    let non_admin_exec = s.client.try_execute_policy_update(&non_admin);
    assert_eq!(non_admin_exec, Err(Ok(Error::NotAdmin)));

    // Admin executes after timelock delay
    s.client.execute_policy_update(&s.admin);

    // Policy is updated, pending update is cleared
    assert_eq!(
        s.client.get_loan_policy().max_loan_duration,
        60 * 24 * 60 * 60
    );
    assert!(s.client.get_pending_policy_update().is_none());

    // Subsequent execute fails because nothing is pending
    let none_pending = s.client.try_execute_policy_update(&s.admin);
    assert_eq!(none_pending, Err(Ok(Error::NoPendingPolicy)));
}

#[test]
fn policy_update_cancel_by_admin() {
    let s = setup(3);
    let non_admin = s.members.get(0).unwrap();

    let mut new_p = policy();
    new_p.quorum_bps = 5_000;
    new_p.min_interest_rate = 1_000;
    new_p.quorum_bps = 5_100;

    s.client.propose_policy_update(&s.admin, &new_p);
    assert!(s.client.get_pending_policy_update().is_some());

    // Non-admin cannot cancel
    let err = s.client.try_cancel_policy_update(&non_admin);
    assert_eq!(err, Err(Ok(Error::NotAdmin)));

    // Admin cancels
    s.client.cancel_policy_update(&s.admin);
    assert!(s.client.get_pending_policy_update().is_none());

    // Even after 48h, execute fails because it was cancelled
    advance(&s.env, TIMELOCK_DURATION + 1);
    let res = s.client.try_execute_policy_update(&s.admin);
    assert_eq!(res, Err(Ok(Error::NoPendingPolicy)));
}

#[test]
// Regression guard for #372: a release must expose the package version it was
// built from, rather than a separately maintained contract constant.
fn get_version_returns_cargo_pkg_version() {
    let env = Env::default();
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);

    let version = client.get_version();
    assert_eq!(version, String::from_str(&env, env!("CARGO_PKG_VERSION")));
}

// #339: the summary must agree field-for-field with the views it replaces.
// Staked funds stay out of the base-token treasury, and the base token is
// listed first among the whitelisted assets.
#[test]
fn get_dao_summary_matches_individual_views() {
    let s = setup(3);
    let staker = s.members.get(0).unwrap();
    s.client.stake(&staker, &500);
    let extra = whitelist_second_asset(&s, 700);

    let summary = s.client.get_dao_summary();
    assert_eq!(summary.total_members, s.client.get_total_members());
    assert_eq!(summary.active_members, s.client.get_active_members());
    assert_eq!(summary.treasury_balance, s.client.get_treasury_balance());
    assert_eq!(summary.total_staked, 500);
    assert_eq!(
        summary.consensus_threshold,
        s.client.get_consensus_threshold()
    );
    assert!(!summary.paused);
    assert_eq!(summary.version, s.client.get_version());

    assert_eq!(summary.treasury_balances.len(), 2);
    let base = summary.treasury_balances.get(0).unwrap();
    assert_eq!(base.asset, s.client.get_token());
    assert_eq!(base.balance, summary.treasury_balance);
    let second = summary.treasury_balances.get(1).unwrap();
    assert_eq!(second.asset, extra.address);
    assert_eq!(second.balance, 700);
    assert_eq!(
        second.balance,
        s.client.get_treasury_balance_in_asset(&extra.address)
    );

    s.client.pause(&s.admin);
    assert!(s.client.get_dao_summary().paused);
}

#[test]
fn test_bump_dao_ttl() {
    let s = setup(1);
    s.client.bump_dao_ttl();
}

// ==================== issue #236: token whitelist ====================

#[test]
fn token_whitelist_lifecycle() {
    let s = setup(1);
    let base = s.token.address.clone();
    let issuer = Address::generate(&s.env);
    let extra = s.env.register_stellar_asset_contract_v2(issuer).address();
    let outsider = s.members.get(0).unwrap();

    assert!(s.client.is_token_whitelisted(&base));
    assert!(!s.client.is_token_whitelisted(&extra));
    assert_eq!(
        s.client.get_whitelisted_tokens(),
        Vec::from_array(&s.env, [base.clone()])
    );

    assert_eq!(
        s.client.try_propose_token_addition(&outsider, &extra),
        Err(Ok(Error::NotAdmin))
    );
    assert_eq!(
        s.client.try_propose_token_addition(&s.admin, &base),
        Err(Ok(Error::TokenAlreadyWhitelisted))
    );
    assert_eq!(
        s.client
            .try_propose_token_addition(&s.admin, &Address::generate(&s.env)),
        Err(Ok(Error::InvalidToken))
    );
    assert_eq!(
        s.client.try_execute_token_addition(&s.admin, &extra),
        Err(Ok(Error::NoPendingTokenAddition))
    );

    s.client.propose_token_addition(&s.admin, &extra);
    assert_eq!(
        s.client.try_execute_token_addition(&s.admin, &extra),
        Err(Ok(Error::TimelockNotExpired))
    );
    advance(&s.env, TIMELOCK_DURATION);
    s.client.execute_token_addition(&s.admin, &extra);
    assert!(s.client.is_token_whitelisted(&extra));
    assert_eq!(
        s.client.get_whitelisted_tokens(),
        Vec::from_array(&s.env, [base.clone(), extra.clone()])
    );
    assert_eq!(s.client.get_treasury_balance_in_asset(&extra), 0);

    assert_eq!(
        s.client.try_remove_token(&s.admin, &base),
        Err(Ok(Error::CannotRemoveBaseToken))
    );
    s.client.remove_token(&s.admin, &extra);
    assert!(!s.client.is_token_whitelisted(&extra));
    assert_eq!(
        s.client.get_whitelisted_tokens(),
        Vec::from_array(&s.env, [base])
    );
    assert_eq!(
        s.client.try_remove_token(&s.admin, &extra),
        Err(Ok(Error::TokenNotWhitelisted))
    );
}

// Admin list handling uses host-side lookups (#357); the list stays a set.
#[test]
fn initialize_rejects_non_adjacent_duplicate_admins() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let client = OurDaoClient::new(&env, &env.register(OurDao, ()));

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let admins = Vec::from_array(&env, [a.clone(), b, a]);
    let res = client.try_initialize(&admins, &5_100u32, &FEE, &token_id, &policy());
    assert_eq!(res, Err(Ok(Error::AlreadyAdmin)));
}

#[test]
fn remove_admin_drops_only_the_target_and_keeps_order() {
    let s = setup(0);
    let second = Address::generate(&s.env);
    let third = Address::generate(&s.env);
    s.client.add_admin(&s.admin, &second);
    s.client.add_admin(&s.admin, &third);
    assert_eq!(
        s.client.try_add_admin(&s.admin, &second),
        Err(Ok(Error::AlreadyAdmin))
    );

    s.client.remove_admin(&s.admin, &second);
    assert_eq!(
        s.client.get_admins(),
        Vec::from_array(&s.env, [s.admin.clone(), third])
    );
    assert_eq!(
        s.client.try_remove_admin(&s.admin, &second),
        Err(Ok(Error::NotAdmin))
    );
}

// `DataKey` left the contract spec (#357); its ledger encoding must not move.
#[test]
fn storage_keys_keep_their_ledger_encoding() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    let admins = s.client.get_admins();
    s.env.as_contract(&s.client.address, || {
        let stored: Vec<Address> = s
            .env
            .storage()
            .instance()
            .get(&(Symbol::new(&s.env, "Admins"),))
            .unwrap();
        assert_eq!(stored, admins);
        assert!(s
            .env
            .storage()
            .persistent()
            .has(&(Symbol::new(&s.env, "Member"), member)));
    });
}

// ==================== issue #319: pauser cannot unpause ====================
#[test]
fn pauser_can_pause_but_cannot_unpause() {
    let s = setup(1);
    let pauser = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);

    // Non-admin cannot assign a pauser
    assert_eq!(
        s.client.try_set_pauser(&stranger, &pauser),
        Err(Ok(Error::NotAdmin))
    );

    // Admin sets pauser
    s.client.set_pauser(&s.admin, &pauser);
    // Setting same pauser again is rejected
    assert_eq!(
        s.client.try_set_pauser(&s.admin, &pauser),
        Err(Ok(Error::AlreadyPauser))
    );

    // Pauser can trigger pause
    assert!(!s.client.is_paused());
    s.client.pause(&pauser);
    assert!(s.client.is_paused());

    // Calling pause while already paused errors
    assert_eq!(s.client.try_pause(&pauser), Err(Ok(Error::Paused)));

    // Pauser explicitly CANNOT unpause (#319)
    assert_eq!(s.client.try_unpause(&pauser), Err(Ok(Error::NotAdmin)));
    assert!(s.client.is_paused());

    // Stranger cannot unpause
    assert_eq!(s.client.try_unpause(&stranger), Err(Ok(Error::NotAdmin)));
    assert!(s.client.is_paused());

    // Only admin can unpause
    s.client.unpause(&s.admin);
    assert!(!s.client.is_paused());

    // Calling unpause while not paused errors
    assert_eq!(s.client.try_unpause(&s.admin), Err(Ok(Error::NotPaused)));

    // Pauser can pause again
    s.client.pause(&pauser);
    assert!(s.client.is_paused());

    // Admin unpauses
    s.client.unpause(&s.admin);
    assert!(!s.client.is_paused());

    // Admin revokes pauser
    s.client.revoke_pauser(&s.admin);
    assert_eq!(
        s.client.try_revoke_pauser(&s.admin),
        Err(Ok(Error::NotPauser))
    );

    // Revoked pauser can no longer pause
    assert_eq!(s.client.try_pause(&pauser), Err(Ok(Error::NotAdmin)));
}

// ==================== issue #272: block remove_token while treasury holds asset ====================

#[test]
fn remove_token_while_treasury_holds_asset_rejected() {
    let s = setup(3);
    let asset = whitelist_second_asset(&s, 5_000);

    // Verify asset is whitelisted and has a positive treasury balance
    assert!(s.client.is_token_whitelisted(&asset.address));
    assert_eq!(
        s.client.get_treasury_balance_in_asset(&asset.address),
        5_000
    );

    // Removing a token with positive treasury balance must fail with TokenHasBalance
    let res = s.client.try_remove_token(&s.admin, &asset.address);
    assert_eq!(res, Err(Ok(Error::TokenHasBalance)));

    // Token must remain whitelisted after rejected removal
    assert!(s.client.is_token_whitelisted(&asset.address));

    // Drain treasury balance to zero
    asset.transfer(&s.client.address, &s.admin, &5_000);
    assert_eq!(s.client.get_treasury_balance_in_asset(&asset.address), 0);

    // Removing an empty token succeeds
    s.client.remove_token(&s.admin, &asset.address);
    assert!(!s.client.is_token_whitelisted(&asset.address));

    // Attempting to remove again returns TokenNotWhitelisted
    let res_again = s.client.try_remove_token(&s.admin, &asset.address);
    assert_eq!(res_again, Err(Ok(Error::TokenNotWhitelisted)));
}

#[test]
fn remove_token_with_zero_balance_succeeds_immediately() {
    let s = setup(3);
    let asset = whitelist_second_asset(&s, 0);

    assert!(s.client.is_token_whitelisted(&asset.address));
    assert_eq!(s.client.get_treasury_balance_in_asset(&asset.address), 0);

    s.client.remove_token(&s.admin, &asset.address);
    assert!(!s.client.is_token_whitelisted(&asset.address));
}
