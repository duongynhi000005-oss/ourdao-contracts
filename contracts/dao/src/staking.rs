//! Staking module. Members lock tokens to signal commitment and gain a
//! capped boost to their voting weight (see `util::voting_weight`). Staked
//! funds are tracked separately from the treasury and are never lent out or
//! distributed as yield.

use soroban_sdk::{symbol_short, Address, Env};

use crate::error::Error;
use crate::storage;
use crate::types::{ProposalPhase, ProposalStatus};
use crate::util;

// `env.events().publish` is deprecated in soroban-sdk in favour of
// `#[contractevent]`, but migration is a coordinated, breaking wire-format
// change (#85).  Suppress per-function so unrelated deprecations still surface.
#[allow(deprecated)]
pub fn stake(env: &Env, member: Address, amount: i128) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    util::require_active_member(env, &member)?;
    if amount <= 0 {
        return Err(Error::InvalidAmount);
    }

    // Commit stake accounting first (CEI); a failed transfer reverts these writes.
    let new_stake = storage::get_stake(env, &member) + amount;
    storage::set_stake(env, &member, new_stake);
    storage::set_total_staked(env, storage::get_total_staked(env) + amount);
    env.storage().persistent().set(
        &crate::storage::DataKey::StakeTime(member.clone()),
        &env.ledger().timestamp(),
    );
    storage::extend_instance(env);

    let voting_weight = util::voting_weight(env, &member);
    util::token_client(env).transfer(&member, util::contract_address(env), &amount);
    env.events().publish(
        (symbol_short!("staked"),),
        (member, amount, new_stake, voting_weight),
    );
    Ok(())
}

#[allow(deprecated)]
pub fn unstake(env: &Env, member: Address, amount: i128) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    member.require_auth();
    if amount <= 0 {
        return Err(Error::InvalidAmount);
    }

    let current = storage::get_stake(env, &member);
    if current == 0 {
        return Err(Error::NoStake);
    }
    if amount > current {
        return Err(Error::InsufficientStake);
    }

    let last_stake: u64 = env
        .storage()
        .persistent()
        .get(&crate::storage::DataKey::StakeTime(member.clone()))
        .unwrap_or(0);
    let policy = storage::get_policy(env);
    if env.ledger().timestamp() < last_stake + policy.cooldown_period {
        return Err(Error::CooldownActive);
    }

    // Issue #175: disallow unstaking while the member has an active pending
    // loan proposal still in the voting phase (collateral must remain locked
    // until the proposal reaches a terminal state).
    let proposal_count = storage::get_proposal_count(env, storage::DataKey::NextProposalId);
    for id in 0..proposal_count {
        if let Some(p) = storage::get_loan_proposal(env, id) {
            if p.borrower == member
                && (p.phase == ProposalPhase::Editing || p.phase == ProposalPhase::Voting)
                && p.status == ProposalStatus::Pending
                && env.ledger().timestamp() <= p.editing_period_end.saturating_add(p.voting_period)
            {
                return Err(Error::HasActiveLoan);
            }
        }
    }

    // A vote cast with stake-derived weight must remain backed until that
    // proposal closes. This includes loan ballots, treasury ballots, and
    // unrevealed private commitments, not just the borrower's own request.
    let treasury_count = storage::get_proposal_count(env, storage::DataKey::NextTreasuryId);
    for id in 0..treasury_count {
        if let Some(p) = storage::get_treasury_proposal(env, id) {
            let voting_end =
                p.created_at
                    .saturating_add(
                        p.voting_period
                            .saturating_mul(if p.private { 2 } else { 1 }),
                    );
            if p.status == ProposalStatus::Pending
                && env.ledger().timestamp() <= voting_end
                && (storage::has_treasury_voted(env, id, &member)
                    || storage::get_commit(env, id, &member).is_some())
            {
                return Err(Error::HasActiveLoan);
            }
        }
    }
    for id in 0..proposal_count {
        if let Some(p) = storage::get_loan_proposal(env, id) {
            if p.status == ProposalStatus::Pending
                && env.ledger().timestamp() <= p.editing_period_end.saturating_add(p.voting_period)
                && storage::has_loan_voted(env, id, &member)
            {
                return Err(Error::HasActiveLoan);
            }
        }
    }

    let new_stake = current - amount;
    storage::set_stake(env, &member, new_stake);
    storage::set_total_staked(env, storage::get_total_staked(env) - amount);
    storage::extend_instance(env);

    let voting_weight = util::voting_weight(env, &member);
    util::token_client(env).transfer(&util::contract_address(env), &member, &amount);
    env.events().publish(
        (symbol_short!("unstaked"),),
        (member, amount, new_stake, voting_weight),
    );
    Ok(())
}
