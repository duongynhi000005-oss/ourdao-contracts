use soroban_sdk::{symbol_short, Symbol};

// ==================== Admin & Config Events ====================
pub const INIT: Symbol = symbol_short!("init");
pub const ADMIN_ADD: Symbol = symbol_short!("admin_add");
pub const ADMIN_REM: Symbol = symbol_short!("admin_rem");
pub const PSET: Symbol = symbol_short!("pset");
pub const PREV: Symbol = symbol_short!("prev");
pub const THRESHOLD: Symbol = symbol_short!("threshold");
pub const POL_PROP: Symbol = symbol_short!("pol_prop");
pub const POLICY: Symbol = symbol_short!("policy");
pub const POL_CANC: Symbol = symbol_short!("pol_canc");
pub const PAUSED: Symbol = symbol_short!("paused");
pub const UNPAUSED: Symbol = symbol_short!("unpaused");

// ==================== Token Whitelist Events ====================
pub const TOK_PROP: Symbol = symbol_short!("tok_prop");
pub const TOK_ADD: Symbol = symbol_short!("tok_add");
pub const TOK_REM: Symbol = symbol_short!("tok_rem");

// ==================== Membership & Delegation Events ====================
pub const JOINED: Symbol = symbol_short!("joined");
pub const EXITED: Symbol = symbol_short!("exited");
pub const CLAIMED: Symbol = symbol_short!("claimed");
pub const DELEGATE: Symbol = symbol_short!("delegate");

// ==================== Staking Events ====================
pub const STAKED: Symbol = symbol_short!("staked");
pub const UNSTAKED: Symbol = symbol_short!("unstaked");

// ==================== Loan Events ====================
pub const LOAN_ASST: Symbol = symbol_short!("loan_asst");
pub const LOAN_REQ: Symbol = symbol_short!("loan_req");
pub const LOAN_EDIT: Symbol = symbol_short!("loan_edit");
pub const LOAN_VOTE: Symbol = symbol_short!("loan_vote");
pub const LOAN_WAIT: Symbol = symbol_short!("loan_wait");
pub const LOAN_REJ: Symbol = symbol_short!("loan_rej");
pub const LOAN_APPR: Symbol = symbol_short!("loan_appr");
pub const LOAN_RPY: Symbol = symbol_short!("loan_rpy");
pub const LOAN_EXP: Symbol = symbol_short!("loan_exp");
pub const LOAN_DFLT: Symbol = symbol_short!("loan_dflt");
pub const INTEREST: Symbol = symbol_short!("interest");

// ==================== Treasury Events ====================
pub const TRE_ASSET: Symbol = symbol_short!("tre_asset");
pub const TRE_PROP: Symbol = symbol_short!("tre_prop");
pub const TRE_VOTE: Symbol = symbol_short!("tre_vote");
pub const TRE_WAIT: Symbol = symbol_short!("tre_wait");
pub const TRE_REJ: Symbol = symbol_short!("tre_rej");
pub const TRE_EXEC: Symbol = symbol_short!("tre_exec");

// ==================== Shared Proposal Events ====================
pub const PROP_CANC: Symbol = symbol_short!("prop_canc");

// ==================== Privacy Events ====================
pub const COMMITTED: Symbol = symbol_short!("committed");
pub const REVEALED: Symbol = symbol_short!("revealed");

// ==================== Registry Events ====================
pub const NAME_REG: Symbol = symbol_short!("name_reg");
pub const NAME_REL: Symbol = symbol_short!("name_rel");

// ==================== Documentation Events ====================
pub const DOC_ATTN: Symbol = symbol_short!("doc_attn");

// ==================== Maintenance Events ====================
pub const TTL_BUMPED: Symbol = symbol_short!("TtlBumped");
