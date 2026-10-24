# ADR-0002: Per-Asset Yield Accumulator

- **Status:** Proposed
- **Date:** 2026-10-24
- **Deciders:** OurDAO Core Architecture Team
- **Related Issues:** #236, #270, #271

## Context

Prior to multi-token support (#236), OurDAO only operated with a single base token. The interest paid on loans was added to a global `DataKey::YieldAccumulator` in `distribute_interest`, and members claimed rewards in the base token via `claim_rewards`.

With the introduction of multi-token whitelisting, loans can now be denominated and disbursed in secondary assets (e.g. Asset B). However, crediting loan interest paid in Asset B into the single base-token accumulator leads to severe economic and accounting flaws:
1. **Yield Inflation / Phantom Base Yield:** Repaying a loan in Asset B artificially increases the base-token yield accumulator, allowing members to drain base-token treasury reserves even if no base-token loan interest was generated.
2. **Cross-Asset Dilution:** Asset B interest remains locked in the treasury contract while base token reserves are depleted.
3. **Accounting Inconsistency:** The contract balance of Asset B does not match its distributed yield liabilities.

## Decision

We introduce an independent, per-asset yield accounting engine with dedicated persistent storage keys:

1. **Storage Schema:**
   - `DataKey::YieldAccumulatorFor(Address)`: Tracks cumulative yield per member for each specific asset.
   - `DataKey::YieldRemainderFor(Address)`: Stores sub-divisible integer remainder when interest is partitioned across active members.
   - `DataKey::MemberYieldSnapshotFor(Address, Address)`: Tracks member's accumulator snapshot at registration, claim, or default for each asset.

2. **Backward Compatibility:**
   - For the native base token (`storage::is_base_token(env, asset)`), queries route to the existing legacy keys `DataKey::YieldAccumulator`, `DataKey::YieldRemainder`, and `DataKey::MemberYieldSnapshot(member)`.
   - Existing deployed state and base-token tests remain 100% untouched and functional.

3. **Public Interface:**
   - `claim_rewards_in_asset(env, member, asset) -> Result<i128, Error>`: Claims accrued yield for a specific whitelisted token.
   - `get_pending_yield_in_asset(env, member, asset) -> i128`: Read-only view returning unclaimed yield for a given member and asset.
   - `claim_rewards` and `get_pending_yield` continue to operate on the base token.

4. **Member Exit Integration:**
   - When calling `exit_dao`, the contract iterates over all whitelisted tokens and settles unclaimed yield in each asset alongside the base-token exit share.

## Alternatives Considered

- **Automatic Swap to Base Token via DEX (e.g., Soroban AMM / Phoenix):**
  - *Rejected:* Introducing external AMM invocations on loan repayment adds volatile slippage, oracle sandwich risks, and non-deterministic execution failure during repayment.
- **Single Normalized Value Accumulator (Oracle-priced):**
  - *Rejected:* Requires an external price oracle to normalize Asset B into base-token units. Adds trust assumptions, oracle latency, and liquidation manipulation vectors.
- **Retain Secondary Interest in Treasury Equity (No Distribution):**
  - *Rejected:* Members would earn zero yield on secondary asset loans, discouraging participation in multi-token lending facilities.

## Consequences

### Positive
- Strict mathematical isolation between different token economies.
- Zero risk of cross-token treasury drain.
- Fair distribution of loan interest directly in the currency borrowed and repaid.

### Negative / Costs
- Member exit scales linearly with the number of whitelisted tokens (mitigated by capping whitelisted tokens at 10 via #282).
- Storage footprint expands by 2 persistent keys per member per active asset.
