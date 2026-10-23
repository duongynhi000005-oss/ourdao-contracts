use soroban_sdk::{symbol_short, Address, Env, String, Vec};

use crate::error::Error;
use crate::storage;
use crate::types::{
    Loan, LoanProposal, LoanStatus, LoanTerms, LoanTermsEdited, MemberStatus, ProposalPhase,
    ProposalStatus, BASIS_POINTS,
};
use crate::util;

/// Quote the terms a loan of `amount` would carry right now. Interest scales
/// linearly with the loan's size relative to the treasury, clamped to policy.
pub fn calculate_loan_terms(env: &Env, amount: i128) -> LoanTerms {
    quote_terms(env, amount, util::treasury_balance(env))
}

/// [`calculate_loan_terms`] against an explicit treasury balance, so loans in
/// a non-base asset are priced against the treasury held in that asset.
fn quote_terms(env: &Env, amount: i128, treasury: i128) -> LoanTerms {
    let policy = storage::get_policy(env);

    let loan_ratio = if treasury > 0 {
        amount
            .checked_mul(BASIS_POINTS)
            .map(|v| util::ceil_div_nonnegative(v, treasury))
            .unwrap_or(BASIS_POINTS)
            .min(BASIS_POINTS)
    } else {
        BASIS_POINTS
    };
    let spread = (policy.max_interest_rate - policy.min_interest_rate) as i128;
    let added_rate = loan_ratio
        .checked_mul(spread)
        .map(|v| util::ceil_div_nonnegative(v, BASIS_POINTS))
        .unwrap_or(spread);
    let rate = (policy.min_interest_rate as i128)
        .checked_add(added_rate)
        .unwrap_or(policy.max_interest_rate as i128)
        .min(policy.max_interest_rate as i128);

    // Member debt: ceil-division rounds up to favour the treasury.
    let interest = if amount > 0 && rate > 0 {
        amount
            .checked_mul(rate)
            .and_then(|v| v.checked_add(BASIS_POINTS - 1))
            .and_then(|v| v.checked_div(BASIS_POINTS))
            .unwrap_or(i128::MAX - amount)
    } else {
        0
    };
    let total_repayment = amount.checked_add(interest).unwrap_or(i128::MAX);

    LoanTerms {
        interest_rate: rate as u32,
        total_repayment,
        duration: policy.max_loan_duration,
    }
}

pub fn is_eligible_for_loan(env: &Env, member: &Address) -> Result<(), Error> {
    let record = match storage::get_member(env, member) {
        Some(m) if m.status == MemberStatus::ActiveMember => m,
        _ => return Err(Error::MemberNotActive),
    };
    if record.has_active_loan {
        return Err(Error::HasActiveLoan);
    }
    let policy = storage::get_policy(env);
    let now = env.ledger().timestamp();
    if now.saturating_sub(record.join_time) < policy.min_membership_duration {
        return Err(Error::NotEligibleForLoan);
    }
    if record.last_loan_time != 0
        && now.saturating_sub(record.last_loan_time) < policy.cooldown_period
    {
        return Err(Error::CooldownActive);
    }
    Ok(())
}

// `env.events().publish` is deprecated in soroban-sdk in favour of
// `#[contractevent]`, but migration is a coordinated, breaking wire-format
// change (#85).  Suppress per-function so unrelated deprecations still surface.
pub fn request_loan(
    env: &Env,
    borrower: Address,
    amount: i128,
    metadata_cid: Option<String>,
) -> Result<u32, Error> {
    util::require_initialized(env)?;
    let base = storage::get_token(env);
    request_loan_in_asset(env, borrower, amount, base, metadata_cid)
}

/// Requests a loan of `amount` denominated in `asset`, which must be
/// whitelisted. Size limits and pricing use the treasury held in that asset.
#[allow(deprecated)]
pub fn request_loan_in_asset(
    env: &Env,
    borrower: Address,
    amount: i128,
    asset: Address,
    metadata_cid: Option<String>,
) -> Result<u32, Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    util::require_active_member(env, &borrower)?;
    if !storage::is_token_whitelisted(env, &asset) {
        return Err(Error::TokenNotWhitelisted);
    }

    if amount <= 0 {
        return Err(Error::InvalidAmount);
    }
    if let Some(ref cid) = metadata_cid {
        if cid.is_empty() {
            return Err(Error::InvalidMetadataCid);
        }
        if cid.len() > 64 {
            return Err(Error::DocumentTooLarge);
        }
    }
    is_eligible_for_loan(env, &borrower)?;
    if storage::get_open_proposals(env, &borrower) >= storage::MAX_OPEN_PROPOSALS_PER_MEMBER {
        return Err(Error::TooManyOpenProposals);
    }

    let policy = storage::get_policy(env);
    let treasury = util::treasury_balance_in_asset(env, &asset);
    // Treasury exposure limit: floor-division rounds down to favour the treasury.
    let max_loan = treasury
        .checked_mul(policy.max_loan_to_treasury_ratio as i128)
        .and_then(|v| v.checked_div(BASIS_POINTS))
        .unwrap_or(0);
    if amount > max_loan {
        return Err(Error::ExceedsTreasuryRatio);
    }

    let terms = quote_terms(env, amount, treasury);
    let now = env.ledger().timestamp();
    let id = storage::next_id(env, storage::DataKey::NextProposalId);
    let proposal = LoanProposal {
        id,
        borrower: borrower.clone(),
        amount,
        interest_rate: terms.interest_rate,
        duration: terms.duration,
        total_repayment: terms.total_repayment,
        created_at: now,
        editing_period_end: now + policy.editing_period,
        phase: ProposalPhase::Editing,
        status: ProposalStatus::Pending,
        for_votes: 0,
        against_votes: 0,
        votes_cast: 0,
        voting_period: policy.voting_period,
        metadata_cid,
        last_edited_at: None,
    };
    storage::set_loan_proposal(env, &proposal);
    // Only non-base assets are recorded, so base-token loans keep their
    // existing storage footprint and events.
    if asset != storage::get_token(env) {
        storage::set_loan_asset(env, id, &asset);
        env.events()
            .publish((symbol_short!("loan_asst"),), (id, asset.clone()));
    }
    storage::extend_instance(env);

    env.events().publish(
        (symbol_short!("loan_req"),),
        (id, borrower, amount, terms.total_repayment, asset),
    );
    Ok(id)
}

#[allow(deprecated)]
pub fn edit_loan_proposal(
    env: &Env,
    borrower: Address,
    proposal_id: u32,
    new_amount: i128,
) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    util::require_active_member(env, &borrower)?;
    let mut proposal =
        storage::get_loan_proposal(env, proposal_id).ok_or(Error::ProposalNotFound)?;
    if proposal.borrower != borrower {
        return Err(Error::NotBorrower);
    }
    let now = env.ledger().timestamp();
    if proposal.phase != ProposalPhase::Editing || now >= proposal.editing_period_end {
        return Err(Error::NotInEditingPhase);
    }
    if new_amount <= 0 {
        return Err(Error::InvalidAmount);
    }

    let prev_amount = proposal.amount;
    let prev_total_repayment = proposal.total_repayment;
    let asset = storage::get_loan_asset(env, proposal_id);
    let terms = quote_terms(
        env,
        new_amount,
        util::treasury_balance_in_asset(env, &asset),
    );
    proposal.amount = new_amount;
    proposal.interest_rate = terms.interest_rate;
    proposal.duration = terms.duration;
    proposal.total_repayment = terms.total_repayment;
    proposal.last_edited_at = Some(now);
    storage::set_loan_proposal(env, &proposal);

    let _structured = LoanTermsEdited {
        proposal_id,
        borrower: borrower.clone(),
        prev_amount,
        prev_total_repayment,
        new_amount,
        total_repayment: terms.total_repayment,
        edited_at: now,
    };
    env.events().publish(
        (symbol_short!("loan_edit"),),
        (proposal_id, borrower, new_amount, terms.total_repayment),
    );
    Ok(())
}

/// Cancel a loan proposal during its editing period. Only the original
/// borrower may cancel, on,y while the proposal is still in the editing
/// phase and pending. Emits a dedicated `ProposalCancelled` event so
/// off-chain indexers can track cancellations cleanly.
#[allow(deprecated)]
pub fn cancel_loan_proposal(env: &Env, borrower: Address, proposal_id: u32) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    util::require_active_member(env, &borrower)?;
    let mut proposal =
        storage::get_loan_proposal(env, proposal_id).ok_or(Error::ProposalNotFound)?;
    if proposal.borrower != borrower {
        return Err(Error::NotBorrower);
    }
    let now = env.ledger().timestamp();
    if proposal.phase != ProposalPhase::Editing || now >= proposal.editing_period_end {
        return Err(Error::NotInEditingPhase);
    }
    if proposal.status != ProposalStatus::Pending {
        return Err(Error::NotInEditingPhase);
    }

    proposal.status = ProposalStatus::Cancelled;
    proposal.phase = ProposalPhase::Expired;
    storage::set_loan_proposal(env, &proposal);

    env.events()
        .publish((symbol_short!("prop_canc"),), (proposal_id, now));
    Ok(())
}

/// Advances a proposal's phase based on the clock. Returns the (possibly
/// mutated) proposal; the caller is responsible for persisting it.
pub fn refresh_phase(env: &Env, mut proposal: LoanProposal) -> LoanProposal {
    let now = env.ledger().timestamp();
    if proposal.phase == ProposalPhase::Editing && now >= proposal.editing_period_end {
        proposal.phase = ProposalPhase::Voting;
    }
    if proposal.phase == ProposalPhase::Voting
        && now > proposal.editing_period_end + proposal.voting_period
        && proposal.status == ProposalStatus::Pending
    {
        // Timed out without quorum: `Expired` status, distinct from the
        // `Rejected` status written by the early-rejection path.
        proposal.phase = ProposalPhase::Expired;
        proposal.status = ProposalStatus::Expired;
    }
    proposal
}

#[allow(deprecated)]
pub fn vote_on_loan_proposal(
    env: &Env,
    voter: Address,
    proposal_id: u32,
    support: bool,
) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    util::require_active_member(env, &voter)?;
    vote_on_loan_proposal_internal(env, &voter, proposal_id, support)
}

pub fn vote_batch(
    env: &Env,
    voter: Address,
    votes: Vec<crate::types::ProposalVote>,
) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    util::require_active_member(env, &voter)?;

    for v in votes.iter() {
        vote_on_loan_proposal_internal(env, &voter, v.proposal_id, v.vote)?;
    }
    Ok(())
}

#[allow(deprecated)]
fn vote_on_loan_proposal_internal(
    env: &Env,
    voter: &Address,
    proposal_id: u32,
    support: bool,
) -> Result<(), Error> {
    let mut proposal = storage::get_loan_proposal(env, proposal_id)
        .ok_or(Error::ProposalNotFound)
        .map(|p| refresh_phase(env, p))?;

    if proposal.phase == ProposalPhase::Editing {
        return Err(Error::NotInVotingPhase);
    }
    if proposal.phase != ProposalPhase::Voting {
        return Err(Error::VotingEnded);
    }
    let now = env.ledger().timestamp();
    // Issue #173: use >= so voting is closed at exactly the deadline second,
    // making voting and post-voting execution strictly mutually exclusive.
    if now >= proposal.editing_period_end + proposal.voting_period {
        return Err(Error::VotingEnded);
    }
    if *voter == proposal.borrower {
        return Err(Error::BorrowerCannotVote);
    }
    if storage::has_loan_voted(env, proposal_id, voter) {
        return Err(Error::AlreadyVoted);
    }

    let weight = util::voting_weight(env, voter);
    if support {
        proposal.for_votes += weight;
    } else {
        proposal.against_votes += weight;
    }
    proposal.votes_cast += 1;
    storage::set_loan_voted(env, proposal_id, voter);
    env.events().publish(
        (symbol_short!("loan_vote"),),
        (proposal_id, voter.clone(), support),
    );

    // Read the policy and the active-member count once per call (#355).
    let policy = storage::get_policy(env);
    let threshold = if policy.quorum_bps > 0 {
        policy.quorum_bps
    } else {
        storage::get_threshold(env)
    };
    let active_members = storage::get_active_members(env);
    let required = util::required_votes(active_members, threshold);
    if proposal.for_votes >= required && proposal.status == ProposalStatus::Pending {
        proposal.status = ProposalStatus::ApprovedPendingDisbursement;
        proposal.phase = ProposalPhase::Executed;
        storage::set_loan_proposal(env, &proposal);
        if approve_and_disburse(env, &proposal).is_ok() {
            proposal.status = ProposalStatus::Approved;
        } else {
            env.events().publish(
                (symbol_short!("loan_wait"),),
                (proposal.id, proposal.amount),
            );
        }
    } else if proposal.status == ProposalStatus::Pending {
        let remaining = active_members.saturating_sub(proposal.votes_cast);
        let max_remaining = remaining as i128 * (1 + util::MAX_STAKE_BONUS);
        if proposal.for_votes + max_remaining < required {
            proposal.status = ProposalStatus::Rejected;
            proposal.phase = ProposalPhase::Expired;
            env.events().publish(
                (symbol_short!("loan_rej"),),
                (proposal.id, proposal.for_votes, proposal.against_votes),
            );
        }
    }
    storage::set_loan_proposal(env, &proposal);
    Ok(())
}

pub fn disburse_approved_loan(env: &Env, proposal_id: u32) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    let proposal = storage::get_loan_proposal(env, proposal_id).ok_or(Error::ProposalNotFound)?;
    if proposal.status != ProposalStatus::ApprovedPendingDisbursement {
        return Err(Error::NotInVotingPhase);
    }
    approve_and_disburse(env, &proposal)?;
    let mut proposal = proposal;
    proposal.status = ProposalStatus::Approved;
    storage::set_loan_proposal(env, &proposal);
    Ok(())
}

#[allow(deprecated)]
fn approve_and_disburse(env: &Env, proposal: &LoanProposal) -> Result<(), Error> {
    let asset = storage::get_loan_asset(env, proposal.id);
    let treasury = util::treasury_balance_in_asset(env, &asset);
    if treasury < proposal.amount {
        return Err(Error::InsufficientTreasury);
    }

    // Issue 61: re-quote at disbursement so rate reflects current treasury balance
    let terms = quote_terms(env, proposal.amount, treasury);

    let now = env.ledger().timestamp();
    // Reuse the proposal's own id rather than a separate counter: a proposal
    // produces at most one loan, so this keeps loan_id == proposal_id as an
    // invariant instead of two sequences that silently diverge the moment
    // any proposal is rejected or expires without disbursing (the off-chain
    // indexer has no other way to correlate a loan back to its proposal).
    let id = proposal.id;
    let loan = Loan {
        id,
        borrower: proposal.borrower.clone(),
        principal: proposal.amount,
        interest_rate: terms.interest_rate,
        total_repayment: terms.total_repayment,
        start_time: now,
        due_time: now + terms.duration,
        status: LoanStatus::Active,
        amount_repaid: 0,
    };
    storage::set_loan(env, &loan);

    let mut borrower = storage::get_member(env, &proposal.borrower).ok_or(Error::NotMember)?;
    borrower.has_active_loan = true;
    borrower.last_loan_time = now;
    borrower.total_loans += 1;
    borrower.active_loans += 1;
    storage::set_member(env, &borrower);
    storage::inc_active_loans_in_asset(env, &asset);

    util::token_client_for(env, &asset).transfer(
        &util::contract_address(env),
        &proposal.borrower,
        &proposal.amount,
    );

    env.events().publish(
        (symbol_short!("loan_appr"),),
        (
            id,
            proposal.borrower.clone(),
            proposal.amount,
            loan.due_time,
            asset,
        ),
    );
    Ok(())
}

/// Repays a loan's entire remaining balance in one transaction. A thin
/// wrapper over the same path [`repay_loan_partial`] uses, so existing
/// integrations built against this zero-amount-argument entrypoint keep
/// working unchanged — see [`repay_loan_partial`] for why a breaking ABI
/// change to this entrypoint was avoided instead.
pub fn repay_loan(env: &Env, borrower: Address, loan_id: u32) -> Result<(), Error> {
    repay_loan_internal(env, borrower, loan_id, None)
}

/// [`repay_loan`] with the caller naming the asset they repay in, so a client
/// can't silently pay in the wrong token: `asset` must be the loan's own asset.
pub fn repay_loan_in_asset(
    env: &Env,
    borrower: Address,
    loan_id: u32,
    asset: Address,
) -> Result<(), Error> {
    util::require_initialized(env)?;
    if storage::get_loan(env, loan_id).is_none() {
        return Err(Error::LoanNotFound);
    }
    if storage::get_loan_asset(env, loan_id) != asset {
        return Err(Error::AssetMismatch);
    }
    repay_loan_internal(env, borrower, loan_id, None)
}

/// Repays up to `amount` of a loan's outstanding balance. `amount` must be
/// positive and may not exceed what's currently outstanding.
///
/// Payments apply to accrued interest first, then principal: whatever
/// portion of `amount` falls within the loan's still-unpaid interest is
/// distributed to active members as yield immediately, exactly as a full
/// repayment always has. The remainder (principal) needs no separate
/// bookkeeping — it's already sitting in the contract's token balance the
/// moment it's transferred in, so it's counted in the treasury right away.
///
/// The loan only flips to `Repaid` (and `has_active_loan` only clears) once
/// the outstanding balance reaches exactly zero.
///
/// This is a new entrypoint rather than an added parameter on `repay_loan`:
/// the ABI isn't upgradeable once deployed, and a new entrypoint leaves
/// every existing caller of `repay_loan(borrower, loan_id)` — including
/// `ourdao-backend` and any already-deployed clients — untouched, at the
/// cost of two entrypoints sharing one code path instead of one.
pub fn repay_loan_partial(
    env: &Env,
    borrower: Address,
    loan_id: u32,
    amount: i128,
) -> Result<(), Error> {
    repay_loan_internal(env, borrower, loan_id, Some(amount))
}

// Not wired to an entrypoint; kept unchanged from the pre-merge version.
#[allow(dead_code, deprecated)]
pub fn repay_partial(env: &Env, borrower: Address, amount: i128) -> Result<(), Error> {
    let next_id = storage::next_id(env, storage::DataKey::NextProposalId);
    let mut loan_id_opt = None;
    for i in 1..next_id {
        if let Some(loan) = storage::get_loan(env, i) {
            if loan.borrower == borrower && loan.status == LoanStatus::Active {
                loan_id_opt = Some(i);
                break;
            }
        }
    }
    let loan_id = loan_id_opt.ok_or(Error::LoanNotFound)?;
    repay_loan_internal(env, borrower.clone(), loan_id, Some(amount))?;
    env.events().publish(
        (soroban_sdk::String::from_str(env, "loan_partial_repaid"),),
        (borrower, amount),
    );
    Ok(())
}

#[allow(deprecated)]
fn repay_loan_internal(
    env: &Env,
    borrower: Address,
    loan_id: u32,
    amount: Option<i128>,
) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    borrower.require_auth();

    let mut loan = storage::get_loan(env, loan_id).ok_or(Error::LoanNotFound)?;
    if loan.borrower != borrower {
        return Err(Error::NotBorrower);
    }
    if loan.status != LoanStatus::Active {
        return Err(Error::LoanNotActive);
    }

    let mut outstanding = loan.total_repayment - loan.amount_repaid;

    let now = env.ledger().timestamp();
    if now > loan.due_time {
        let policy = storage::get_policy(env);
        // Overdue penalty (member debt): ceil-division rounds up to favour the treasury.
        let penalty = if outstanding > 0 && policy.default_penalty_bps > 0 {
            outstanding
                .checked_mul(policy.default_penalty_bps as i128)
                .and_then(|v| v.checked_add(crate::types::BASIS_POINTS - 1))
                .and_then(|v| v.checked_div(crate::types::BASIS_POINTS))
                .unwrap_or(0)
        } else {
            0
        };
        outstanding += penalty;
        loan.total_repayment += penalty;
        loan.principal += penalty; // Keep penalty in treasury, don't distribute as interest
    }

    let amount = amount.unwrap_or(outstanding);
    if amount <= 0 || amount > outstanding {
        return Err(Error::InvalidAmount);
    }

    let asset = storage::get_loan_asset(env, loan_id);

    // Interest-first split: how much of the loan's total interest was
    // already covered before this payment vs. after it. The difference is
    // the interest component of *this* payment; everything else is principal.
    let interest_total = loan.total_repayment - loan.principal;
    let interest_before = loan.amount_repaid.min(interest_total);
    loan.amount_repaid += amount;
    let interest_after = loan.amount_repaid.min(interest_total);
    let interest_component = interest_after - interest_before;

    let remaining = loan.total_repayment - loan.amount_repaid;
    if remaining == 0 {
        loan.status = LoanStatus::Repaid;
        storage::dec_active_loans_in_asset(env, &asset);
        if let Some(mut member) = storage::get_member(env, &borrower) {
            member.has_active_loan = false;
            member.repaid_loans += 1;
            member.active_loans = member.active_loans.saturating_sub(1);
            storage::set_member(env, &member);
        }
    }
    storage::set_loan(env, &loan);

    // Yield is accounted and claimed in the base token, so interest paid in
    // any other asset stays in the treasury rather than being distributed.
    if asset == storage::get_token(env) {
        distribute_interest(env, interest_component, &asset);
    }
    // External token interaction last; failed transfers revert all accounting effects.
    util::token_client_for(env, &asset).transfer(&borrower, util::contract_address(env), &amount);

    env.events().publish(
        (symbol_short!("loan_rpy"),),
        (loan_id, borrower, remaining, asset),
    );
    Ok(())
}

/// Permissionless keeper call: persists the expired/rejected transition for a
/// loan proposal whose voting window has passed without reaching quorum.
/// Succeeds exactly once per proposal — subsequent calls are a no-op (no
/// double event).
#[allow(deprecated)]
pub fn expire_loan_proposal(env: &Env, proposal_id: u32) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    let mut proposal =
        storage::get_loan_proposal(env, proposal_id).ok_or(Error::ProposalNotFound)?;
    proposal = refresh_phase(env, proposal);
    if proposal.phase != ProposalPhase::Expired {
        return Err(Error::ProposalNotExpired);
    }
    // Check if already persisted (no-op for repeat calls).
    // Re-read from storage to compare: if already Expired, skip.
    if let Some(original) = storage::get_loan_proposal(env, proposal_id) {
        if original.phase == ProposalPhase::Expired {
            return Ok(());
        }
    }
    storage::set_loan_proposal(env, &proposal);
    storage::extend_instance(env);

    env.events().publish(
        (symbol_short!("loan_exp"),),
        (proposal_id, proposal.borrower),
    );
    Ok(())
}

/// Marks an overdue loan as defaulted. Permissionless and callable by anyone
/// once `due_time + policy.default_grace_period` has passed — this is an
/// objective, time-based state transition (a keeper call), not an admin
/// action, so there's nothing to authorize.
///
/// Consequence: the borrower's `contribution` (their pro-rata claim on the
/// treasury via `calculate_exit_share`) is slashed by `default_penalty_bps`,
/// and `has_active_loan` is cleared. Clearing the flag is deliberate: it lets
/// a defaulted borrower still exit the DAO with their reduced share rather
/// than being trapped (exit is blocked while `has_active_loan` is true), and
/// lets them request a new loan again after the normal cooldown. Like
/// `Repaid`, `Defaulted` is terminal — a defaulted loan can't later be repaid.
#[allow(deprecated)]
pub fn mark_loan_defaulted(env: &Env, loan_id: u32) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    let mut loan = storage::get_loan(env, loan_id).ok_or(Error::LoanNotFound)?;
    if loan.status != LoanStatus::Active {
        return Err(Error::LoanNotActive);
    }

    let policy = storage::get_policy(env);
    let now = env.ledger().timestamp();
    if now < loan.due_time + policy.default_grace_period {
        return Err(Error::LoanNotOverdue);
    }

    loan.status = LoanStatus::Defaulted;
    let asset = storage::get_loan_asset(env, loan_id);
    storage::dec_active_loans_in_asset(env, &asset);
    storage::set_loan(env, &loan);

    let mut penalty: i128 = 0;
    if let Some(mut member) = storage::get_member(env, &loan.borrower) {
        // Default slashing penalty (member debt to treasury): ceil-division rounds up to favour the treasury.
        penalty = if member.contribution > 0 && policy.default_penalty_bps > 0 {
            member
                .contribution
                .checked_mul(policy.default_penalty_bps as i128)
                .and_then(|v| v.checked_add(BASIS_POINTS - 1))
                .and_then(|v| v.checked_div(BASIS_POINTS))
                .unwrap_or(member.contribution)
                .min(member.contribution)
        } else {
            0
        };
        member.contribution -= penalty;
        member.has_active_loan = false;
        // Defaulted loans are terminal but were never repaid, so only the
        // outstanding count changes — `repaid_loans` deliberately stays put.
        member.active_loans = member.active_loans.saturating_sub(1);
        storage::set_member(env, &member);
        if penalty > 0 {
            storage::set_active_contributions(
                env,
                storage::get_active_contributions(env) - penalty,
            );
        }
        // Yield is split equally (see `distribute_interest`), so a defaulter
        // keeps earning from here on like any active member, but forfeits
        // whatever yield they had accrued and not yet claimed at the moment
        // of default. Advancing the snapshot leaves that amount in the
        // treasury, where it accrues to the remaining members' exit shares.
        let acc = storage::get_yield_accumulator(env);
        storage::set_yield_snapshot(env, &loan.borrower, acc);
    }
    storage::extend_instance(env);

    env.events().publish(
        (symbol_short!("loan_dflt"),),
        (loan_id, loan.borrower.clone(), penalty),
    );
    Ok(())
}

/// Splits repaid interest equally across active members as claimable yield.
///
/// Yield is deliberately one-member-one-share while the exit share is
/// proportional to contribution: yield rewards participation, the exit share
/// returns capital. A defaulted borrower forfeits their unclaimed yield at the
/// moment of default (see `mark_loan_defaulted`).
///
/// Any indivisible remainder is retained by the treasury.
///
/// Uses a pull-based accumulator: instead of iterating every member and
/// bumping their `PendingYield` (O(n)), we increment a global
/// `YieldAccumulator` by `interest / active_members`. Each member stores
/// a snapshot of the accumulator at their last interaction (join, claim,
/// or exit). Their pending yield is `(accumulator - snapshot) * 1`.
///
/// `pub(crate)` (rather than private) solely so the property tests in
/// `tests/properties.rs` can drive it directly with arbitrary `interest` values instead
/// of only the ones reachable through a real loan's computed interest.
///
/// `asset` is the asset the interest was paid in. It is carried on the
/// `interest` event so indexers can attribute it to the right treasury.
#[allow(deprecated)]
pub(crate) fn distribute_interest(env: &Env, interest: i128, asset: &Address) {
    let active = storage::get_active_members(env) as i128;
    if interest <= 0 || active == 0 {
        return;
    }

    // #60 — Carry the sub-divisible remainder forward instead of silently discarding
    let total_interest = interest + storage::get_yield_remainder(env);
    let per_member = total_interest / active;
    let remainder = total_interest % active;

    storage::set_yield_remainder(env, remainder);

    if per_member > 0 {
        let current = storage::get_yield_accumulator(env);
        storage::set_yield_accumulator(env, current + per_member);
    }

    // Unconditionally publish the event so the indexer sees the interest paid
    env.events().publish(
        (symbol_short!("interest"),),
        (interest, active, asset.clone()),
    );
}
