use super::common::*;
use crate::Error;

// ==================== issue #309: tests for propose/execute/cancel_policy_update time-lock ====================
#[test]
fn policy_update_timelock_propose_and_execute() {
    let s = setup(2);
    let mut policy = s.client.get_loan_policy();
    policy.quorum_bps = 5_000;
    policy.max_interest_rate = 1_200;

    // Propose a policy update
    s.client.propose_policy_update(&s.admin, &policy);

    // Immediate execution should fail (timelock not expired)
    let res = s.client.try_execute_policy_update(&s.admin);
    assert_eq!(res, Err(Ok(Error::TimelockNotExpired)));

    // Advance past timelock
    advance(&s.env, crate::admin::TIMELOCK_DURATION);
    s.client.execute_policy_update(&s.admin);

    let updated = s.client.get_loan_policy();
    assert_eq!(updated.max_interest_rate, 1_200);
}

#[test]
fn policy_update_timelock_cancel_before_execution() {
    let s = setup(2);
    let mut policy = s.client.get_loan_policy();
    policy.quorum_bps = 5_000;
    policy.min_membership_duration = 36 * 60 * 60;

    // Propose a policy update
    s.client.propose_policy_update(&s.admin, &policy);

    // Cancel before timelock expires
    s.client.cancel_policy_update(&s.admin);
    advance(&s.env, 1);

    // Should not be able to execute cancelled update
    let res = s.client.try_execute_policy_update(&s.admin);
    assert_eq!(res, Err(Ok(Error::NoPendingPolicy)));
}

#[test]
fn policy_update_timelock_block_after_execution() {
    let s = setup(2);
    let mut policy = s.client.get_loan_policy();
    policy.quorum_bps = 5_000;
    policy.max_loan_to_treasury_ratio = 6_000;

    // Propose and execute
    s.client.propose_policy_update(&s.admin, &policy);
    advance(&s.env, crate::admin::TIMELOCK_DURATION);
    s.client.execute_policy_update(&s.admin);

    // Another propose should work (new update can be proposed)
    let mut policy2 = s.client.get_loan_policy();
    policy2.quorum_bps = 5_000;
    policy2.min_membership_duration = 72 * 60 * 60;
    s.client.propose_policy_update(&s.admin, &policy2);

    // This should be approved and executed normally
    advance(&s.env, crate::admin::TIMELOCK_DURATION);
    s.client.execute_policy_update(&s.admin);

    let updated = s.client.get_loan_policy();
    assert_eq!(updated.min_membership_duration, 72 * 60 * 60);
}
