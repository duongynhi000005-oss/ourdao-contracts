use soroban_sdk::{symbol_short, Address, Env};

use crate::error::Error;
use crate::storage::{self, extend_instance};
use crate::types::{Member, MemberStatus, StakingRewardClaimed};
use crate::util;

// `env.events().publish` is deprecated in soroban-sdk in favour of
// `#[contractevent]`, but migration is a coordinated, breaking wire-format
// change (#85).  Suppress per-function so unrelated deprecations still surface.
#[allow(deprecated)]
pub fn register_member(env: &Env, member: Address) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    member.require_auth();

    // Reject only genuinely active members; a previously-exited member may rejoin.
    let existing = storage::get_member(env, &member);
    if let Some(ref record) = existing {
        if record.status == MemberStatus::ActiveMember {
            return Err(Error::AlreadyMember);
        }
    }

    let fee = storage::get_membership_fee(env);

    let is_returning = existing.is_some();
    // Preserve the member's lifetime loan counters across an exit/rejoin so the
    // on-chain credit track record isn't reset when they come back.
    let (total_loans, repaid_loans, active_loans) = match &existing {
        Some(record) => (record.total_loans, record.repaid_loans, record.active_loans),
        None => (0, 0, 0),
    };
    let record = Member {
        address: member.clone(),
        status: MemberStatus::ActiveMember,
        join_time: env.ledger().timestamp(),
        contribution: fee,
        share_balance: fee,
        has_active_loan: false,
        last_loan_time: 0,
        total_loans,
        repaid_loans,
        active_loans,
    };
    storage::set_member(env, &record);

    // Snapshot the accumulator so the new member earns nothing from interest
    // repaid before they joined.
    let acc = storage::get_yield_accumulator(env);
    storage::set_yield_snapshot(env, &member, acc);

    if !is_returning {
        let mut members = storage::get_members(env);
        members.push_back(member.clone());
        storage::set_members(env, &members);
        storage::set_total_members(env, storage::get_total_members(env) + 1);
    }
    storage::set_active_members(env, storage::get_active_members(env) + 1);
    storage::set_active_contributions(env, storage::get_active_contributions(env) + fee);
    extend_instance(env);

    // Interaction last (checks-effects-interactions): the fee transfer only
    // happens once every state transition above has completed.
    util::token_client(env).transfer(&member, util::contract_address(env), &fee);

    env.events()
        .publish((symbol_short!("joined"),), (member, fee));
    Ok(())
}

#[allow(deprecated)]
pub fn exit_dao(env: &Env, member: Address) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    let mut record = util::auth_member(env, &member)?;
    if record.has_active_loan {
        return Err(Error::HasActiveLoan);
    }

    let share = calculate_exit_share(env, &member);
    let stake = storage::get_stake(env, &member);
    let pending = compute_pending_yield(env, &member);
    let payout = share + stake + pending;

    // Issue #176: check the contract's actual token balance before attempting
    // the transfer so a temporarily depleted treasury returns a typed error
    // instead of trapping with an uninformative host panic.
    if payout > 0 {
        let contract_balance = util::token_client(env).balance(&util::contract_address(env));
        if contract_balance < payout {
            return Err(Error::InsufficientTreasury);
        }
        util::token_client(env).transfer(&util::contract_address(env), &member, &payout);
    }
    if stake > 0 {
        storage::set_stake(env, &member, 0);
        storage::set_total_staked(env, storage::get_total_staked(env) - stake);
    }
    // Settle yield: snapshot to current accumulator so rejoining starts clean.
    let acc = storage::get_yield_accumulator(env);
    storage::set_yield_snapshot(env, &member, acc);

    record.status = MemberStatus::Inactive;
    record.share_balance = 0;
    // `share` was computed above against the total that still included this
    // member; only now is it safe to remove their contribution (#353).
    let exiting_contribution = record.contribution;
    storage::set_member(env, &record);
    storage::set_active_members(env, storage::get_active_members(env) - 1);
    storage::set_active_contributions(
        env,
        storage::get_active_contributions(env) - exiting_contribution,
    );
    extend_instance(env);

    env.events()
        .publish((symbol_short!("exited"),), (member, share, pending));
    Ok(())
}

#[allow(deprecated)]
pub fn claim_rewards(env: &Env, member: Address) -> Result<i128, Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    util::auth_member(env, &member)?;
    let pending = compute_pending_yield(env, &member);
    if pending <= 0 {
        return Err(Error::NothingToClaim);
    }
    // Settle: advance snapshot to current accumulator.
    let acc = storage::get_yield_accumulator(env);
    storage::set_yield_snapshot(env, &member, acc);
    util::token_client(env).transfer(&util::contract_address(env), &member, &pending);
    let now = env.ledger().timestamp();
    env.events().publish(
        (symbol_short!("claimed"), member.clone(), pending),
        StakingRewardClaimed {
            member,
            amount: pending,
            timestamp: now,
        },
    );
    Ok(pending)
}

fn share_from_contributions(treasury: i128, contribution: i128, total_contributions: i128) -> i128 {
    if total_contributions == 0 || treasury <= 0 {
        return 0;
    }
    treasury * contribution / total_contributions
}

pub fn calculate_exit_share(env: &Env, member: &Address) -> i128 {
    let record = match storage::get_member(env, member) {
        Some(m) if m.status == MemberStatus::ActiveMember => m,
        _ => return 0,
    };
    let total_contributions = storage::get_active_contributions(env);
    let treasury = util::treasury_balance(env);
    share_from_contributions(treasury, record.contribution, total_contributions)
}

fn compute_pending_yield(env: &Env, addr: &Address) -> i128 {
    let acc = storage::get_yield_accumulator(env);
    let snap = storage::get_yield_snapshot(env, addr);
    (acc - snap).max(0)
}

#[allow(deprecated)]
pub fn delegate_vote(env: &Env, delegator: Address, delegatee: Address) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    delegator.require_auth();

    util::auth_member(env, &delegator)?;
    util::require_active_member(env, &delegatee)?;

    if delegator == delegatee {
        return Err(Error::InvalidDelegation);
    }

    storage::set_delegation(env, &delegator, &delegatee);
    env.events()
        .publish((symbol_short!("delegate"),), (delegator, delegatee));
    Ok(())
}
