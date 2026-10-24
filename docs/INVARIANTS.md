# Contract Invariants

This document lists every invariant the OurDAO contract relies on, cross-referenced
against the test that covers it (if any). Following the introduction of the multi-token
treasury (#236), each invariant explicitly declares its **Asset scope** (whether it
applies strictly to the `base_token`, per whitelisted asset, or globally across all assets).

Untested invariants have a tracking issue linked; they must not be broken by a
contributor's change. See [CONTRIBUTING.md](../CONTRIBUTING.md#contract-specific-rules)
for the process.

---

## Accounting invariants

### A1 — Distributed interest never exceeds collected interest
**Asset scope:** Base token (for yield distributions) and Per-Asset (for treasury interest retention).

**Statement:** The sum of `get_pending_yield(m)` across all active members is
less than or equal to the total `interest` passed to `distribute_interest`.
Specifically, for base-token loans, each member receives exactly `interest / active_members`
and the remainder (`interest % active_members`) stays in the treasury. For secondary asset loans
(`repay_loan_in_asset`), interest is retained directly in the asset's treasury balance rather than
inflating base-token yield accumulators, guaranteeing that distributed yield never exceeds collected interest.

**Tested:** ✅ `tests::properties::distributed_interest_never_exceeds_collected`
(`contracts/dao/src/tests/properties.rs`)

---

### A2 — Exit shares never exceed the treasury
**Asset scope:** Per-Asset (applies independently to every whitelisted asset $A$ held by the DAO).

**Statement:** For any asset $A$ held in the DAO treasury, a member's exit payout
`calculate_exit_share(m)` is bounded by `(m.contribution / total_contributions) * treasury_balance(A)`.
The sum of all members' exit shares across asset $A$ is therefore strictly bounded by the total
treasury balance of asset $A$:
$$\sum_{m \in \text{Active}} \text{exit\_share}_A(m) \le \text{treasury\_balance}(A)$$
Paying out every member simultaneously can never leave any asset in deficit or strand unbacked claims.

**Tested:** ✅ `tests::properties::exit_shares_never_exceed_treasury`
(`contracts/dao/src/tests/properties.rs`)

---

### A3 — Treasury balance excludes staked principal
**Asset scope:** Base token for staking subtraction; Per-Asset for solvency verification.

**Statement:** For the base token, `treasury_balance(base_token)` is strictly defined as
`base_token.balance(contract) - total_staked`. For secondary whitelisted assets $A \ne \text{base\_token}$,
`treasury_balance(A)` equals `token(A).balance(contract)`.
Staked tokens (which are deposited exclusively in the base token) are segregated collateral and
are never counted as lendable, withdrawable, or distributable treasury funds.

**Tested:** ⚠️ Covered implicitly by staking tests and multi-token balance views (`get_treasury_balances`),
with formal property tests tracking under #134.

---

### A4 — Total contributions counter consistency
**Asset scope:** Base token only.

**Statement:** The internal `TotalContributions` counter represents member equity denominated strictly
in the base token. It must equal the sum of `member.contribution` across all active members.
Slashing or default penalties decrement this counter to preserve the invariant that active equity
matches real capital.

**Tested:** ❌ **Not tested.** The counter is only ever decremented (e.g., on
default slashing), never incremented to mirror actual joins. This is a known
inconsistency; tracking issue: open as follow-up (#134).

---

### A5 — Whitelist retention and asset safety
**Asset scope:** All whitelisted assets ($A \in \text{Whitelist}$).

**Statement:** An asset $A$ cannot be removed from the whitelist via `remove_token` while
`treasury_balance(A) > 0` or while active loans denominated in $A$ exist. The contract's `base_token`
is permanently whitelisted upon initialization and cannot be removed under any circumstance
(`CannotRemoveBaseToken`).

**Tested:** ✅ Covered by whitelist validation tests in `tests::admin` and multi-token lifecycle tests (#272, #273, #274).

---

## Per-Asset Invariants

The multi-token architecture (#236) separates asset accounting into discrete ledgers. The following
invariants govern per-asset operations:

### P1 — Per-Asset Treasury Solvency
**Statement:** For every whitelisted token $A$:
$$\text{token}(A).\text{balance}(\text{contract}) \ge \text{treasury\_balance}(A) + \begin{cases} \text{total\_staked} & \text{if } A = \text{base\_token} \\ 0 & \text{otherwise} \end{cases}$$
No internal accounting balance may ever exceed physical token custody.

### P2 — Cross-Asset Isolation
**Statement:** Operations denominated in token $A$ (loan disbursements, repayments, treasury withdrawals)
affect only the reserves and outstanding debt of asset $A$. Debt in token $A$ cannot be serviced using
token $B$ (`AssetMismatch`), and token $A$ yield cannot dilute or cross-subsidize token $B$ reserves.

### P3 — Multi-Asset Exit Distribution
**Statement:** When the DAO holds multiple whitelisted assets, an exiting member receives their proportional
equity share $(m.\text{contribution} / \text{total\_contributions})$ independently computed across each
held asset balance. An exit cannot drain one asset to satisfy equity in another.

---

## Lifecycle invariants

### L1 — `loan_id == proposal_id`
**Asset scope:** Global (applies to all loans regardless of asset denomination).

**Statement:** Every `Loan` record has the same `id` as the `LoanProposal` that
created it. This identity is used by the storage layer and `schema.sql` downstream
to join the two tables without a separate foreign-key column.

**Reference:** `contracts/dao/src/loans.rs:247–251` (comment in source).

**Tested:** ⚠️ Asserted implicitly by the loan approval path, but not as a
named invariant test. Tracking issue: open as follow-up (#134).

---

### L2 — A member with `has_active_loan` has exactly one `Active` loan
**Asset scope:** Global per member across all assets.

**Statement:** `member.has_active_loan == true` if and only if exactly one loan
record with `status == LoanStatus::Active` exists for that member's address across
all whitelisted currencies. A member with an outstanding base-token loan cannot borrow
in a secondary token, and vice-versa.

**Tested:** ❌ **Not tested** as a standalone invariant.
Tracking issue: open as follow-up (#134).

---

### L3 — `active_members <= total_members`
**Asset scope:** Global governance (asset-agnostic).

**Statement:** `get_active_members()` is always less than or equal to
`get_total_members()`. `total_members` counts exits and inactive members;
`active_members` counts only `MemberStatus::ActiveMember` records.

**Tested:** ⚠️ Covered by specific exit tests; not a property test.
Tracking issue: open as follow-up (#134).

---

### L4 — `total_staked == sum of member stakes`
**Asset scope:** Base token only.

**Statement:** `get_total_staked()` equals the sum of `storage::get_stake(m)`
across all members. The two must stay in sync on every `stake` / `unstake`
call. Staking is strictly denominated in the base token.

**Tested:** ❌ **Not tested** as a standalone invariant.
Tracking issue: open as follow-up (#134).

---

## Governance invariants

### G1 — Voting weight stays in `[1, 6]`
**Asset scope:** Base token only (governance voting power boost).

**Statement:** `voting_weight(m)` is always in the range `[1, MAX_STAKE_BONUS + 1]`
(`[1, 6]`). One base vote per active member, plus a square-root boost of up to
`MAX_STAKE_BONUS` (5) bonus votes (#182) calculated solely from base token stake.

**Tested:** ✅ `properties::voting_weight_stays_in_bounds`,
`staking::voting_weight_is_capped_above_the_square_root_curve`
(`contracts/dao/src/tests/`)

---

### G2 — Voting weight is monotonic in stake
**Asset scope:** Base token only.

**Statement:** More stake never reduces voting weight:
$$\text{stake}_a \le \text{stake}_b \implies \text{voting\_weight}(a) \le \text{voting\_weight}(b)$$

**Tested:** ✅ `properties::voting_weight_is_monotonic_in_stake`
(`contracts/dao/src/tests/properties.rs`)

---

### G3 — Interest rate stays within policy bounds
**Asset scope:** All whitelisted loan assets ($A \in \text{Whitelist}$).

**Statement:** `calculate_loan_terms(amount).interest_rate` is always in
`[policy.min_interest_rate, policy.max_interest_rate]`, for any `amount` and
any treasury size, whether issued in the base token or secondary assets.

**Tested:** ✅ `properties::loan_terms_rate_stays_within_policy_bounds`
(`contracts/dao/src/tests/properties.rs`)

---

### G4 — The stake boost is quadratic and capped
**Asset scope:** Base token only.

**Statement:** `stake_boost(s) == min(isqrt(s / STAKE_WEIGHT_UNIT), MAX_STAKE_BONUS)`,
i.e. `k` bonus votes cost `STAKE_WEIGHT_UNIT * k * k` staked tokens
(100 / 400 / 900 / 1600 / 2500) of the base token. Consequences that must keep holding:
- inside the band `[k^2, (k + 1)^2)` units the boost is exactly `k`;
- 4x the stake buys 2x the boost, and no more;
- past `STAKE_WEIGHT_UNIT * MAX_STAKE_BONUS^2` (2500) the boost is pinned at
  the cap, so one member can never hold more than 6 votes;
- the boost is never *more* generous than the linear rule it replaced.

**Tested:** ✅ `properties::boost_is_constant_within_each_square_band`,
`properties::boost_saturates_at_the_cap`,
`properties::boost_never_exceeds_the_linear_rule`,
`properties::isqrt_never_rounds_up`, `staking::isqrt_is_exact_on_every_perfect_square`,
`staking::quadrupling_stake_doubles_the_boost`
(`contracts/dao/src/tests/`)

---

## Notes for contributors

1. Before changing any code path that touches treasury accounting, staking,
   member lifecycle, or voting weight, verify that all invariants above still hold.
2. If you add a new invariant (a new "must always be true" statement), add it here
   and either add a property test for it or open a tracking issue.
3. Untested invariants (❌) are the highest-priority candidates for new property
   tests — they represent the shortest path from "working code" to "a bug nobody
   noticed."
