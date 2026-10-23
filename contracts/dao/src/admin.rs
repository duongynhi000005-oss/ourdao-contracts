use soroban_sdk::{symbol_short, token, Address, Env, Vec};

use crate::error::Error;
use crate::storage::{self, extend_instance};
use crate::types::{LoanPolicy, PendingPolicyUpdate, BASIS_POINTS};
use crate::util;

/// Timelock delay between proposing and executing a policy update (48 hours in seconds).
pub const TIMELOCK_DURATION: u64 = 48 * 60 * 60;

fn validate_policy(policy: &LoanPolicy) -> Result<(), Error> {
    if policy.membership_contribution <= 0
        || policy.max_loan_duration == 0
        || policy.min_interest_rate as i128 > BASIS_POINTS
        || policy.max_interest_rate as i128 > BASIS_POINTS
        || policy.min_interest_rate > policy.max_interest_rate
        || policy.max_loan_to_treasury_ratio == 0
        || policy.max_loan_to_treasury_ratio as i128 > BASIS_POINTS
        || policy.default_penalty_bps as i128 > BASIS_POINTS
        || policy.editing_period == 0
        || policy.voting_period == 0
        || policy.editing_period > 30 * 24 * 60 * 60
        || policy.voting_period > 30 * 24 * 60 * 60
        || policy.min_membership_duration > 30 * 24 * 60 * 60
        || policy.cooldown_period > 30 * 24 * 60 * 60
        || policy.default_grace_period > 30 * 24 * 60 * 60
        || policy.treasury_threshold == 0
        || policy.treasury_threshold as i128 > BASIS_POINTS
        || policy.quorum_bps as i128 > BASIS_POINTS
    {
        return Err(Error::InvalidLoanPolicy);
    }
    Ok(())
}

/// Probe `token` with read-only `decimals` and `balance` calls so a wrong address
/// (an account, a non-token contract, or a typo) is rejected up front
/// instead of bricking later token interactions (#275). `try_` calls turn a missing
/// contract or missing function into an error rather than a trap.
fn validate_token(env: &Env, token: &Address) -> Result<(), Error> {
    let client = token::Client::new(env, token);
    match (
        client.try_decimals(),
        client.try_balance(&env.current_contract_address()),
    ) {
        (Ok(Ok(_)), Ok(Ok(_))) => Ok(()),
        _ => Err(Error::InvalidToken),
    }
}

// `env.events().publish` is deprecated in soroban-sdk in favour of
// `#[contractevent]`, but migration changes the wire format and is a
// coordinated, breaking change.  Suppress only in functions that publish events
// so unrelated future deprecations still surface via `cargo clippy -D warnings`.
#[allow(deprecated)]
pub fn initialize(
    env: &Env,
    admins: Vec<Address>,
    consensus_threshold: u32,
    membership_fee: i128,
    token: Address,
    policy: LoanPolicy,
) -> Result<(), Error> {
    if storage::is_initialized(env) {
        return Err(Error::AlreadyInitialized);
    }
    if consensus_threshold == 0 || consensus_threshold as i128 > BASIS_POINTS {
        return Err(Error::InvalidThreshold);
    }
    if membership_fee <= 0 {
        return Err(Error::InvalidAmount);
    }
    if admins.is_empty() {
        return Err(Error::NotAuthorized);
    }
    // Admins must be unique: `remove_admin` relies on it, and duplicates once
    // let it leave the contract with zero admins (#42). `first_index_of` is a
    // single host call per admin, so this check stays linear.
    for (i, a) in admins.iter().enumerate() {
        if admins.first_index_of(&a) != Some(i as u32) {
            return Err(Error::AlreadyAdmin);
        }
    }
    validate_policy(&policy)?;
    validate_token(env, &token)?;

    storage::set_admins(env, &admins);
    storage::set_threshold(env, consensus_threshold);
    storage::set_membership_fee(env, membership_fee);
    storage::set_token(env, &token);
    storage::set_policy(env, &policy);
    storage::set_paused(env, false);
    storage::set_members(env, &Vec::new(env));
    storage::set_total_members(env, 0);
    storage::set_active_members(env, 0);
    extend_instance(env);

    env.events().publish(
        (symbol_short!("init"),),
        (admins, consensus_threshold, membership_fee, token),
    );
    Ok(())
}

#[allow(deprecated)]
pub fn add_admin(env: &Env, caller: Address, admin: Address) -> Result<(), Error> {
    util::require_admin(env, &caller)?;
    let mut admins = storage::get_admins(env);
    if admins.contains(&admin) {
        return Err(Error::AlreadyAdmin);
    }
    admins.push_back(admin.clone());
    storage::set_admins(env, &admins);
    extend_instance(env);
    env.events().publish((symbol_short!("admin_add"),), admin);
    Ok(())
}

#[allow(deprecated)]
pub fn remove_admin(env: &Env, caller: Address, admin: Address) -> Result<(), Error> {
    util::require_admin(env, &caller)?;
    let mut admins = storage::get_admins(env);
    if admins.len() <= 1 {
        return Err(Error::CannotRemoveLastAdmin);
    }
    // `initialize` and `add_admin` reject duplicates, so at most one entry matches.
    let i = admins.first_index_of(&admin).ok_or(Error::NotAdmin)?;
    admins.remove(i);
    storage::set_admins(env, &admins);
    extend_instance(env);
    env.events().publish((symbol_short!("admin_rem"),), admin);
    Ok(())
}

#[allow(deprecated)]
pub fn set_pauser(env: &Env, caller: Address, pauser: Address) -> Result<(), Error> {
    util::require_admin(env, &caller)?;
    if storage::get_pauser(env).as_ref() == Some(&pauser) {
        return Err(Error::AlreadyPauser);
    }
    storage::set_pauser(env, &pauser);
    extend_instance(env);
    env.events().publish((symbol_short!("pset"),), pauser);
    Ok(())
}

#[allow(deprecated)]
pub fn revoke_pauser(env: &Env, caller: Address) -> Result<(), Error> {
    util::require_admin(env, &caller)?;
    if storage::get_pauser(env).is_none() {
        return Err(Error::NotPauser);
    }
    storage::remove_pauser(env);
    extend_instance(env);
    env.events().publish((symbol_short!("prev"),), caller);
    Ok(())
}

#[allow(deprecated)]
pub fn set_consensus_threshold(env: &Env, caller: Address, threshold: u32) -> Result<(), Error> {
    util::require_admin(env, &caller)?;
    if threshold == 0 || threshold as i128 > BASIS_POINTS {
        return Err(Error::InvalidThreshold);
    }
    let old_threshold = storage::get_threshold(env);
    storage::set_threshold(env, threshold);
    extend_instance(env);
    env.events()
        .publish((symbol_short!("threshold"),), (old_threshold, threshold));
    Ok(())
}

#[allow(deprecated)]
pub fn propose_policy_update(env: &Env, caller: Address, policy: LoanPolicy) -> Result<(), Error> {
    util::require_admin(env, &caller)?;
    if policy.quorum_bps == 0 {
        return Err(Error::InvalidLoanPolicy);
    }
    validate_policy(&policy)?;
    if policy.treasury_threshold == 0 || policy.treasury_threshold as i128 > BASIS_POINTS {
        return Err(Error::InvalidLoanPolicy);
    }
    if policy.quorum_bps == 0 || policy.quorum_bps as i128 > BASIS_POINTS {
        return Err(Error::InvalidLoanPolicy);
    }
    let now = env.ledger().timestamp();
    let execution_time = now + TIMELOCK_DURATION;
    let update = PendingPolicyUpdate {
        policy,
        proposed_at: now,
        execution_time,
    };
    storage::set_pending_policy_update(env, &update);
    extend_instance(env);
    env.events()
        .publish((symbol_short!("pol_prop"),), (caller, execution_time));
    Ok(())
}

#[allow(deprecated)]
pub fn execute_policy_update(env: &Env, caller: Address) -> Result<(), Error> {
    util::require_admin(env, &caller)?;
    let update = storage::get_pending_policy_update(env).ok_or(Error::NoPendingPolicy)?;
    let now = env.ledger().timestamp();
    if now < update.execution_time {
        return Err(Error::TimelockNotExpired);
    }
    let old_policy = storage::get_policy(env);
    storage::set_policy(env, &update.policy);
    storage::remove_pending_policy_update(env);
    extend_instance(env);
    env.events()
        .publish((symbol_short!("policy"),), (old_policy, update.policy));
    Ok(())
}

#[allow(deprecated)]
pub fn cancel_policy_update(env: &Env, caller: Address) -> Result<(), Error> {
    util::require_admin(env, &caller)?;
    if storage::get_pending_policy_update(env).is_none() {
        return Err(Error::NoPendingPolicy);
    }
    storage::remove_pending_policy_update(env);
    extend_instance(env);
    env.events().publish((symbol_short!("pol_canc"),), caller);
    Ok(())
}

pub fn get_pending_policy_update(env: &Env) -> Option<PendingPolicyUpdate> {
    storage::get_pending_policy_update(env)
}

pub fn set_policy(env: &Env, caller: Address, policy: LoanPolicy) -> Result<(), Error> {
    propose_policy_update(env, caller, policy)
}

#[allow(deprecated)]
pub fn pause(env: &Env, caller: Address) -> Result<(), Error> {
    caller.require_auth();
    if !util::is_admin(env, &caller) && !util::is_pauser(env, &caller) {
        return Err(Error::NotAdmin);
    }
    if storage::is_paused(env) {
        return Err(Error::Paused);
    }
    storage::set_paused(env, true);
    extend_instance(env);
    env.events().publish((symbol_short!("paused"),), caller);
    Ok(())
}

/// Resumes normal operations from an emergency stop.
///
/// Only admins can unpause. An assigned pauser explicitly CANNOT unpause
/// unless they are also an admin (#319).
#[allow(deprecated)]
/// Only an administrator may resume the DAO. The emergency pauser is pause-only.
/// Keep this check before any state mutation so rejected calls cannot clear a pause.
pub fn unpause(env: &Env, caller: Address) -> Result<(), Error> {
    caller.require_auth();
    // Pausers can trigger pause in an emergency, but only admins can unpause (#319).
    if !util::is_admin(env, &caller) {
        return Err(Error::NotAdmin);
    }
    if !storage::is_paused(env) {
        return Err(Error::NotPaused);
    }
    storage::set_paused(env, false);
    extend_instance(env);
    env.events().publish((symbol_short!("unpaused"),), caller);
    Ok(())
}

// ---- token whitelist (#236) ----

/// Proposes adding `token` to the treasury's supported-asset whitelist. Like a
/// policy update it only takes effect after `TIMELOCK_DURATION`, giving the
/// other admins and members time to react before loans and withdrawals can be
/// denominated in it. The token is probed up front so a typo can't be queued.
#[allow(deprecated)]
pub fn propose_token_addition(env: &Env, caller: Address, token: Address) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_admin(env, &caller)?;
    if storage::is_token_whitelisted(env, &token) {
        return Err(Error::TokenAlreadyWhitelisted);
    }
    validate_token(env, &token)?;
    let execution_time = env.ledger().timestamp() + TIMELOCK_DURATION;
    storage::set_pending_token_addition(env, &token, execution_time);
    extend_instance(env);
    env.events().publish(
        (symbol_short!("tok_prop"),),
        (caller, token, execution_time),
    );
    Ok(())
}

/// Whitelists a previously proposed `token` once its timelock has expired.
#[allow(deprecated)]
pub fn execute_token_addition(env: &Env, caller: Address, token: Address) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_admin(env, &caller)?;
    let execution_time =
        storage::get_pending_token_addition(env, &token).ok_or(Error::NoPendingTokenAddition)?;
    if env.ledger().timestamp() < execution_time {
        return Err(Error::TimelockNotExpired);
    }
    storage::remove_pending_token_addition(env, &token);
    storage::set_token_whitelisted(env, &token, true);
    extend_instance(env);
    env.events()
        .publish((symbol_short!("tok_add"),), (caller, token));
    Ok(())
}

/// Removes `token` from the whitelist, blocking new loans and withdrawals in
/// it. Loans already issued in `token` can still be repaid. The base token is
/// permanent.
#[allow(deprecated)]
pub fn remove_token(env: &Env, caller: Address, token: Address) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_admin(env, &caller)?;
    if token == storage::get_token(env) {
        return Err(Error::CannotRemoveBaseToken);
    }
    if !storage::is_token_whitelisted(env, &token) {
        return Err(Error::TokenNotWhitelisted);
    }
    storage::set_token_whitelisted(env, &token, false);
    extend_instance(env);
    env.events()
        .publish((symbol_short!("tok_rem"),), (caller, token));
    Ok(())
}
