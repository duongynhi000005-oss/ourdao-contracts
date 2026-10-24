# Migrating off a deployed OurDAO

OurDAO is deliberately immutable once deployed (see [Known limitations](../README.md#known-limitations)) — there is no upgrade path, and there never will be one. That's a trust-minimization choice, not an oversight. But it leaves one real question unanswered until now: **if a bug is found in a deployed OurDAO, what exactly do the members do?**

This document is that procedure. It is a design and process document — it changes no contract code. The one piece of contract surface it assumes, a `seed_state` entrypoint on the *new* contract, is specified precisely enough to implement but is **not implemented here**; that's deliberately a separate, follow-up issue, same as any other new entrypoint.

There is no way around the core trust tradeoff this creates: **whoever runs a migration is trusted to seed the new contract truthfully.** Nothing on-chain proves the seeded state matches the old contract's real history. What makes that checkable after the fact is (a) `ourdao-backend`'s append-only event log, which every member can independently replay, and (b) the old contract's own views, which stay live and publicly readable for the entire migration window. A migration is legitimate to the extent members can — and did — check it against those two things themselves, not because the procedure was followed.

## Table of contents

- [State inventory](#state-inventory)
- [Procedure](#procedure)
- [Edge cases: mid-migration loans and proposals](#edge-cases-mid-migration-loans-and-proposals)
- [TTL / state-archival risk](#ttl--state-archival-risk)
- [The `seed_state` entrypoint (design only)](#the-seed_state-entrypoint-design-only)
- [Trust assumption](#trust-assumption)

## State inventory

Every category of state a migration must carry, and where it comes from. "View" means read live from the old contract before it's decommissioned. "Event" means reconstructible from `ourdao-backend`'s indexed event log even after the old contract is gone. Where a category needs both — a view to get the *current* value, an event stream to get the *set of addresses/ids to query* — both are listed.

Following feature additions (multi-token whitelist #236, emergency pauser #184, voting delegation #188, member loan stats #305, and O(1) active contributions #353), every `DataKey` variant in `storage.rs` is cataloged below across its storage type and value schema:

| State Item / DataKey | Storage Type | Schema / Value Type | Read Source | Migration Notes & Flags |
|---|---|---|---|---|
| **Admin set** (`DataKey::Admins`) | Instance | `Vec<Address>` | View: `get_admins()` | Fully enumerable directly — no event replay needed. |
| **Consensus threshold** (`DataKey::Threshold`) | Instance | `u32` | View: `get_consensus_threshold()` | Quorum threshold basis points (e.g. 5100 = 51%). |
| **Loan policy** (`DataKey::Policy`) | Instance | `LoanPolicy` struct | View: `get_loan_policy()` | Active rates, durations, and treasury ratio bounds. |
| **Base token** (`DataKey::Token`) | Instance | `Address` | View: `get_token()` | Primary DAO asset used for membership and staking. |
| **Membership fee** (`DataKey::MembershipFee`) | Instance | `i128` | View: `get_membership_fee()` / Init config | Joining fee denominated strictly in base token. |
| **Emergency pauser** (`DataKey::Pauser`) | Instance | `Address` | Event: `pauser_set` / `pauser_rev` | ⚠️ **FLAG: No contract view currently exposes active pauser address.** Reconstruct from event replay or re-assign post-migration. |
| **Pause state** (`DataKey::Paused`) | Instance | `bool` | View: `is_paused()` | Permanent `pause()` on old contract; new deployment starts unpaused. |
| **Pending policy update** (`DataKey::PendingPolicyUpdate`) | Instance | `(LoanPolicy, u64)` | Event: `policy_prop` / `policy_exec` | ⚠️ **FLAG: No view exposes pending policy draft & execution time.** Recommend executing or cancelling before cutoff. |
| **Member address set** (`DataKey::Members`) | Instance | `Vec<Address>` | View: `get_members()` / Events: `joined` | Full list of unique member addresses ever joined. |
| **Member counters** (`TotalMembers`, `ActiveMembers`) | Instance | `u32` | View: `get_total_members()` / `get_active_members()` | Cross-checks total and active member records seeded. |
| **Member record** (`DataKey::Member(Address)`) | Persistent | `Member` struct | View: `get_member(address)`, per address | Carries `status`, `join_ledger`, `contribution`, `share_balance`, `has_active_loan`, `last_loan_time`. |
| **Total contributions** (`DataKey::TotalContributions`) | Persistent | `i128` | Storage counter | Cumulative member equity counter. |
| **Active contributions** (`DataKey::ActiveContributions`) | Persistent | `i128` | Storage counter (#353) | Running sum of `contribution` across all `ActiveMember`s. Reconstructible by summing active members' contributions. |
| **Vote delegation** (`DataKey::Delegation(Address)`) | Persistent | `Address` | Event: `delegated` | ⚠️ **FLAG: No contract view currently exposes `get_delegate(member)` (#409).** Must be reconstructed by replaying delegation events. |
| **Member stakes** (`DataKey::Stake(Address)`) | Persistent | `i128` | View: `get_stake(address)`, per address | Staked collateral in base token; eligible for quadratic voting boost. |
| **Stake timestamps** (`DataKey::StakeTime(Address)`) | Persistent | `u64` | Storage | Timestamp of last stake operation per member. |
| **Total staked** (`DataKey::TotalStaked`) | Instance | `i128` | View: `get_total_staked()` | Segregated collateral total; excluded from lendable treasury. |
| **Pending yield** (`DataKey::PendingYield(Address)`) | Persistent | `i128` | View: `get_pending_yield(address)`, per address — **must be read live** | **Reconstructible from `exit_dao` events** — emits `(member, share, pending)`. Seed via pull-accumulator snapshot (`MemberYieldSnapshot = -X`). |
| **Yield accumulator & remainder** (`YieldAccumulator`, `YieldRemainder`) | Instance / Persistent | `i128` | Internal counters | Pull accumulator resets to 0 on new contract; fractional remainder carries into new treasury balance. |
| **Member yield snapshot** (`DataKey::MemberYieldSnapshot(Address)`) | Persistent | `i128` | Storage | Pull-accumulator baseline. Seeded as `-pending_yield` on new contract. |
| **Token whitelist** (`DataKey::WhitelistedTokens`) | Persistent | `Vec<Address>` | View: `get_whitelisted_tokens()` | Ordered list of all secondary whitelisted assets (#236). |
| **Token whitelist status** (`DataKey::TokenWhitelisted(Address)`) | Persistent | `bool` | View: `is_token_whitelisted(token)` | 1:1 lookup for token whitelist membership. |
| **Pending token addition** (`DataKey::PendingTokenAddition(Address)`) | Persistent | `u64` | Event: `tok_prop` | ⚠️ **FLAG: No view exposes pending token addition timestamp.** Recommend waiting out 48h timelock before cutoff. |
| **Per-asset treasury balances** (`DataKey::TreasuryBalance(Address)`) | Persistent | `i128` | View: `get_treasury_balance_in_asset(asset)` | Available uncommitted treasury reserves per whitelisted asset. |
| **Active loans** (`DataKey::Loan(u32)`) | Persistent | `Loan` struct | View: `get_loan(loan_id)`, per id | Carries `due_time`, `amount`, `outstanding_balance`, `asset`. Settle before migration if possible. |
| **Loan denomination asset** (`DataKey::LoanAsset(u32)`) | Persistent | `Address` | View: `get_loan(id)` / Event: `loan_asst` | Denomination currency of loan (absent = base token). |
| **Member loan stats** (`DataKey::MemberLoanStats(Address)`) | Persistent | `MemberLoanStats` struct | View: `get_member_loan_stats(address)` (#305) / Events | Historical borrower track record (`total_borrowed`, `total_repaid`, `default_count`). |
| **In-flight loan proposals** (`DataKey::LoanProposal(u32)`) | Persistent | `LoanProposal` struct | View: `get_loan_proposal(id)`, per id | Ephemeral; recommend requiring resubmission on new contract. |
| **In-flight treasury proposals** (`DataKey::TreasuryProposal(u32)`) | Persistent | `TreasuryProposal` struct | View: `get_treasury_proposal(id)`, per id | Ephemeral; recommend resubmission after migration. |
| **Treasury proposal asset** (`DataKey::TreasuryAsset(u32)`) | Persistent | `Address` | View: `get_treasury_proposal(id)` / Event: `tre_asset` | Payout currency of withdrawal (absent = base token). |
| **Vote records** (`LoanVoted`, `TreasuryVoted`) | Persistent | `bool` | View: `has_voted(kind, id, voter)` | Per-proposal voter records; dropped if proposals are resubmitted. |
| **Private vote commitments** (`DataKey::Commit(u32, Address)`) | Persistent | `BytesN<32>` | Event: `committed` | Unrevealed commit-reveal votes; must be revealed before migration cutoff. |
| **Name registry** (`Name(String)`, `NameOf(Address)`) | Persistent | `Address` / `String` | View: `resolve_name(name)` / `name_of(owner)` | Bi-directional 1:1 handle registry mapping. |
| **Document hashes** (`DataKey::Doc(ProposalKind, u32)`) | Persistent | `Bytes` | View: `get_document(kind, id)` | Attached IPFS CIDs or sha256 content hashes. |
| **ID sequence counters** (`NextProposalId`, `NextTreasuryId`) | Instance | `u32` | Internal counters | Seed on new contract as `max(existing_ids) + 1` to prevent ID collisions. |

One structural note that falls out of this table: **the member-address set, extra token additions, and every loan/proposal/name/doc id must come from the event log or enumeration views**, because contract storage keys without views cannot be scanned on-chain. If `ourdao-backend` has not been running continuously since deployment, this procedure cannot be completed — see the RPC caveat below.

> Public Soroban RPC retains only ~24h of ledger history. "Just replay the event log" is only possible because `ourdao-backend` has (assumed) been indexing continuously since deployment into its own durable store — RPC itself is not that store.

## Procedure

1. **Freeze the address/id sets.** Query `ourdao-backend` for every member address, loan id, proposal id, name, and `(kind, id)` doc pair as of a chosen cutoff ledger. This defines exactly what the live-view reads in the next steps need to cover.
2. **`pause()` the old contract.** This is an admin call and the first on-chain action of the migration. Know precisely what it does and doesn't stop (see the table below) — it is not a full freeze.
3. **Read every category in the [state inventory](#state-inventory) live**, for every address/id from step 1. Do this as soon as possible after pausing — see [TTL risk](#ttl--state-archival-risk) for why waiting is dangerous.
4. **Settle active loans.** Pausing does *not* block `repay_loan`, `repay_loan_partial`, or the permissionless `mark_loan_defaulted`. Give borrowers a defined settlement window (e.g. matching the existing `default_grace_period`) to repay in full or in part; any loan still active and overdue at the end of the window gets called via `mark_loan_defaulted` before state is re-read for the final time. This is the recommended path over carrying a still-active loan's `due_time`/`status` across into the new deployment, which would otherwise force the new contract to reproduce default-timing continuity for state it didn't originate. See [edge cases](#edge-cases-mid-migration-loans-and-proposals) for the fallback if a clean settlement window isn't acceptable.
5. **Resolve in-flight proposals.** Recommended: let them lapse (loan proposals auto-expire via `expire_loan_proposal`; treasury proposals auto-expire via `expire_treasury_proposal` once `VOTING_PERIOD` has elapsed — call it on each outstanding proposal id after the voting period passes). Do not carry partial vote tallies into the new contract. See [edge cases](#edge-cases-mid-migration-loans-and-proposals).
6. **Deploy the new contract** and `initialize()` it normally (admins, threshold, membership fee, token, policy — from the values read in step 3).
7. **Seed the new contract's state** via the (not-yet-implemented, see below) `seed_state` entrypoint(s), from the data captured in step 3.
8. **Independent verification window.** Before normal operation resumes, publish the full seeded dataset (or its hashes) so members can diff it against their own replay of the event log and their own last-known balances. This is the only real check on the trust assumption below — don't skip it or rush it.
9. **Lock seeding** on the new contract (see [below](#the-seed_state-entrypoint-design-only)) and resume normal operation. Publicly retire the old contract's frontend/indexing so members aren't misled into interacting with a decommissioned deployment; the old contract itself cannot be "shut off" beyond `pause()`, so its `pause()` state stays permanent.

Step 2's `pause()` blocks: `register_member`, `propose_treasury_withdrawal`, `vote_on_treasury_proposal`, `commit_treasury_vote`, `reveal_treasury_vote`, `request_loan`, `vote_on_loan_proposal`. It does **not** block: `repay_loan`, `repay_loan_partial`, `exit_dao`, `claim_rewards`, `mark_loan_defaulted`, `expire_loan_proposal`, `stake`, `unstake`, `register_name`, `attach_document`. Steps 3 onward have to account for state legitimately continuing to change in the unblocked paths until the live reads in step 3 are taken as final.

## Edge cases: mid-migration loans and proposals

**A member with an active loan mid-migration.** The recommended path (step 4 above) settles every loan — via repayment or default — before the final state read, so no active loan ever needs to be represented in the seed data at all. If a project chooses not to impose a settlement window (e.g. the DAO wants migration to complete faster than borrowers can reasonably repay), the fallback is to carry the `Loan` struct across verbatim, including `due_time`. That due date was set against the old contract's ledger timestamps, which are directly portable (Soroban ledger time is a real Unix timestamp, not contract-relative), so `mark_loan_defaulted`'s existing time comparison keeps working unmodified on the new contract post-seed. The cost is that the new contract's very first blocks of activity include a borrower who never interacted with it, which is unusual but not unsound.

**A proposal mid-vote.** Recommended: require resubmission after migration rather than carrying over partial tallies. Carrying a proposal over would mean seeding not just the proposal but every individual vote record (`LoanVoted`/`TreasuryVoted` per voter) to prevent double-voting on the new contract, which meaningfully complicates the seed format for a comparatively low-value class of state — proposals are ephemeral by design, and members who already discussed and voted for something will not find re-proposing it costly. Note this in whatever member communication accompanies the migration, since it does mean real, if minor, lost momentum on anything mid-vote at cutoff.

## TTL / state-archival risk

Every persistent storage entry (members, loans, proposals, votes, stakes, yield snapshots, docs, name records) extends its TTL on every read *or* write (90-day bump — see `storage.rs`). An entry nobody has touched in 90 days is eligible for state archival: it becomes unreadable through the normal `get_*` views without an explicit restore operation first, and depending on how long it's been archived, restoration may not be possible at all.

This matters specifically for step 3 above: if the migration is triggered by a bug discovery rather than routine housekeeping, some members' entries — particularly low-activity members who haven't interacted with the contract in a while — may already be close to or past their TTL by the time the migration actually runs. Concretely:

- **Read every category as early as possible**, ideally as the very next action after `pause()`, not after the settlement window in step 4. A member whose entry expires between pause and the final read is a member whose pending yield (in particular — see the inventory table) becomes genuinely unrecoverable.
- If any entries are already archived when the migration starts, they need an explicit restore (`stellar contract restore` or equivalent) before they're readable again — budget for this as a real possibility, not an edge case, for any DAO that's been live for several months with low-activity members.
- Instance-storage singletons (admin set, threshold, policy, token, the yield accumulator itself) bump on a shorter 30-day cycle but are touched by nearly every call, so in practice they're at far lower risk than a specific inactive member's persistent entries.

## The `seed_state` entrypoint (design only)

Not implemented here — this is a specification for a follow-up issue, on the **new** contract only (the old, already-deployed contract's ABI cannot change).

**Shape.** Chunked rather than a single call: a live DAO's member/loan/proposal counts can exceed what fits in one transaction's resource limits, so seeding needs to be splittable across multiple calls without any ordering requirement between chunks (each entry keyed by its own address/id, so a partial batch is safe to retry or reorder):

```
fn seed_members(env: Env, caller: Address, members: Vec<Member>) -> Result<(), Error>
fn seed_stakes(env: Env, caller: Address, stakes: Vec<(Address, i128)>) -> Result<(), Error>
fn seed_pending_yield(env: Env, caller: Address, yields: Vec<(Address, i128)>) -> Result<(), Error>
fn seed_loans(env: Env, caller: Address, loans: Vec<Loan>) -> Result<(), Error>
fn seed_names(env: Env, caller: Address, names: Vec<(String, Address)>) -> Result<(), Error>
fn seed_docs(env: Env, caller: Address, docs: Vec<(ProposalKind, u32, Bytes)>) -> Result<(), Error>
fn seed_whitelisted_tokens(env: Env, caller: Address, tokens: Vec<Address>) -> Result<(), Error>
fn seed_delegations(env: Env, caller: Address, delegations: Vec<(Address, Address)>) -> Result<(), Error>
fn seed_member_loan_stats(env: Env, caller: Address, stats: Vec<(Address, MemberLoanStats)>) -> Result<(), Error>
fn lock_seeding(env: Env, caller: Address) -> Result<(), Error>
```

**Pending yield seeding, specifically.** The pull-based accumulator (`YieldAccumulator` + per-member `MemberYieldSnapshot`, see `loans::distribute_interest`) makes "set this member's pending yield to X" not a direct field write. The clean way to seed it without touching `claim_rewards`/`distribute_interest` at all: leave the new contract's `YieldAccumulator` at its natural starting value (0), and for a member with pending yield `X`, set their `MemberYieldSnapshot` to `-X`. `get_pending_yield` computes `(accumulator - snapshot).max(0)`, so `(0 - (-X)).max(0) == X` — the existing formula reconstructs the seeded value with no special-casing.

**Authorization model.** Admin-only (`require_admin`, same as every other privileged entrypoint), and one-shot in aggregate: every seed function checks a `MigrationSeedingLocked` instance-storage flag and rejects if set, and `lock_seeding` is the only way to set that flag — irreversibly, no `unlock`. Call `lock_seeding` only once independent verification (procedure step 8) is done. Before that lock, seeding is idempotent per key (re-seeding the same address/id overwrites, so a bad chunk can be corrected by re-sending it) but is otherwise unrestricted — this is exactly the trust-concentration point the next section is about.

**Why not fold this into `initialize`?** Splitting it out means the normal `initialize` path (a fresh, non-migrated DAO) doesn't carry any migration-only surface area, and it lets seeding happen in as many transactions as the real member count requires instead of being bounded by what a single `initialize` call could hold.

## Trust assumption

Say this plainly, because burying it is how it stops being checked: **admins seeding a migrated contract are trusted to seed it truthfully.** The contract has no way to verify that a seeded `Member` record, loan, or stake actually matches what the old contract held — `seed_state` accepts whatever the caller passes. Nothing about the design in this document changes that; it can't, because verifying arbitrary historical state on-chain against a contract that's being decommissioned isn't a problem Soroban gives us primitives for.

What makes a migration accountable, then, isn't the contract — it's procedure step 8. The seeded dataset (or a hash of it, with the raw data published off-chain) has to be checkable by any member against `ourdao-backend`'s event log and against their own last interaction with the old contract, *before* `lock_seeding` closes the window. A migration nobody checked is a migration that was trusted on faith, regardless of how carefully this document was followed.
