use soroban_sdk::{symbol_short, Address, Env};

use crate::error::Error;
use crate::storage::{self, extend_instance};
use crate::types::{Member, MemberLoanStats, MemberOverview, MemberStatus, StakingRewardClaimed};
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
    let mut record = util::require_active_member(env, &member)?;
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
    }
    if stake > 0 {
        storage::set_stake(env, &member, 0);
        storage::set_total_staked(env, storage::get_total_staked(env) - stake);
    }
    // Settle yield: snapshot to current accumulator so rejoining starts clean.
    let acc = storage::get_yield_accumulator(env);
    storage::set_yield_snapshot(env, &member, acc);

    // Release the member's name so they can re-register after rejoining.
    if let Some(name) = storage::get_name_of(env, &member) {
        env.storage()
            .persistent()
            .remove(&storage::DataKey::Name(name.clone()));
        env.storage()
            .persistent()
            .remove(&storage::DataKey::NameOf(member.clone()));
        env.events()
            .publish((symbol_short!("name_rel"),), (name, member.clone()));
    }

    // Clear delegation on exit so the member no longer delegates their vote (#409).
    storage::remove_delegation(env, &member);

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

    // Interact only after all membership, stake and yield effects are written.
    // Soroban rolls back these writes if the token transfer fails.
    if payout > 0 {
        util::token_client(env).transfer(&util::contract_address(env), &member, &payout);
    }

    env.events()
        .publish((symbol_short!("exited"),), (member, share, pending));
    Ok(())
}

#[allow(deprecated)]
pub fn claim_rewards(env: &Env, member: Address) -> Result<i128, Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    util::require_active_member(env, &member)?;
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

/// Read-only aggregate of a member's record, stake, unclaimed yield, loan
/// track record, registered name and delegate. Returns `NotMember` only if
/// the address has never registered; an exited member keeps their record and
/// is reported with its `Inactive` status.
pub fn member_overview(env: &Env, member: &Address) -> Result<MemberOverview, Error> {
    let record = storage::get_member(env, member).ok_or(Error::NotMember)?;
    Ok(MemberOverview {
        loan_stats: MemberLoanStats {
            total_loans: record.total_loans,
            repaid_loans: record.repaid_loans,
            active_loans: record.active_loans,
        },
        stake: storage::get_stake(env, member),
        pending_yield: compute_pending_yield(env, member),
        name: storage::get_name_of(env, member),
        delegate: storage::get_delegation(env, member),
        member: record,
    })
}

#[allow(deprecated)]
pub fn delegate_vote(env: &Env, delegator: Address, delegatee: Address) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;

    // Single require_auth via require_active_member to avoid Error(Auth, ExistingValue) (#409)
    util::require_active_member(env, &delegator)?;
    // Delegatee must be an active member, but unilateral delegation does not require delegatee auth (#409)
    util::check_active_member(env, &delegatee)?;

    if delegator == delegatee {
        return Err(Error::InvalidDelegation);
    }

    // Detect circular delegation: traverse delegatee's delegation chain.
    // If it reaches delegator, reject with InvalidDelegation (#409).
    let mut current = delegatee.clone();
    let max_depth = storage::get_active_members(env);
    let mut steps = 0;
    while let Some(next) = storage::get_delegation(env, &current) {
        if next == delegator {
            return Err(Error::InvalidDelegation);
        }
        current = next;
        steps += 1;
        if steps > max_depth {
            return Err(Error::InvalidDelegation);
        }
    }

    storage::set_delegation(env, &delegator, &delegatee);
    env.events()
        .publish((symbol_short!("delegate"),), (delegator, delegatee));
    Ok(())
}
