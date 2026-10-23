use soroban_sdk::contracterror;

/// Every failure mode the DAO can return. Numeric codes are stable and part of
/// the contract's public ABI, so append new variants rather than renumbering.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    // ---- lifecycle / config ----
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidThreshold = 3,
    InvalidAmount = 4,
    InvalidLoanPolicy = 5,
    Paused = 6,
    NotPaused = 7,

    // ---- authorization ----
    NotAuthorized = 10,
    NotAdmin = 11,
    NotMember = 12,
    AlreadyAdmin = 13,
    AlreadyMember = 14,
    CannotRemoveLastAdmin = 15,

    // ---- membership ----
    MemberNotActive = 20,
    HasActiveLoan = 21,

    // ---- loans ----
    ProposalNotFound = 30,
    NotBorrower = 31,
    NotInEditingPhase = 32,
    NotInVotingPhase = 33,
    VotingEnded = 34,
    AlreadyVoted = 35,
    NotEligibleForLoan = 36,
    CooldownActive = 37,
    LoanNotFound = 38,
    LoanNotActive = 39,
    ExceedsTreasuryRatio = 40,
    InsufficientTreasury = 41,
    LoanNotOverdue = 42,

    // ---- treasury ----
    TreasuryProposalNotFound = 50,

    // ---- native-swap modules ----
    NameTaken = 60,
    /// Reserved for future name registry use
    NameNotFound = 61,
    NoStake = 62,
    InsufficientStake = 63,
    NoCommitment = 64,
    CommitmentMismatch = 65,
    AlreadyRevealed = 66,
    NothingToClaim = 67,

    // ---- appended (keep at end) ----
    ProposalNotExpired = 70,
    InvalidName = 71,
    NotYetRevealed = 72,
    /// Caller is not the proposal's proposer/borrower and may not modify its
    /// attached document (#21).
    NotProposalOwner = 73,
    DocumentTooLarge = 74,
    /// The token passed to `initialize` is not a contract implementing the
    /// token interface (#115).
    InvalidToken = 75,
    /// Timelock delay has not expired yet (#192).
    TimelockNotExpired = 76,
    /// No policy update is currently pending (#192).
    NoPendingPolicy = 77,
    /// Proposal metadata CID is invalid (#194).
    InvalidMetadataCid = 78,
    /// Invalid delegation target (e.g. self-delegation) (#188).
    InvalidDelegation = 79,
    InvalidProposal = 80,
    AlreadyPauser = 81,
    NotPauser = 82,
    /// The asset is not on the treasury whitelist (#236).
    TokenNotWhitelisted = 83,
    /// The asset is already on the treasury whitelist (#236).
    TokenAlreadyWhitelisted = 84,
    /// No addition is pending for this asset (#236).
    NoPendingTokenAddition = 85,
    /// The asset does not match the one the loan was issued in (#236).
    AssetMismatch = 86,
    /// The base token set at initialization can't be removed (#236).
    CannotRemoveBaseToken = 87,
    /// Seeding is locked; `lock_seeding` has already been called (#332).
    SeedingLocked = 88,
    /// Seeding is already locked; calling `lock_seeding` again is a no-op error (#333).
    SeedingAlreadyLocked = 89,
    /// Member already has the maximum number of simultaneously open proposals.
    TooManyOpenProposals = 90,
    /// A borrower cannot vote on their own loan proposal (#327).
    BorrowerCannotVote = 91,
    /// Cannot remove a token while active loans exist in that asset (#273).
    TokenHasActiveLoans = 92,
}
