use soroban_sdk::{contracttype, Address, BytesN, Env, String, Vec};

use crate::types::{Loan, LoanPolicy, LoanProposal, Member, PendingPolicyUpdate, TreasuryProposal};

// Soroban produces one ledger every ~5 seconds.
const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_BUMP: u32 = 30 * DAY_IN_LEDGERS;
const INSTANCE_THRESHOLD: u32 = INSTANCE_BUMP - DAY_IN_LEDGERS;
const PERSISTENT_BUMP: u32 = 90 * DAY_IN_LEDGERS;
const PERSISTENT_THRESHOLD: u32 = PERSISTENT_BUMP - DAY_IN_LEDGERS;

/// Distinguishes which proposal family a shared reference (docs, commits) targets.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalKind {
    Loan,
    Treasury,
}

/// Storage keys. Crate-private on purpose: `#[contracttype]` only writes
/// `pub` types into the contract spec, and these keys are an internal storage
/// layout, not part of the callable interface. Keeping them out of the spec
/// saves ~1.5 KB of wasm (#357). Variants still encode by name on the ledger, so
/// renaming one orphans every entry already stored under it.
#[contracttype]
#[derive(Clone)]
pub(crate) enum DataKey {
    // ---- singletons (instance storage) ----
    Admins,
    Threshold,
    MembershipFee,
    Token,
    Policy,
    Paused,
    Pauser,
    Members,
    TotalMembers,
    ActiveMembers,
    NextProposalId,
    NextTreasuryId,
    TotalStaked,
    PendingPolicyUpdate,

    // ---- per-entity (persistent storage) ----
    Member(Address),
    LoanProposal(u32),
    Loan(u32),
    TreasuryProposal(u32),
    LoanVoted(u32, Address),
    StakeTime(Address),
    TreasuryVoted(u32, Address),
    PendingYield(Address),
    Stake(Address),
    /// Cached count, refreshed from live proposal state before quota checks.
    OpenProposals(Address),
    // ---- pull-based yield accrual ----
    YieldAccumulator,
    YieldRemainder,
    MemberYieldSnapshot(Address),
    // native-swap modules
    Doc(ProposalKind, u32),
    Name(String),
    NameOf(Address),
    Commit(u32, Address),
    TotalContributions,
    /// Running sum of `contribution` over members whose status is
    /// `ActiveMember` (#353). Kept in sync by register / exit / default.
    ActiveContributions,
    Delegation(Address),
    // ---- multi-token treasury (#236) ----
    /// Present (and `true`) while an extra asset is whitelisted. The base
    /// token is implicitly whitelisted and never stored here.
    TokenWhitelisted(Address),
    /// Extra whitelisted assets, in the order they were added.
    WhitelistedTokens,
    /// Earliest timestamp a proposed asset addition may be executed.
    PendingTokenAddition(Address),
    /// Asset a loan proposal (and the loan it becomes, which shares its id)
    /// is denominated in. Absent means the base token.
    LoanAsset(u32),
    /// Asset a treasury proposal pays out in. Absent means the base token.
    TreasuryAsset(u32),
    /// Set (to `true`) once by `lock_seeding`; never cleared. Seed entrypoints
    /// reject any call made after this flag is present (#332/#333).
    MigrationSeedingLocked,
}

pub fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_THRESHOLD, INSTANCE_BUMP);
}

fn extend_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
}

// ---------- singleton config accessors ----------

pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admins)
}

pub fn get_admins(env: &Env) -> Vec<Address> {
    env.storage().instance().get(&DataKey::Admins).unwrap()
}

pub fn set_admins(env: &Env, admins: &Vec<Address>) {
    env.storage().instance().set(&DataKey::Admins, admins);
}

pub fn get_pauser(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::Pauser)
}

pub fn set_pauser(env: &Env, pauser: &Address) {
    env.storage().instance().set(&DataKey::Pauser, pauser);
}

pub fn remove_pauser(env: &Env) {
    env.storage().instance().remove(&DataKey::Pauser);
}

pub fn get_threshold(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::Threshold).unwrap()
}

pub fn set_threshold(env: &Env, t: u32) {
    env.storage().instance().set(&DataKey::Threshold, &t);
}

pub fn get_membership_fee(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::MembershipFee)
        .unwrap()
}

pub fn set_membership_fee(env: &Env, fee: i128) {
    env.storage().instance().set(&DataKey::MembershipFee, &fee);
}

pub fn get_token(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Token).unwrap()
}

pub fn set_token(env: &Env, token: &Address) {
    env.storage().instance().set(&DataKey::Token, token);
}

pub fn get_policy(env: &Env) -> LoanPolicy {
    env.storage().instance().get(&DataKey::Policy).unwrap()
}

pub fn set_policy(env: &Env, policy: &LoanPolicy) {
    env.storage().instance().set(&DataKey::Policy, policy);
}

pub fn get_pending_policy_update(env: &Env) -> Option<PendingPolicyUpdate> {
    env.storage().instance().get(&DataKey::PendingPolicyUpdate)
}

pub fn set_pending_policy_update(env: &Env, update: &PendingPolicyUpdate) {
    env.storage()
        .instance()
        .set(&DataKey::PendingPolicyUpdate, update);
}

pub fn remove_pending_policy_update(env: &Env) {
    env.storage()
        .instance()
        .remove(&DataKey::PendingPolicyUpdate);
}

pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::Paused, &paused);
}

pub fn get_members(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::Members)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_members(env: &Env, members: &Vec<Address>) {
    env.storage().instance().set(&DataKey::Members, members);
}

pub fn get_total_members(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::TotalMembers)
        .unwrap_or(0)
}

pub fn set_total_members(env: &Env, n: u32) {
    env.storage().instance().set(&DataKey::TotalMembers, &n);
}

#[allow(dead_code)]
pub fn get_total_contributions(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalContributions)
        .unwrap_or(0)
}

#[allow(dead_code)]
pub fn set_total_contributions(env: &Env, amount: i128) {
    env.storage()
        .instance()
        .set(&DataKey::TotalContributions, &amount);
}

pub fn get_active_contributions(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::ActiveContributions)
        .unwrap_or(0)
}

pub fn set_active_contributions(env: &Env, amount: i128) {
    env.storage()
        .instance()
        .set(&DataKey::ActiveContributions, &amount);
}

pub fn get_active_members(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::ActiveMembers)
        .unwrap_or(0)
}

pub fn set_active_members(env: &Env, n: u32) {
    env.storage().instance().set(&DataKey::ActiveMembers, &n);
}

/// Atomically fetches and increments one of the id counters, returning the id to use.
pub fn next_id(env: &Env, key: DataKey) -> u32 {
    let current: u32 = env.storage().instance().get(&key).unwrap_or(0);
    env.storage().instance().set(&key, &(current + 1));
    current
}

pub fn get_proposal_count(env: &Env, key: DataKey) -> u32 {
    env.storage().instance().get(&key).unwrap_or(0)
}

pub fn get_total_staked(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0)
}

pub fn set_total_staked(env: &Env, amount: i128) {
    env.storage().instance().set(&DataKey::TotalStaked, &amount);
}

// ---------- per-entity accessors ----------

pub fn get_member(env: &Env, addr: &Address) -> Option<Member> {
    let key = DataKey::Member(addr.clone());
    let m = env.storage().persistent().get(&key);
    if m.is_some() {
        extend_persistent(env, &key);
    }
    m
}

pub fn set_member(env: &Env, member: &Member) {
    let key = DataKey::Member(member.address.clone());
    env.storage().persistent().set(&key, member);
    extend_persistent(env, &key);
}

/// Bound how many unclosed proposals one member may have at a time.
pub const MAX_OPEN_PROPOSALS_PER_MEMBER: u32 = 3;

/// Refresh the member's quota from authoritative proposal states. Expired
/// pending records do not consume quota even when no keeper has finalized them.
pub fn get_open_proposals(env: &Env, member: &Address) -> u32 {
    let now = env.ledger().timestamp();
    let mut count: u32 = 0;
    for id in 0..get_proposal_count(env, DataKey::NextProposalId) {
        if let Some(p) = get_loan_proposal(env, id) {
            if &p.borrower == member
                && p.status == crate::types::ProposalStatus::Pending
                && now <= p.editing_period_end.saturating_add(p.voting_period)
            {
                count = count.saturating_add(1);
            }
        }
    }
    for id in 0..get_proposal_count(env, DataKey::NextTreasuryId) {
        if let Some(p) = get_treasury_proposal(env, id) {
            if &p.proposer == member
                && p.status == crate::types::ProposalStatus::Pending
                && now
                    <= p.created_at
                        .saturating_add(p.voting_period.saturating_mul(if p.private {
                            2
                        } else {
                            1
                        }))
            {
                count = count.saturating_add(1);
            }
        }
    }
    let key = DataKey::OpenProposals(member.clone());
    env.storage().persistent().set(&key, &count);
    extend_persistent(env, &key);
    count
}

pub fn get_loan_proposal(env: &Env, id: u32) -> Option<LoanProposal> {
    let key = DataKey::LoanProposal(id);
    let p = env.storage().persistent().get(&key);
    if p.is_some() {
        extend_persistent(env, &key);
    }
    p
}

pub fn set_loan_proposal(env: &Env, p: &LoanProposal) {
    let key = DataKey::LoanProposal(p.id);
    env.storage().persistent().set(&key, p);
    extend_persistent(env, &key);
    get_open_proposals(env, &p.borrower);
}

pub fn get_loan(env: &Env, id: u32) -> Option<Loan> {
    let key = DataKey::Loan(id);
    let l = env.storage().persistent().get(&key);
    if l.is_some() {
        extend_persistent(env, &key);
    }
    l
}

pub fn set_loan(env: &Env, l: &Loan) {
    let key = DataKey::Loan(l.id);
    env.storage().persistent().set(&key, l);
    extend_persistent(env, &key);
}

pub fn get_treasury_proposal(env: &Env, id: u32) -> Option<TreasuryProposal> {
    let key = DataKey::TreasuryProposal(id);
    let p = env.storage().persistent().get(&key);
    if p.is_some() {
        extend_persistent(env, &key);
    }
    p
}

pub fn set_treasury_proposal(env: &Env, p: &TreasuryProposal) {
    let key = DataKey::TreasuryProposal(p.id);
    env.storage().persistent().set(&key, p);
    extend_persistent(env, &key);
    get_open_proposals(env, &p.proposer);
}

pub fn has_loan_voted(env: &Env, id: u32, voter: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::LoanVoted(id, voter.clone()))
}

pub fn set_loan_voted(env: &Env, id: u32, voter: &Address) {
    let key = DataKey::LoanVoted(id, voter.clone());
    env.storage().persistent().set(&key, &true);
    extend_persistent(env, &key);
}

pub fn has_treasury_voted(env: &Env, id: u32, voter: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::TreasuryVoted(id, voter.clone()))
}

pub fn set_treasury_voted(env: &Env, id: u32, voter: &Address) {
    let key = DataKey::TreasuryVoted(id, voter.clone());
    env.storage().persistent().set(&key, &true);
    extend_persistent(env, &key);
}

pub fn get_stake(env: &Env, addr: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Stake(addr.clone()))
        .unwrap_or(0)
}

pub fn set_stake(env: &Env, addr: &Address, amount: i128) {
    let key = DataKey::Stake(addr.clone());
    env.storage().persistent().set(&key, &amount);
    extend_persistent(env, &key);
}

// ---------- pull-based yield accrual ----------

pub fn get_yield_accumulator(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::YieldAccumulator)
        .unwrap_or(0)
}

pub fn set_yield_accumulator(env: &Env, value: i128) {
    env.storage()
        .instance()
        .set(&DataKey::YieldAccumulator, &value);
}

pub fn get_yield_snapshot(env: &Env, addr: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::MemberYieldSnapshot(addr.clone()))
        .unwrap_or(0)
}

pub fn set_yield_snapshot(env: &Env, addr: &Address, value: i128) {
    let key = DataKey::MemberYieldSnapshot(addr.clone());
    env.storage().persistent().set(&key, &value);
    extend_persistent(env, &key);
}

// ---------- native-swap module accessors ----------

pub fn get_doc(env: &Env, kind: ProposalKind, id: u32) -> Option<soroban_sdk::Bytes> {
    env.storage().persistent().get(&DataKey::Doc(kind, id))
}

pub fn set_doc(env: &Env, kind: ProposalKind, id: u32, hash: &soroban_sdk::Bytes) {
    let key = DataKey::Doc(kind, id);
    env.storage().persistent().set(&key, hash);
    extend_persistent(env, &key);
}

pub fn get_name_owner(env: &Env, name: &String) -> Option<Address> {
    env.storage().persistent().get(&DataKey::Name(name.clone()))
}

pub fn set_name(env: &Env, name: &String, owner: &Address) {
    let nkey = DataKey::Name(name.clone());
    let okey = DataKey::NameOf(owner.clone());
    env.storage().persistent().set(&nkey, owner);
    env.storage().persistent().set(&okey, name);
    extend_persistent(env, &nkey);
    extend_persistent(env, &okey);
}

pub fn get_name_of(env: &Env, owner: &Address) -> Option<String> {
    env.storage()
        .persistent()
        .get(&DataKey::NameOf(owner.clone()))
}

pub fn get_commit(env: &Env, id: u32, voter: &Address) -> Option<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&DataKey::Commit(id, voter.clone()))
}

pub fn set_commit(env: &Env, id: u32, voter: &Address, commitment: &BytesN<32>) {
    let key = DataKey::Commit(id, voter.clone());
    env.storage().persistent().set(&key, commitment);
    extend_persistent(env, &key);
}

pub fn remove_commit(env: &Env, id: u32, voter: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::Commit(id, voter.clone()));
}

pub fn get_yield_remainder(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::YieldRemainder)
        .unwrap_or(0)
}

pub fn set_yield_remainder(env: &Env, value: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::YieldRemainder, &value);
}

pub fn get_delegation(env: &Env, member: &Address) -> Option<Address> {
    let key = DataKey::Delegation(member.clone());
    let res = env.storage().persistent().get(&key);
    if res.is_some() {
        extend_persistent(env, &key);
    }
    res
}

pub fn set_delegation(env: &Env, member: &Address, delegatee: &Address) {
    let key = DataKey::Delegation(member.clone());
    env.storage().persistent().set(&key, delegatee);
    extend_persistent(env, &key);
}

pub fn remove_delegation(env: &Env, member: &Address) {
    let key = DataKey::Delegation(member.clone());
    env.storage().persistent().remove(&key);
}

// ---------- multi-token treasury (#236) ----------

/// The base token is always whitelisted; any other asset only while its
/// `TokenWhitelisted` entry exists.
pub fn is_token_whitelisted(env: &Env, token: &Address) -> bool {
    let base: Option<Address> = env.storage().instance().get(&DataKey::Token);
    if base.as_ref() == Some(token) {
        return true;
    }
    let key = DataKey::TokenWhitelisted(token.clone());
    let listed = env.storage().persistent().has(&key);
    if listed {
        extend_persistent(env, &key);
    }
    listed
}

/// Adds (`true`) or removes (`false`) an extra asset, keeping the
/// `WhitelistedTokens` list in step. Idempotent in both directions.
pub fn set_token_whitelisted(env: &Env, token: &Address, whitelisted: bool) {
    let key = DataKey::TokenWhitelisted(token.clone());
    let mut tokens: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::WhitelistedTokens)
        .unwrap_or_else(|| Vec::new(env));
    if whitelisted {
        env.storage().persistent().set(&key, &true);
        extend_persistent(env, &key);
        if !tokens.contains(token) {
            tokens.push_back(token.clone());
        }
    } else {
        env.storage().persistent().remove(&key);
        if let Some(i) = tokens.first_index_of(token) {
            tokens.remove(i);
        }
    }
    env.storage()
        .persistent()
        .set(&DataKey::WhitelistedTokens, &tokens);
    extend_persistent(env, &DataKey::WhitelistedTokens);
}

/// Every accepted asset: the base token first, then extras in insertion order.
pub fn get_whitelisted_tokens(env: &Env) -> Vec<Address> {
    let mut all = Vec::new(env);
    if let Some(base) = env.storage().instance().get::<_, Address>(&DataKey::Token) {
        all.push_back(base);
    }
    let key = DataKey::WhitelistedTokens;
    if let Some(extra) = env.storage().persistent().get::<_, Vec<Address>>(&key) {
        extend_persistent(env, &key);
        all.append(&extra);
    }
    all
}

pub fn get_pending_token_addition(env: &Env, token: &Address) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::PendingTokenAddition(token.clone()))
}

pub fn set_pending_token_addition(env: &Env, token: &Address, execution_time: u64) {
    let key = DataKey::PendingTokenAddition(token.clone());
    env.storage().persistent().set(&key, &execution_time);
    extend_persistent(env, &key);
}

pub fn remove_pending_token_addition(env: &Env, token: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::PendingTokenAddition(token.clone()));
}

/// Asset of loan proposal / loan `id`; the base token unless recorded.
pub fn get_loan_asset(env: &Env, id: u32) -> Address {
    let key = DataKey::LoanAsset(id);
    match env.storage().persistent().get(&key) {
        Some(asset) => {
            extend_persistent(env, &key);
            asset
        }
        None => get_token(env),
    }
}

pub fn set_loan_asset(env: &Env, id: u32, asset: &Address) {
    let key = DataKey::LoanAsset(id);
    env.storage().persistent().set(&key, asset);
    extend_persistent(env, &key);
}

/// Asset of treasury proposal `id`; the base token unless recorded.
pub fn get_treasury_asset(env: &Env, id: u32) -> Address {
    let key = DataKey::TreasuryAsset(id);
    match env.storage().persistent().get(&key) {
        Some(asset) => {
            extend_persistent(env, &key);
            asset
        }
        None => get_token(env),
    }
}

pub fn set_treasury_asset(env: &Env, id: u32, asset: &Address) {
    let key = DataKey::TreasuryAsset(id);
    env.storage().persistent().set(&key, asset);
    extend_persistent(env, &key);
}

// ---------- migration seeding lock (#332 / #333) ----------

pub fn is_seeding_locked(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::MigrationSeedingLocked)
        .unwrap_or(false)
}

pub fn lock_seeding(env: &Env) {
    env.storage()
        .instance()
        .set(&DataKey::MigrationSeedingLocked, &true);
}

pub fn set_next_proposal_id_at_least(env: &Env, min: u32) {
    let current: u32 = env
        .storage()
        .instance()
        .get(&DataKey::NextProposalId)
        .unwrap_or(0);
    if min > current {
        env.storage().instance().set(&DataKey::NextProposalId, &min);
    }
}

pub fn set_next_treasury_id_at_least(env: &Env, min: u32) {
    let current: u32 = env
        .storage()
        .instance()
        .get(&DataKey::NextTreasuryId)
        .unwrap_or(0);
    if min > current {
        env.storage().instance().set(&DataKey::NextTreasuryId, &min);
    }
}
