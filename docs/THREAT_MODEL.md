# Threat Model and Trust Assumptions

Status: testnet-stage, **not externally audited**. This document states what the contract assumes and what it does not defend against. It complements [`SECURITY.md`](../SECURITY.md) (reporting) and [`docs/INVARIANTS.md`](./INVARIANTS.md) (what must always hold).

## Assets

- Treasury funds held by the contract in the base DAO token as well as any whitelisted secondary assets (#236).
- Member stakes (denominated in the base token) and accrued yield.
- Governance and accounting state: proposals, votes, loans, membership records, token whitelist registry, and emergency pauser role.

## Actors and trust

| Actor | Trusted to | Not trusted to |
| --- | --- | --- |
| Admin set | Perform admin-only entrypoints (policy updates, pause/unpause, pauser role assignment/revocation, token whitelist additions/removals, migration seeding) and stay online | Move member funds outside the entrypoints' rules |
| Emergency Pauser | Promptly invoke `pause()` during an ongoing incident or exploit without requiring multi-party admin quorum (#184) | Call `unpause()` (restricted strictly to admins, #319), modify policies, set consensus thresholds, whitelist tokens, or alter treasury funds |
| Members | Vote, borrow, stake, and act only for their own address (`require_auth` per call) | Act on another address's behalf |
| Anyone | Call permissionless keeper entrypoints such as `mark_loan_defaulted` and `expire_loan_proposal`, which are pure functions of ledger time and proposal/loan state | Bypass state machine rules or manipulate fund disbursements |
| Whitelisted token contracts | Behave as standard Soroban tokens implementing the expected token interface | Deduct hidden transfer fees (fee-on-transfer), execute arbitrary external hooks, or falsely report balance deltas |
| Off-chain backend and frontend | Index and display state | They are not a source of truth; the on-chain contract state is |

## Trust assumptions

1. **Admins are honest and available.** Admin keys can pause/unpause the contract, assign or revoke the pauser role, and propose policy or whitelist updates. A compromised admin quorum is the largest single risk. Consensus thresholds and initial admin quorums are established at initialization.
2. **The pauser role is segregated.** The emergency pauser role (#184) is designed for high-availability operational keys (e.g. automated security monitoring bots or on-call responders). While a compromised pauser can cause temporary denial of service, they cannot unpause the contract (#319), siphon assets, or tamper with governance state.
3. **Whitelisted tokens are well behaved.** Balance-changing operations follow checks-effects-interactions, but a whitelisted contract that lies about balances or transfers compromises accounting. Token contracts must conform to Soroban token standards.
4. **Token whitelist governance follows timelocks.** Adding a secondary asset requires a two-step proposal and execution separated by a 48-hour timelock (`TIMELOCK_DURATION`), allowing stakeholders to inspect the proposed token contract before it becomes active (#192, #236).
5. **Migration seeding is trusted.** A migrated deployment relies on admins seeding state truthfully; see [`docs/MIGRATION.md`](./MIGRATION.md#trust-assumption).
6. **The contract is immutable.** There is no upgrade proxy or migration backdoor; resolving critical logic defects requires migrating state to a fresh deployment.
7. **Ledger time is the only clock.** Voting periods, timelocks, grace periods, expiries, and default calculations rely on Soroban ledger timestamps.

## Threats considered

| Threat | Impact | Mitigation / Status |
| --- | --- | --- |
| Acting on another member's behalf | Unauthorized loans, votes, or stake withdrawal | `require_auth()` enforced on the specific member/caller address in every state-changing entrypoint. |
| Double or miscounted votes | Governance hijacking or false quorum | Per-member vote tracking (`has_voted`), proposal phase guards, and voting-period expiry. |
| Private proposal stalling or reveal manipulation | Deadlocked proposals or selective vote withholding | Commit-reveal phase guards with dedicated reveal deadlines; see open issues #47, #48. |
| Reentrancy | State corruption during token transfers | Soroban runtime does not support reentrant mid-call external executions; state is updated before/alongside external token transfers (checks-effects-interactions). |
| Rounding drift in yield | Fractional value accumulation or leakage | `YieldRemainder` carried forward across distributions, bounded by property tests. |
| Unbounded storage / cost growth | Denial of service via ledger footprint limits | Document content-hash size limits (64 bytes), documented membership ceilings, and resource cost benchmarks (#138, #357). |
| **Compromised or rogue pauser** | Malicious pauser repeatedly pauses the DAO to cause Denial of Service (DoS) | **Mitigated (#319, #184):** Pausers cannot `unpause()`—unpausing is strictly admin-only. Admins can revoke a rogue pauser at any time via `revoke_pauser()`. Borrowers can continue repaying active loans during pause (`repay_loan`), preventing forced defaults. |
| **Malicious or fee-on-transfer token whitelisting** | Token transfers deduct unexpected fees, creating accounting discrepancies between internal records and real balances | **Mitigated & Ongoing (#275, #283):** Token additions require a 48h timelock (`TIMELOCK_DURATION`) for auditability. Contract verification prevents whitelisting unverified targets (#275), and fee-on-transfer checks reject tokens with mismatched transfer balances (#283). |
| **Premature token removal with residual funds** | Removing a token from whitelist while treasury still holds funds, stranding member capital | **Mitigated (#272):** `remove_token` checks that `treasury_balance(token) == 0`, rejecting removal with `Error::InsufficientTreasury` if unwithdrawn reserves remain. |
| **Premature token removal with active loans** | Removing a token while loans in that asset are still outstanding, preventing clean repayment or accounting | **Mitigated (#273):** `remove_token` enforces that no active loans exist denominated in that asset, rejecting removal until all loans are fully repaid or resolved. |
| **Base token removal attack** | Malicious or accidental removal of the base token, breaking core membership fees, staking, and voting boosts | **Mitigated (#274):** Explicit guard in `remove_token` permanently prohibits removing the initialized base token (`Error::CannotRemoveBaseToken`). |
| **Instant whitelist injection** | Admin bypasses community review to immediately whitelist an asset | **Mitigated (#236, #192):** Token additions must be proposed via `propose_token_addition` and can only be executed via `execute_token_addition` after `TIMELOCK_DURATION` (48 hours), returning `TimelockNotExpired` otherwise. |

## Known gaps

- **No external audit.** The codebase is an unaudited testnet prototype; external security review is required before mainnet production.
- **Dynamic decimal-aware loan sizing:** Token policies currently assume 7 decimals matching the base token; per-asset decimal handling is tracked in #276.
- **Strict fee-on-transfer token rejection:** Further hardening of deposit/repay balance delta verifications is tracked in #283.
- **Membership fee and reveal deadlines:** Fixed membership fee and missing reveal deadline on private proposals are tracked in #47 and #48.
- **Admin-key custody:** Multi-signature threshold custody, key rotation, and monitoring are operational responsibilities outside the contract boundary.
