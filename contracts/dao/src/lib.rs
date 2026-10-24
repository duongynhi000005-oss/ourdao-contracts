#![no_std]
//! OurDAO — a member-owned lending DAO for Stellar Soroban.
//!
//! All value moves through a single configurable token set at initialization
//! (USDC, XLM via the Stellar Asset Contract, or any Stellar asset). Four
//! additional Soroban-native modules extend the core lending/treasury flow:
//!
//! * [`registry`] name registry
//! * [`docs`] content-hash proposal metadata
//! * [`privacy`] commit-reveal voting
//! * [`staking`] voting-weight staking

mod admin;
mod docs;
mod error;
mod loans;
mod membership;
mod migration;
mod privacy;
mod registry;
mod staking;
mod storage;
mod treasury;
mod types;
mod util;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "bench-budget"))]
mod bench;

use soroban_sdk::{contract, contractimpl, Address, Bytes, BytesN, Env, String, Vec};
use types::{
    AssetBalance, DaoSummary, SeedDocEntry, SeedNameEntry, SeedStakeEntry, SeedYieldEntry,
};

pub use error::Error;
pub use storage::ProposalKind;
pub use types::{
    Loan, LoanPolicy, LoanProposal, LoanTerms, Member, MemberLoanStats, MemberOverview,
    PendingPolicyUpdate, StakingRewardClaimed, TreasuryProposal,
};

#[contract]
pub struct OurDao;

#[contractimpl]
impl OurDao {
    // ==================== lifecycle / governance ====================

    /// One-time setup. `admins` bootstrap governance, `consensus_threshold` is
    /// in basis points (e.g. 5100 = 51%), and `token` is the asset all DAO
    /// value flows through.
    pub fn initialize(
        env: Env,
        admins: Vec<Address>,
        consensus_threshold: u32,
        membership_fee: i128,
        token: Address,
        policy: LoanPolicy,
    ) -> Result<(), Error> {
        admin::initialize(
            &env,
            admins,
            consensus_threshold,
            membership_fee,
            token,
            policy,
        )
    }

    pub fn add_admin(env: Env, caller: Address, admin: Address) -> Result<(), Error> {
        admin::add_admin(&env, caller, admin)
    }

    pub fn remove_admin(env: Env, caller: Address, admin: Address) -> Result<(), Error> {
        admin::remove_admin(&env, caller, admin)
    }

    pub fn set_pauser(env: Env, caller: Address, pauser: Address) -> Result<(), Error> {
        admin::set_pauser(&env, caller, pauser)
    }

    pub fn revoke_pauser(env: Env, caller: Address) -> Result<(), Error> {
        admin::revoke_pauser(&env, caller)
    }

    pub fn set_consensus_threshold(env: Env, caller: Address, threshold: u32) -> Result<(), Error> {
        admin::set_consensus_threshold(&env, caller, threshold)
    }

    pub fn propose_policy_update(
        env: Env,
        caller: Address,
        policy: LoanPolicy,
    ) -> Result<(), Error> {
        admin::propose_policy_update(&env, caller, policy)
    }

    pub fn execute_policy_update(env: Env, caller: Address) -> Result<(), Error> {
        admin::execute_policy_update(&env, caller)
    }

    pub fn cancel_policy_update(env: Env, caller: Address) -> Result<(), Error> {
        admin::cancel_policy_update(&env, caller)
    }

    pub fn set_loan_policy(env: Env, caller: Address, policy: LoanPolicy) -> Result<(), Error> {
        admin::set_policy(&env, caller, policy)
    }

    pub fn pause(env: Env, caller: Address) -> Result<(), Error> {
        admin::pause(&env, caller)
    }

    pub fn unpause(env: Env, caller: Address) -> Result<(), Error> {
        admin::unpause(&env, caller)
    }

    // ==================== membership ====================

    pub fn register_member(env: Env, member: Address) -> Result<(), Error> {
        membership::register_member(&env, member)
    }

    pub fn exit_dao(env: Env, member: Address) -> Result<(), Error> {
        membership::exit_dao(&env, member)
    }

    /// Withdraw accrued loan-interest yield for the caller.
    pub fn claim_rewards(env: Env, member: Address) -> Result<i128, Error> {
        membership::claim_rewards(&env, member)
    }

    /// Withdraw accrued loan-interest yield in a specific whitelisted asset for the caller.
    pub fn claim_rewards_in_asset(
        env: Env,
        member: Address,
        asset: Address,
    ) -> Result<i128, Error> {
        membership::claim_rewards_in_asset(&env, member, asset)
    }

    pub fn delegate_vote(env: Env, delegator: Address, delegatee: Address) -> Result<(), Error> {
        membership::delegate_vote(&env, delegator, delegatee)
    }

    // ==================== loans ====================

    pub fn request_loan(
        env: Env,
        borrower: Address,
        amount: i128,
        metadata_cid: Option<String>,
    ) -> Result<u32, Error> {
        loans::request_loan(&env, borrower, amount, metadata_cid)
    }

    /// Requests a loan denominated in a specific whitelisted asset.
    pub fn request_loan_in_asset(
        env: Env,
        borrower: Address,
        amount: i128,
        asset: Address,
        metadata_cid: Option<String>,
    ) -> Result<u32, Error> {
        loans::request_loan_in_asset(&env, borrower, amount, asset, metadata_cid)
    }

    pub fn edit_loan_proposal(
        env: Env,
        borrower: Address,
        proposal_id: u32,
        new_amount: i128,
    ) -> Result<(), Error> {
        loans::edit_loan_proposal(&env, borrower, proposal_id, new_amount)
    }

    /// Withdraws a loan proposal while it is still in its editing period.
    /// See `loans::cancel_loan_proposal`.
    pub fn cancel_loan_proposal(
        env: Env,
        borrower: Address,
        proposal_id: u32,
    ) -> Result<(), Error> {
        loans::cancel_loan_proposal(&env, borrower, proposal_id)
    }

    pub fn vote_on_loan_proposal(
        env: Env,
        voter: Address,
        proposal_id: u32,
        support: bool,
    ) -> Result<(), Error> {
        loans::vote_on_loan_proposal(&env, voter, proposal_id, support)
    }

    pub fn vote_batch(
        env: Env,
        voter: Address,
        votes: Vec<crate::types::ProposalVote>,
    ) -> Result<(), Error> {
        loans::vote_batch(&env, voter, votes)
    }

    pub fn disburse_approved_loan(env: Env, proposal_id: u32) -> Result<(), Error> {
        loans::disburse_approved_loan(&env, proposal_id)
    }

    pub fn repay_loan(env: Env, borrower: Address, loan_id: u32) -> Result<(), Error> {
        loans::repay_loan(&env, borrower, loan_id)
    }

    /// Repays a loan using the asset the loan was denominated in.
    pub fn repay_loan_in_asset(
        env: Env,
        borrower: Address,
        loan_id: u32,
        asset: Address,
    ) -> Result<(), Error> {
        loans::repay_loan_in_asset(&env, borrower, loan_id, asset)
    }

    /// Repays up to `amount` of a loan's outstanding balance. See
    /// `loans::repay_loan_partial` for the interest/principal split and why
    /// this is a separate entrypoint from `repay_loan`.
    pub fn repay_loan_partial(
        env: Env,
        borrower: Address,
        loan_id: u32,
        amount: i128,
    ) -> Result<(), Error> {
        loans::repay_loan_partial(&env, borrower, loan_id, amount)
    }

    /// Marks an overdue loan as defaulted. Permissionless — see `loans::mark_loan_defaulted`.
    pub fn mark_loan_defaulted(env: Env, loan_id: u32) -> Result<(), Error> {
        loans::mark_loan_defaulted(&env, loan_id)
    }

    /// Permissionless keeper call: persists the expired/rejected transition for
    /// a loan proposal whose voting window has passed without reaching quorum.
    pub fn expire_loan_proposal(env: Env, proposal_id: u32) -> Result<(), Error> {
        loans::expire_loan_proposal(&env, proposal_id)
    }

    // ==================== treasury ====================

    pub fn propose_treasury_withdrawal(
        env: Env,
        proposer: Address,
        amount: i128,
        destination: Address,
        reason: String,
        private: bool,
    ) -> Result<u32, Error> {
        treasury::propose_withdrawal(&env, proposer, amount, destination, reason, private)
    }

    /// Proposes a treasury withdrawal denominated in a specific whitelisted asset.
    /// Named without the `treasury_` prefix because Soroban caps entrypoint
    /// names at 32 characters.
    pub fn propose_withdrawal_in_asset(
        env: Env,
        proposer: Address,
        amount: i128,
        asset: Address,
        destination: Address,
        reason: String,
        private: bool,
    ) -> Result<u32, Error> {
        treasury::propose_withdrawal_in_asset(
            &env,
            proposer,
            amount,
            asset,
            destination,
            reason,
            private,
        )
    }

    pub fn vote_on_treasury_proposal(
        env: Env,
        voter: Address,
        proposal_id: u32,
        support: bool,
    ) -> Result<(), Error> {
        treasury::vote(&env, voter, proposal_id, support)
    }

    /// Withdraws a pending treasury proposal within the policy's editing
    /// period. See `treasury::cancel_treasury_proposal`.
    pub fn cancel_treasury_proposal(
        env: Env,
        proposer: Address,
        proposal_id: u32,
    ) -> Result<(), Error> {
        treasury::cancel_treasury_proposal(&env, proposer, proposal_id)
    }

    pub fn expire_treasury_proposal(env: Env, proposal_id: u32) -> Result<(), Error> {
        treasury::expire_treasury_proposal(&env, proposal_id)
    }

    pub fn execute_treasury_proposal(env: Env, proposal_id: u32) -> Result<(), Error> {
        treasury::execute_approved(&env, proposal_id)
    }

    // ==================== maintenance ====================

    #[allow(deprecated)]
    pub fn bump_dao_ttl(env: Env) {
        storage::extend_instance(&env);
        env.events()
            .publish((soroban_sdk::symbol_short!("TtlBumped"),), ());
    }

    // ==================== native swap: staking ====================

    pub fn stake(env: Env, member: Address, amount: i128) -> Result<(), Error> {
        staking::stake(&env, member, amount)
    }

    pub fn unstake(env: Env, member: Address, amount: i128) -> Result<(), Error> {
        staking::unstake(&env, member, amount)
    }

    // ==================== native swap: name registry ====================

    pub fn register_name(env: Env, owner: Address, name: String) -> Result<(), Error> {
        registry::register_name(&env, owner, name)
    }

    pub fn resolve_name(env: Env, name: String) -> Result<Option<Address>, Error> {
        registry::resolve_name(&env, name)
    }

    pub fn name_of(env: Env, owner: Address) -> Result<Option<String>, Error> {
        registry::name_of(&env, owner)
    }

    // ==================== native swap: commit-reveal voting ====================

    pub fn commit_treasury_vote(
        env: Env,
        voter: Address,
        proposal_id: u32,
        commitment: BytesN<32>,
    ) -> Result<(), Error> {
        privacy::commit_vote(&env, voter, proposal_id, commitment)
    }

    pub fn reveal_treasury_vote(
        env: Env,
        voter: Address,
        proposal_id: u32,
        support: bool,
        salt: BytesN<32>,
    ) -> Result<(), Error> {
        privacy::reveal_vote(&env, voter, proposal_id, support, salt)
    }

    // ==================== native swap: content-hash docs ====================

    pub fn attach_document(
        env: Env,
        caller: Address,
        kind: ProposalKind,
        proposal_id: u32,
        content_hash: Bytes,
    ) -> Result<(), Error> {
        docs::attach_document(&env, caller, kind, proposal_id, content_hash)
    }

    pub fn get_document(
        env: Env,
        kind: ProposalKind,
        proposal_id: u32,
    ) -> Result<Option<Bytes>, Error> {
        docs::get_document(&env, kind, proposal_id)
    }

    // ==================== views ====================
    //
    // Rule: a view that reads DAO configuration or the registry returns
    // `Err(Error::NotInitialized)` on an uninitialized contract. Per-key
    // lookups (`get_member`, `get_loan`, ...) keep returning `None`; use
    // `is_initialized` to tell a missing record from a wrong address.

    pub fn get_member(env: Env, address: Address) -> Option<Member> {
        storage::get_member(&env, &address)
    }

    /// Returns `member`'s lifetime loan track record: total loans taken, loans
    /// fully repaid, and loans currently outstanding. Single O(1) storage read;
    /// returns `NotMember` if the address has never registered.
    pub fn get_member_loan_stats(env: Env, member: Address) -> Result<MemberLoanStats, Error> {
        storage::get_member(&env, &member)
            .map(|m| MemberLoanStats {
                total_loans: m.total_loans,
                repaid_loans: m.repaid_loans,
                active_loans: m.active_loans,
            })
            .ok_or(Error::NotMember)
    }

    /// Returns everything about `member` in one call: their record, stake,
    /// unclaimed yield, loan track record, registered name and delegate.
    /// Returns `NotMember` if the address has never registered.
    pub fn get_member_overview(env: Env, member: Address) -> Result<MemberOverview, Error> {
        membership::member_overview(&env, &member)
    }

    pub fn get_loan(env: Env, loan_id: u32) -> Option<Loan> {
        storage::get_loan(&env, loan_id)
    }

    pub fn get_loan_proposal(env: Env, proposal_id: u32) -> Option<LoanProposal> {
        storage::get_loan_proposal(&env, proposal_id).map(|p| loans::refresh_phase(&env, p))
    }

    pub fn get_treasury_proposal(env: Env, proposal_id: u32) -> Option<TreasuryProposal> {
        storage::get_treasury_proposal(&env, proposal_id)
    }

    pub fn get_loan_proposal_count(env: Env) -> u32 {
        storage::get_proposal_count(&env, storage::DataKey::NextProposalId)
    }

    pub fn get_treasury_proposal_count(env: Env) -> u32 {
        storage::get_proposal_count(&env, storage::DataKey::NextTreasuryId)
    }

    pub fn get_loan_policy(env: Env) -> Result<LoanPolicy, Error> {
        util::require_initialized(&env)?;
        Ok(storage::get_policy(&env))
    }

    pub fn get_pending_policy_update(env: Env) -> Option<PendingPolicyUpdate> {
        admin::get_pending_policy_update(&env)
    }

    pub fn get_admins(env: Env) -> Result<Vec<Address>, Error> {
        util::require_initialized(&env)?;
        Ok(storage::get_admins(&env))
    }

    pub fn is_admin(env: Env, address: Address) -> Result<bool, Error> {
        util::require_initialized(&env)?;
        Ok(util::is_admin(&env, &address))
    }

    pub fn is_member(env: Env, address: Address) -> bool {
        matches!(
            storage::get_member(&env, &address),
            Some(m) if m.status == types::MemberStatus::ActiveMember
        )
    }

    pub fn is_eligible_for_loan(env: Env, member: Address) -> Result<(), Error> {
        loans::is_eligible_for_loan(&env, &member)
    }

    pub fn get_treasury_balance(env: Env) -> Result<i128, Error> {
        util::require_initialized(&env)?;
        Ok(util::treasury_balance(&env))
    }

    /// Returns the treasury balance held in a specific whitelisted asset.
    pub fn get_treasury_balance_in_asset(env: Env, asset: Address) -> i128 {
        util::treasury_balance_in_asset(&env, &asset)
    }

    pub fn get_total_members(env: Env) -> u32 {
        storage::get_total_members(&env)
    }

    pub fn get_active_members(env: Env) -> u32 {
        storage::get_active_members(&env)
    }

    pub fn get_consensus_threshold(env: Env) -> Result<u32, Error> {
        util::require_initialized(&env)?;
        Ok(storage::get_threshold(&env))
    }

    pub fn get_token(env: Env) -> Result<Address, Error> {
        util::require_initialized(&env)?;
        Ok(storage::get_token(&env))
    }

    /// Whether `initialize` has run. Clients should check this before relying
    /// on any other view: every state-reading view returns
    /// `Error::NotInitialized` on an uninitialized contract rather than
    /// trapping or returning an ambiguous empty value.
    pub fn is_initialized(env: Env) -> bool {
        storage::is_initialized(&env)
    }

    /// Headline DAO state in one call (#339): membership counts, spendable
    /// treasury per whitelisted asset, total staked, threshold, pause flag and
    /// version. Each field matches the individual view it summarises.
    pub fn get_dao_summary(env: Env) -> DaoSummary {
        let mut treasury_balances = Vec::new(&env);
        for asset in storage::get_whitelisted_tokens(&env).iter() {
            let balance = util::treasury_balance_in_asset(&env, &asset);
            treasury_balances.push_back(AssetBalance { asset, balance });
        }
        DaoSummary {
            total_members: storage::get_total_members(&env),
            active_members: storage::get_active_members(&env),
            treasury_balance: util::treasury_balance(&env),
            treasury_balances,
            total_staked: storage::get_total_staked(&env),
            consensus_threshold: storage::get_threshold(&env),
            paused: storage::is_paused(&env),
            version: String::from_str(&env, env!("CARGO_PKG_VERSION")),
        }
    }

    // ==================== token whitelist ====================

    /// Proposes adding `token` to the treasury's supported-asset whitelist.
    /// Requires admin consensus; see `admin::propose_token_addition`.
    pub fn propose_token_addition(env: Env, caller: Address, token: Address) -> Result<(), Error> {
        admin::propose_token_addition(&env, caller, token)
    }

    /// Executes a previously proposed token addition once consensus is met.
    pub fn execute_token_addition(env: Env, caller: Address, token: Address) -> Result<(), Error> {
        admin::execute_token_addition(&env, caller, token)
    }

    /// Removes `token` from the supported-asset whitelist.
    pub fn remove_token(env: Env, caller: Address, token: Address) -> Result<(), Error> {
        admin::remove_token(&env, caller, token)
    }

    /// Returns whether `token` is currently an approved treasury asset.
    pub fn is_token_whitelisted(env: Env, token: Address) -> bool {
        storage::is_token_whitelisted(&env, &token)
    }

    /// Returns the full list of approved treasury assets.
    pub fn get_whitelisted_tokens(env: Env) -> Vec<Address> {
        storage::get_whitelisted_tokens(&env)
    }

    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    /// Returns this contract's semver, read from CARGO_PKG_VERSION at build
    /// time (#197) — lets off-chain tooling / indexers detect which
    /// contract build a given deployment is running without relying on the
    /// WASM hash alone.
    pub fn get_version(env: Env) -> String {
        String::from_str(&env, env!("CARGO_PKG_VERSION"))
    }

    pub fn get_stake(env: Env, member: Address) -> i128 {
        storage::get_stake(&env, &member)
    }

    /// Returns `member`'s current voting weight (base vote + staking bonus).
    /// Equivalent to the weight applied when they cast a loan or treasury vote.
    pub fn get_voting_weight(env: Env, member: Address) -> i128 {
        util::get_voting_weight(&env, &member)
    }

    /// Stake tokens required per additional unit of voting bonus.
    /// Every `stake_weight_unit` tokens staked grants +1 vote, up to the cap.
    pub fn get_stake_weight_unit(_env: Env) -> i128 {
        util::get_stake_weight_unit()
    }

    /// Maximum additional votes a member can earn through staking.
    pub fn get_max_stake_bonus(_env: Env) -> i128 {
        util::get_max_stake_bonus()
    }

    pub fn get_pending_yield(env: Env, member: Address) -> i128 {
        membership::compute_pending_yield(&env, &member)
    }

    pub fn get_pending_yield_in_asset(env: Env, member: Address, asset: Address) -> i128 {
        membership::compute_pending_yield_for(&env, &member, &asset)
    }

    /// Returns the address `delegator` has delegated their vote to, or `None`
    /// if they have never called `delegate_vote`.
    pub fn get_delegate(env: Env, delegator: Address) -> Option<Address> {
        storage::get_delegation(&env, &delegator)
    }

    /// Returns whether `voter` has cast a (possibly not-yet-revealed) vote
    /// on the given proposal. For public (non-private) proposals this is a
    /// simple check against the vote record. For commit-reveal treasury
    /// proposals it checks whether the voter has committed (which is the
    /// earliest point at which a double-vote is prevented).
    pub fn has_voted(env: Env, kind: ProposalKind, proposal_id: u32, voter: Address) -> bool {
        match kind {
            ProposalKind::Loan => storage::has_loan_voted(&env, proposal_id, &voter),
            ProposalKind::Treasury => {
                // For private (commit-reveal) proposals, a commitment is
                // sufficient to report "voted" — the double-vote guard is
                // enforced at commit time, and the client needs to know the
                // member can't vote again regardless of reveal status.
                storage::has_treasury_voted(&env, proposal_id, &voter)
                    || storage::get_commit(&env, proposal_id, &voter).is_some()
            }
        }
    }

    pub fn calculate_loan_terms(env: Env, amount: i128) -> Result<LoanTerms, Error> {
        util::require_initialized(&env)?;
        Ok(loans::calculate_loan_terms(&env, amount))
    }

    pub fn calculate_exit_share(env: Env, member: Address) -> Result<i128, Error> {
        util::require_initialized(&env)?;
        Ok(membership::calculate_exit_share(&env, &member))
    }

    // ==================== paginated list views (#334 / #335) ====================

    /// Returns up to `limit` member records starting at offset `start` in the
    /// internal member-address list. Returns an empty vec when `start` is past
    /// the end.  Skips addresses whose persistent storage entry has expired.
    pub fn list_members(env: Env, start: u32, limit: u32) -> Vec<Member> {
        let all = storage::get_members(&env);
        let mut out = Vec::new(&env);
        let len = all.len();
        let from = start.min(len);
        let to = (start + limit).min(len);
        for i in from..to {
            let addr = all.get(i).unwrap();
            if let Some(m) = storage::get_member(&env, &addr) {
                out.push_back(m);
            }
        }
        out
    }

    /// Returns up to `limit` treasury proposals starting at proposal id `start`.
    /// Ids are sequential from 0; absent entries (expired from storage) are
    /// skipped.  Returns an empty vec when `start` is past the last proposal.
    pub fn list_treasury_proposals(env: Env, start: u32, limit: u32) -> Vec<TreasuryProposal> {
        let count = storage::get_proposal_count(&env, storage::DataKey::NextTreasuryId);
        let mut out = Vec::new(&env);
        let from = start.min(count);
        let to = (start + limit).min(count);
        for id in from..to {
            if let Some(p) = storage::get_treasury_proposal(&env, id) {
                out.push_back(p);
            }
        }
        out
    }

    // ==================== migration seed entrypoints (#332 / #333) ====================

    /// Seeds member records from a prior deployment. Admin-only; rejected after
    /// `lock_seeding()` is called. See `docs/MIGRATION.md` for the full procedure.
    pub fn seed_members(env: Env, caller: Address, members: Vec<Member>) -> Result<(), Error> {
        migration::seed_members(&env, caller, members)
    }

    pub fn seed_stakes(
        env: Env,
        caller: Address,
        stakes: Vec<SeedStakeEntry>,
    ) -> Result<(), Error> {
        migration::seed_stakes(&env, caller, stakes)
    }

    pub fn seed_pending_yield(
        env: Env,
        caller: Address,
        yields: Vec<SeedYieldEntry>,
    ) -> Result<(), Error> {
        migration::seed_pending_yield(&env, caller, yields)
    }

    pub fn seed_loans(env: Env, caller: Address, loans: Vec<Loan>) -> Result<(), Error> {
        migration::seed_loans(&env, caller, loans)
    }

    pub fn seed_names(env: Env, caller: Address, names: Vec<SeedNameEntry>) -> Result<(), Error> {
        migration::seed_names(&env, caller, names)
    }

    pub fn seed_docs(env: Env, caller: Address, docs: Vec<SeedDocEntry>) -> Result<(), Error> {
        migration::seed_docs(&env, caller, docs)
    }

    /// Irreversibly closes the seeding window. All `seed_*` entrypoints reject
    /// calls after this. Admin-only. Returns `SeedingAlreadyLocked` if already
    /// locked.
    pub fn lock_seeding(env: Env, caller: Address) -> Result<(), Error> {
        migration::lock_seeding(&env, caller)
    }

    /// Returns `true` while the seeding window is open (before `lock_seeding`
    /// is called).
    pub fn is_seeding_locked(env: Env) -> bool {
        storage::is_seeding_locked(&env)
    }
}
