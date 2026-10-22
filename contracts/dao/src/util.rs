use soroban_sdk::{token, Address, Env};

use crate::error::Error;
use crate::storage;
use crate::types::{Member, MemberStatus, ProposalStatus};

/// Staked tokens are turned into bonus voting weight on a *quadratic* (square
/// root) curve, and this is the number of staked tokens whose square root is
/// one unit of that weight: `k` bonus votes cost `STAKE_WEIGHT_UNIT * k * k`
/// staked tokens (100, 400, 900, 1600, 2500 for 1..=5 votes).
pub const STAKE_WEIGHT_UNIT: i128 = 100;
/// ...capped here, so a whale can never fully dominate member consensus even
/// after staking far more than every other member combined.
pub const MAX_STAKE_BONUS: i128 = 5;
/// The unconditional vote every active member gets, before any stake boost.
pub const BASE_VOTE_WEIGHT: i128 = 1;

pub fn token_client(env: &Env) -> token::Client<'_> {
    token::Client::new(env, &storage::get_token(env))
}

/// Token client for any asset, e.g. a whitelisted non-base asset (#236).
pub fn token_client_for<'a>(env: &'a Env, asset: &Address) -> token::Client<'a> {
    token::Client::new(env, asset)
}

pub fn contract_address(env: &Env) -> Address {
    env.current_contract_address()
}

/// Treasury == the DAO's own token balance minus funds earmarked as stake,
/// so staked principal is never lent out or counted as distributable equity.
pub fn treasury_balance(env: &Env) -> i128 {
    treasury_balance_in_asset(env, &storage::get_token(env))
}

/// The DAO's raw holding of `asset`, staked funds included.
pub fn treasury_balance_for(env: &Env, asset: &Address) -> i128 {
    token_client_for(env, asset).balance(&contract_address(env))
}

/// Spendable treasury in `asset`. Staking only ever happens in the base token,
/// so staked funds are excluded for the base token alone.
pub fn treasury_balance_in_asset(env: &Env, asset: &Address) -> i128 {
    let bal = treasury_balance_for(env, asset);
    if *asset == storage::get_token(env) {
        bal - storage::get_total_staked(env)
    } else {
        bal
    }
}

pub fn require_initialized(env: &Env) -> Result<(), Error> {
    if storage::is_initialized(env) {
        Ok(())
    } else {
        Err(Error::NotInitialized)
    }
}

pub fn require_not_paused(env: &Env) -> Result<(), Error> {
    if storage::is_paused(env) {
        Err(Error::Paused)
    } else {
        Ok(())
    }
}

pub fn is_admin(env: &Env, who: &Address) -> bool {
    storage::get_admins(env).contains(who)
}

pub fn is_pauser(env: &Env, who: &Address) -> bool {
    storage::get_pauser(env).as_ref() == Some(who)
}

/// Authorizes `caller` and asserts admin membership.
pub fn require_admin(env: &Env, caller: &Address) -> Result<(), Error> {
    caller.require_auth();
    if is_admin(env, caller) {
        Ok(())
    } else {
        Err(Error::NotAdmin)
    }
}

/// Returns the active-member record for `who` without asserting auth.
pub fn check_active_member(env: &Env, who: &Address) -> Result<Member, Error> {
    match storage::get_member(env, who) {
        Some(m) if m.status == MemberStatus::ActiveMember => Ok(m),
        Some(_) => Err(Error::MemberNotActive),
        None => Err(Error::NotMember),
    }
}

pub fn is_active_member(env: &Env, who: &Address) -> bool {
    matches!(storage::get_member(env, who), Some(m) if m.status == MemberStatus::ActiveMember)
}

/// Authorizes `caller` and returns their active-member record, or errors.
pub fn require_active_member(env: &Env, caller: &Address) -> Result<Member, Error> {
    caller.require_auth();
    check_active_member(env, caller)
}

/// One base vote per active member, plus a quadratic (square-root) boost for
/// staked commitment, capped at `MAX_STAKE_BONUS`.
pub fn voting_weight(env: &Env, who: &Address) -> i128 {
    if storage::get_delegation(env, who).is_some() {
        return 0; // delegated their vote away
    }
    let mut total = BASE_VOTE_WEIGHT + stake_boost(storage::get_stake(env, who));
    for member in storage::get_members(env).iter() {
        // Exited/inactive members must not contribute delegated voting weight (#409)
        if !is_active_member(env, &member) {
            continue;
        }
        if let Some(delegatee) = storage::get_delegation(env, &member) {
            if delegatee == *who {
                total += BASE_VOTE_WEIGHT + stake_boost(storage::get_stake(env, &member));
            }
        }
    }
    total
}

/// Bonus votes earned by `staked` under the quadratic staking curve:
/// `isqrt(staked / STAKE_WEIGHT_UNIT)`, clamped to the `MAX_STAKE_BONUS` cap.
///
/// Squaring the boost is what makes the curve sub-linear: doubling a stake
/// buys only a ~1.41x boost and every additional vote costs 4x the stake of
/// the one before it, so pooled capital cannot buy proportional control
/// (issue #182). Weight stays in `[BASE_VOTE_WEIGHT, BASE_VOTE_WEIGHT +
/// MAX_STAKE_BONUS]` for any stake, including `i128::MAX`.
pub fn stake_boost(staked: i128) -> i128 {
    if staked <= 0 {
        return 0;
    }
    // Divide first: `isqrt(floor(x)) == floor(sqrt(x))` for `x >= 0`, so this
    // is exactly `floor(sqrt(staked / STAKE_WEIGHT_UNIT))` while keeping the
    // radicand (and every intermediate) far from `i128` overflow.
    isqrt(staked / STAKE_WEIGHT_UNIT).min(MAX_STAKE_BONUS)
}

/// Integer square root: the largest `r` with `r * r <= n`, floored. Returns 0
/// for `n <= 0` (a negative amount has no real square root, and 0 keeps the
/// caller free of special cases).
///
/// Computed digit-by-digit, two bits of the radicand at a time, so no square
/// is ever evaluated and the full `i128` range is safe.
pub fn isqrt(n: i128) -> i128 {
    if n <= 0 {
        return 0;
    }
    let mut remainder = n as u128;
    // Largest power of four <= u128::MAX, walked down to the radicand's own
    // most significant bit; each step adds at most one bit to the root.
    let mut bit: u128 = 1 << 126;
    while bit > remainder {
        bit >>= 2;
    }
    let mut root: u128 = 0;
    while bit != 0 {
        if remainder >= root + bit {
            remainder -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root as i128
}

/// Returns the current voting weight for `who`. Readable on-chain so clients
/// and frontends can display voting power without parsing source code.
pub fn get_voting_weight(env: &Env, who: &Address) -> i128 {
    voting_weight(env, who)
}

/// The amount of stake required per additional unit of voting bonus.
/// Stakes below this threshold carry no bonus; each full unit grants +1.
pub fn get_stake_weight_unit() -> i128 {
    STAKE_WEIGHT_UNIT
}

/// Maximum bonus votes a member can accumulate through staking.
/// Caps the influence of large token holders over member consensus.
pub fn get_max_stake_bonus() -> i128 {
    MAX_STAKE_BONUS
}

/// Treasury-favouring ceiling division for non-negative money or rate numerators.
/// Assumes a strictly positive denominator and non-negative numerator.
pub fn ceil_div_nonnegative(value: i128, divisor: i128) -> i128 {
    assert!(value >= 0 && divisor > 0);
    value / divisor + i128::from(value % divisor != 0)
}

/// Ceil-division consensus bar over the active-member base, in basis points:
/// `(base * threshold + BP - 1) / BP`.
pub fn required_votes(active_members: u32, threshold_bps: u32) -> i128 {
    let base = active_members as i128;
    let bp = crate::types::BASIS_POINTS;
    (base * threshold_bps as i128 + bp - 1) / bp
}

/// Returns the total amount committed to loan proposals that have passed
/// quorum but have not yet been disbursed (status == ApprovedPendingDisbursement).
/// These funds are effectively reserved and must not be double-counted as
/// available treasury for new withdrawals (#172).
/// Only proposals denominated in `asset` count (#236).
pub fn reserved_loan_commitments(env: &Env, asset: &Address) -> i128 {
    let count = storage::get_proposal_count(env, storage::DataKey::NextProposalId);
    let mut total: i128 = 0;
    for id in 0..count {
        if let Some(proposal) = storage::get_loan_proposal(env, id) {
            if proposal.status == ProposalStatus::ApprovedPendingDisbursement
                && storage::get_loan_asset(env, id) == *asset
            {
                total += proposal.amount;
            }
        }
    }
    total
}
