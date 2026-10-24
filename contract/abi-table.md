## Public interface (ABI)

All 78 entrypoints are on the `OurDao` contract (`lib.rs`). Errors are the numeric [`Error`](#error-codes) codes below.

**Lifecycle / governance**

| Method | Description |
|---|---|
| `initialize(admins, consensus_threshold, membership_fee, token, policy)` | One-time setup: bootstrap admins, consensus threshold (basis points), membership fee, token address, and loan policy. |
| `add_admin(caller, admin)` | Admin-only. Adds a new address to the admin set. |
| `remove_admin(caller, admin)` | Admin-only. Removes an admin; the last remaining admin can never be removed. |
| `set_pauser(caller, pauser)` | Admin-only. Designates an emergency pauser address. |
| `revoke_pauser(caller)` | Admin-only. Revokes the designated emergency pauser. |
| `set_consensus_threshold(caller, threshold)` | Admin-only. Updates the consensus threshold in basis points (1–10,000). |
| `propose_policy_update(caller, policy)` | Admin-only. Proposes a new loan policy subject to a 48-hour timelock delay. |
| `execute_policy_update(caller)` | Admin-only. Executes a pending policy update after the 48-hour timelock duration has elapsed. |
| `cancel_policy_update(caller)` | Admin-only. Cancels a pending policy update proposal before execution. |
| `set_loan_policy(caller, policy)` | Admin-only. Direct policy update (subject to timelock in production). |
| `pause(caller)` | Emergency pause. Blocks all mutating operations instantly. |
| `unpause(caller)` | Admin-only. Lifts the emergency pause. |

**Membership & delegation**

| Method | Description |
|---|---|
| `register_member(member)` | Pays the membership fee and registers as an active member in the DAO. |
| `exit_dao(member)` | Withdraws pro-rata treasury share, stake, and accrued yield. Blocked while member has an active loan. |
| `claim_rewards(member) -> i128` | Withdraws accrued loan-interest yield distributions in the base token. |
| `delegate_vote(delegator, delegatee)` | Delegates voting power (base vote + staking boost) to another member. |

**Lending**

| Method | Description |
|---|---|
| `request_loan(borrower, amount, metadata_cid) -> u32` | Opens a new loan proposal in the base token (3-day editing window). |
| `request_loan_in_asset(borrower, amount, asset, metadata_cid) -> u32` | Opens a loan proposal denominated in a whitelisted non-base asset. |
| `edit_loan_proposal(borrower, proposal_id, new_amount)` | Borrower-only. Amends loan amount during the editing window. |
| `cancel_loan_proposal(borrower, proposal_id)` | Borrower-only. Cancels loan proposal during the editing window. |
| `vote_on_loan_proposal(voter, proposal_id, support)` | Casts a vote on an active loan proposal; auto-approves and disburses when threshold is met. |
| `vote_batch(voter, votes)` | Atomic batch voting across multiple loan proposals in a single transaction. |
| `disburse_approved_loan(proposal_id)` | Permissionless keeper call. Disburses principal for an approved loan waiting on treasury liquidity. |
| `repay_loan(borrower, loan_id)` | Collects the full outstanding loan balance in the base token. |
| `repay_loan_in_asset(borrower, loan_id, asset)` | Repays a loan in its designated whitelisted asset. |
| `repay_loan_partial(borrower, loan_id, amount)` | Applies a partial repayment: covers accrued interest first, then reduces principal. |
| `mark_loan_defaulted(loan_id)` | Permissionless keeper call. Marks overdue loan defaulted after due time plus grace period, slashing contribution. |
| `expire_loan_proposal(proposal_id)` | Permissionless keeper call. Persists expired state for loan proposal that failed to reach quorum. |

**Treasury**

| Method | Description |
|---|---|
| `propose_treasury_withdrawal(proposer, amount, destination, reason, private) -> u32` | Proposes treasury withdrawal in base token; `private = true` routes through commit-reveal. |
| `propose_withdrawal_in_asset(proposer, amount, asset, destination, reason, private) -> u32` | Proposes treasury withdrawal in a whitelisted non-base asset. |
| `vote_on_treasury_proposal(voter, proposal_id, support)` | Casts open vote on treasury proposal; auto-executes when threshold is met. |
| `cancel_treasury_proposal(proposer, proposal_id)` | Proposer-only. Cancels proposal while pending within the editing period. |
| `expire_treasury_proposal(proposal_id)` | Permissionless keeper call. Persists expired status for treasury proposal past voting deadline. |
| `execute_treasury_proposal(proposal_id)` | Permissionless keeper call. Executes approved withdrawal that was waiting on treasury liquidity. |

**Multi-token whitelist**

| Method | Description |
|---|---|
| `propose_token_addition(caller, token)` | Admin-only. Proposes adding an extra asset to the whitelist behind a 48-hour timelock. |
| `execute_token_addition(caller, token)` | Admin-only. Executes whitelisting of proposed token once timelock expires. |
| `remove_token(caller, token)` | Admin-only. Removes token from whitelist; existing loans remain repayable but new loans/withdrawals blocked. |
| `is_token_whitelisted(token) -> bool` | View. Returns whether `token` is currently whitelisted. |
| `get_whitelisted_tokens() -> Vec<Address>` | View. Returns array of all extra whitelisted treasury assets. |

**Staking & voting power**

| Method | Description |
|---|---|
| `stake(member, amount)` | Stakes DAO tokens to boost voting weight quadratically (`1 + min(isqrt(stake / 100), 5)`). |
| `unstake(member, amount)` | Unstakes and withdraws staked tokens (blocked while proposals are in editing/voting). |
| `get_stake(member) -> i128` | View. Returns a member's currently staked balance. |
| `get_voting_weight(member) -> i128` | View. Returns a member's total voting weight including staking boost and delegations. |
| `get_stake_weight_unit() -> i128` | View. Returns constant tokens per unit step for quadratic staking math. |
| `get_max_stake_bonus() -> i128` | View. Returns maximum bonus votes achievable from staking (5). |

**Name registry**

| Method | Description |
|---|---|
| `register_name(owner, name)` | Registers a 1:1 human-readable handle (3–32 chars); automatically releases previous name if held. |
| `resolve_name(name) -> Option<Address>` | View. Resolves a registered handle to its owner address. |
| `name_of(owner) -> Option<String>` | View. Reverse lookup: returns registered name for an address. |

**Commit-reveal voting**

| Method | Description |
|---|---|
| `commit_treasury_vote(voter, proposal_id, commitment)` | Submits blinded commitment `sha256(support ++ salt)` during private proposal voting phase. |
| `reveal_treasury_vote(voter, proposal_id, support, salt)` | Reveals vote choice and salt during reveal window; verified commitment tallies into proposal. |

**Content-hash proposal documents**

| Method | Description |
|---|---|
| `attach_document(caller, kind, proposal_id, content_hash)` | Proposer-only. Anchors an off-chain content hash (IPFS CID, max 64 bytes) during editing phase. |
| `get_document(kind, proposal_id) -> Option<Bytes>` | View. Returns document content hash attached to a proposal. |

**Views & state queries**

| Method | Description |
|---|---|
| `get_member(address) -> Option<Member>` | View. Returns member record (join time, status, contribution, loan counts). |
| `get_member_loan_stats(member) -> MemberLoanStats` | View. Returns member's loan history (active loans, repaid loans, total loans). |
| `get_loan(loan_id) -> Option<Loan>` | View. Returns active/repaid/defaulted loan record. |
| `get_loan_proposal(proposal_id) -> Option<LoanProposal>` | View. Returns loan proposal (auto-refreshes expired phase based on ledger timestamp). |
| `get_treasury_proposal(proposal_id) -> Option<TreasuryProposal>` | View. Returns treasury proposal record. |
| `get_loan_proposal_count() -> u32` | View. Returns total lifetime loan proposals created. |
| `get_treasury_proposal_count() -> u32` | View. Returns total lifetime treasury proposals created. |
| `get_loan_policy() -> LoanPolicy` | View. Returns active loan policy parameters. |
| `get_pending_policy_update() -> Option<PendingPolicyUpdate>` | View. Returns pending policy update proposal and execution timestamp if queued. |
| `get_admins() -> Vec<Address>` | View. Returns list of all current contract admin addresses. |
| `is_admin(address) -> bool` | View. Returns whether address is an admin. |
| `is_member(address) -> bool` | View. Returns whether address is a member. |
| `is_eligible_for_loan(member)` | View. Returns `Ok(())` if member meets cooldown, membership duration, and active loan criteria. |
| `get_treasury_balance() -> i128` | View. Returns total unencumbered base token treasury balance. |
| `get_treasury_balance_in_asset(asset) -> i128` | View. Returns treasury balance held in a specified whitelisted asset. |
| `get_total_members() -> u32` | View. Returns lifetime total unique registered members count. |
| `get_active_members() -> u32` | View. Returns current active members count. |
| `get_consensus_threshold() -> u32` | View. Returns consensus threshold in basis points. |
| `get_token() -> Address` | View. Returns base governance token contract address. |
| `is_paused() -> bool` | View. Returns whether contract is currently paused. |
| `get_version() -> String` | View. Returns contract package semantic version string. |
| `get_pending_yield(member) -> i128` | View. Returns claimable yield accrued to member from loan repayments. |
| `has_voted(kind, proposal_id, voter) -> bool` | View. Returns whether voter has already cast or revealed a vote on a proposal. |
| `calculate_loan_terms(amount) -> LoanTerms` | View. Quotes interest rate and total repayment for a given loan principal. |
| `calculate_exit_share(member) -> i128` | View. Quotes pro-rata treasury withdrawal share for a member. |

**Maintenance**

| Method | Description |
|---|---|
| `bump_dao_ttl()` | Permissionless keeper call. Extends storage TTL for contract instance and code. |

