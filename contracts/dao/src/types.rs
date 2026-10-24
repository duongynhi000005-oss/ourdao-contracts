use soroban_sdk::{contracttype, Address, Bytes, String, Vec};

/// Basis-points denominator (100% == 10_000).
pub const BASIS_POINTS: i128 = 10_000;
/// Alias for basis-points divisor.
#[allow(dead_code)]
pub const BPS_DIVISOR: i128 = BASIS_POINTS;
/// Basis-points ceiling round-up increment (BASIS_POINTS - 1).
pub const BPS_CEIL_INCREMENT: i128 = BASIS_POINTS - 1;
/// Maximum percentage represented in basis points (100% == 10_000).
#[allow(dead_code)]
pub const MAX_BPS: u32 = 10_000;
/// Default threshold in basis points (51% == 5_100).
#[allow(dead_code)]
pub const DEFAULT_THRESHOLD_BPS: u32 = 5_100;

/// Number of seconds in one minute (60s).
#[allow(dead_code)]
pub const SECONDS_PER_MINUTE: u64 = 60;
/// Number of seconds in one hour (3_600s).
#[allow(dead_code)]
pub const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
/// Number of seconds in one day (86_400s).
#[allow(dead_code)]
pub const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;

/// Multiplier for private proposal voting duration (commit phase + reveal phase = 2x).
pub const PRIVATE_PROPOSAL_PERIOD_MULTIPLIER: u64 = 2;
/// Maximum length of proposal reason string in bytes.
pub const MAX_PROPOSAL_REASON_LEN: u32 = 256;
/// Maximum length of metadata CID string in bytes.
pub const MAX_METADATA_CID_LEN: u32 = 64;
pub const NAME_MIN_LEN: u32 = 3;
/// Maximum length for a registered name (inclusive). Keeps the on-chain
/// storage and every `name_reg` event payload bounded.
pub const NAME_MAX_LEN: u32 = 32;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemberStatus {
    ActiveMember,
    Inactive,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    Pending,
    ApprovedPendingDisbursement,
    Approved,
    Rejected,
    Executed,
    Expired,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalPhase {
    Editing,
    Voting,
    Executed,
    Expired,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoanStatus {
    Active,
    Repaid,
    Defaulted,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Member {
    pub address: Address,
    pub status: MemberStatus,
    pub join_time: u64,
    pub contribution: i128,
    pub share_balance: i128,
    pub has_active_loan: bool,
    pub last_loan_time: u64,
    /// Lifetime count of loans disbursed to this member.
    pub total_loans: u32,
    /// Lifetime count of this member's loans that reached full repayment.
    pub repaid_loans: u32,
    /// Loans currently outstanding (disbursed and not yet repaid or defaulted).
    pub active_loans: u32,
}

/// Everything the DAO knows about one member, returned by
/// `get_member_overview` in a single call.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberOverview {
    pub member: Member,
    /// Tokens the member has staked for a voting-weight bonus.
    pub stake: i128,
    /// Interest yield accrued and not yet claimed via `claim_rewards`.
    pub pending_yield: i128,
    pub loan_stats: MemberLoanStats,
    /// Name registered to this address, if any.
    pub name: Option<String>,
    /// Address this member delegated their vote to, if any.
    pub delegate: Option<Address>,
}

/// On-chain credit track record for a member. Returned by
/// `get_member_loan_stats` as an O(1) read of the member record.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemberLoanStats {
    pub total_loans: u32,
    pub repaid_loans: u32,
    pub active_loans: u32,
}

/// Tunable lending parameters. Durations are in ledger seconds; rates in bps.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanPolicy {
    pub min_membership_duration: u64,
    pub membership_contribution: i128,
    pub max_loan_duration: u64,
    pub min_interest_rate: u32,
    pub max_interest_rate: u32,
    pub cooldown_period: u64,
    pub max_loan_to_treasury_ratio: u32,
    /// Extra time past `Loan.due_time` before a loan becomes markable as defaulted.
    pub default_grace_period: u64,
    /// Basis-points of the defaulting borrower's `contribution` slashed on default.
    pub default_penalty_bps: u32,
    pub editing_period: u64,
    pub voting_period: u64,
    pub treasury_threshold: u32,
    pub quorum_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanProposal {
    pub id: u32,
    pub borrower: Address,
    pub amount: i128,
    pub interest_rate: u32,
    pub duration: u64,
    pub total_repayment: i128,
    pub created_at: u64,
    pub editing_period_end: u64,
    pub phase: ProposalPhase,
    pub status: ProposalStatus,
    pub for_votes: i128,
    pub against_votes: i128,
    pub votes_cast: u32,
    pub voting_period: u64,
    pub metadata_cid: Option<String>,
    /// Timestamp of the last `edit_loan_proposal` call (`None` if never edited).
    pub last_edited_at: Option<u64>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Loan {
    pub id: u32,
    pub borrower: Address,
    pub principal: i128,
    pub interest_rate: u32,
    pub total_repayment: i128,
    pub start_time: u64,
    pub due_time: u64,
    pub status: LoanStatus,
    pub amount_repaid: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryProposal {
    pub id: u32,
    pub proposer: Address,
    pub amount: i128,
    pub destination: Address,
    pub reason: String,
    pub created_at: u64,
    pub status: ProposalStatus,
    pub for_votes: i128,
    pub against_votes: i128,
    pub votes_cast: u32,
    pub voting_period: u64,
    pub treasury_threshold: u32,
    /// When true, votes must be committed then revealed (commit-reveal privacy).
    pub private: bool,
}

/// Spendable treasury balance in one whitelisted asset (#339).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetBalance {
    pub asset: Address,
    pub balance: i128,
}

/// Headline DAO state returned by `get_dao_summary` (#339).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaoSummary {
    pub total_members: u32,
    pub active_members: u32,
    /// Spendable treasury in the base token; same value as `get_treasury_balance`.
    pub treasury_balance: i128,
    /// Spendable treasury in every whitelisted asset, base token first.
    pub treasury_balances: Vec<AssetBalance>,
    pub total_staked: i128,
    pub consensus_threshold: u32,
    pub paused: bool,
    pub version: String,
}

/// Computed loan terms returned by the read-only quote helper.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanTerms {
    pub interest_rate: u32,
    pub total_repayment: i128,
    pub duration: u64,
}

/// Structured event emitted when a borrower edits loan proposal terms.
/// Captures prior and updated terms plus the edit timestamp.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanTermsEdited {
    pub proposal_id: u32,
    pub borrower: Address,
    pub prev_amount: i128,
    pub prev_total_repayment: i128,
    pub new_amount: i128,
    pub total_repayment: i128,
    pub edited_at: u64,
}

/// Pending policy update undergoing timelock delay (#192).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingPolicyUpdate {
    pub policy: LoanPolicy,
    pub proposed_at: u64,
    pub execution_time: u64,
}

/// Structured event emitted upon staking reward claim (#193).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StakingRewardClaimed {
    pub member: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposalVote {
    pub proposal_id: u32,
    pub vote: bool,
}

// ── seed_state entry types (#332) ─────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeedStakeEntry {
    pub address: Address,
    pub amount: i128,
}

/// Pending yield for a member. `amount` is stored as the negative yield snapshot
/// so the existing `get_pending_yield` formula `(accumulator - snapshot).max(0)`
/// reconstructs the correct value from a fresh accumulator of 0 (#332).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeedYieldEntry {
    pub address: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeedNameEntry {
    pub name: String,
    pub owner: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeedDocEntry {
    pub kind: crate::storage::ProposalKind,
    pub proposal_id: u32,
    pub content_hash: Bytes,
}
