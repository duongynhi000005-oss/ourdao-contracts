# Contract Invariants

This document lists every invariant the OurDAO contract relies on, cross-referenced
against the test that covers it (if any).

Untested invariants have a tracking issue linked; they must not be broken by a
contributor's change. See [CONTRIBUTING.md](../CONTRIBUTING.md#contract-specific-rules)
for the process.

---

## Accounting invariants

### A1 — Distributed interest never exceeds collected interest
**Statement:** The sum of `get_pending_yield(m)` across all active members is
less than or equal to the total `interest` passed to `distribute_interest`.
Specifically, each member receives exactly `interest / active_members` and the
remainder (`interest % active_members`) stays in the treasury.

**Tested:** ✅ `tests::properties::distributed_interest_never_exceeds_collected`
(`contracts/dao/src/tests/properties.rs`)

---

### A2 — Exit shares never exceed the treasury
**Statement:** `calculate_exit_share(m)` is bounded by `m.contribution / total_contributions
* treasury_balance`. The sum of all members' exit shares is therefore bounded by the
total treasury, so paying out every member simultaneously can never leave the
treasury in deficit.

**Tested:** ✅ `tests::properties::exit_shares_never_exceed_treasury`
(`contracts/dao/src/tests/properties.rs`)

---

### A3 — Treasury balance excludes staked principal
**Statement:** `treasury_balance` is defined as `token.balance(contract) - total_staked`.
Staked tokens are never counted as lendable or distributable treasury.

**Tested:** ⚠️ Covered implicitly by staking tests, but no dedicated invariant
property test exists. Tracking issue: open as follow-up (#134).

---

### A4 — Total contributions counter consistency
**Statement:** The internal `TotalContributions` counter must equal the sum of
`member.contribution` across all active members.

**Tested:** ❌ **Not tested.** The counter is only ever decremented (e.g., on
default slashing), never incremented to mirror actual joins. This is a known
inconsistency; tracking issue: open as follow-up (#134).

> **Note:** This is the invariant whose absence is hardest to notice without a
> written list. A stated invariant is what makes the contradiction visible.

---

## Lifecycle invariants

### L1 — `loan_id == proposal_id`
**Statement:** Every `Loan` record has the same `id` as the `LoanProposal` that
created it. This identity is used by the storage layer and `schema.sql` downstream
to join the two tables without a separate foreign-key column.

**Reference:** `contracts/dao/src/loans.rs:247–251` (comment in source).

**Tested:** ⚠️ Asserted implicitly by the loan approval path, but not as a
named invariant test. Tracking issue: open as follow-up (#134).

---

### L2 — A member with `has_active_loan` has exactly one `Active` loan
**Statement:** `member.has_active_loan == true` if and only if exactly one loan
record with `status == LoanStatus::Active` exists for that member's address.

**Tested:** ❌ **Not tested** as a standalone invariant.
Tracking issue: open as follow-up (#134).

---

### L3 — `active_members <= total_members`
**Statement:** `get_active_members()` is always less than or equal to
`get_total_members()`. `total_members` counts exits and inactive members;
`active_members` counts only `MemberStatus::ActiveMember` records.

**Tested:** ⚠️ Covered by specific exit tests; not a property test.
Tracking issue: open as follow-up (#134).

---

### L4 — `total_staked == sum of member stakes`
**Statement:** `get_total_staked()` equals the sum of `storage::get_stake(m)`
across all members. The two must stay in sync on every `stake` / `unstake`
call.

**Tested:** ❌ **Not tested** as a standalone invariant.
Tracking issue: open as follow-up (#134).

---


### L5 — Default slashing for multi-asset loans slashes base contribution
**Statement:** When a loan denominated in any asset (base token or secondary asset)
defaults, the slashing penalty (default_penalty_bps, 20% ceil-rounded) is calculated
from and deducted exclusively from the borrower's base-token membership contribution
(member.contribution). Active contributions (ctive_contributions) are reduced by
this penalty, and accrued base-token yield snapshot is advanced (forfeiting uncollected yield).
The treasury balance of the secondary asset is unaffected at default time, and no
secondary asset balance is deducted from the borrower.

**Tested:** ✅ 	ests::loans::default_in_second_asset
(contracts/dao/src/tests/loans.rs)

---

## Governance invariants

### G1 — Voting weight stays in `[1, 6]`
**Statement:** `voting_weight(m)` is always in the range `[1, MAX_STAKE_BONUS + 1]`
(`[1, 6]`). One base vote per active member, plus a square-root boost of up to
`MAX_STAKE_BONUS` (5) bonus votes (#182).

**Tested:** ✅ `properties::voting_weight_stays_in_bounds`,
`staking::voting_weight_is_capped_above_the_square_root_curve`
(`contracts/dao/src/tests/`)

---

### G2 — Voting weight is monotonic in stake
**Statement:** More stake never reduces voting weight:
`stake_a <= stake_b ⟹ voting_weight(a) <= voting_weight(b)`.

**Tested:** ✅ `properties::voting_weight_is_monotonic_in_stake`
(`contracts/dao/src/tests/properties.rs`)

---

### G3 — Interest rate stays within policy bounds
**Statement:** `calculate_loan_terms(amount).interest_rate` is always in
`[policy.min_interest_rate, policy.max_interest_rate]`, for any `amount` and
any treasury size.

**Tested:** ✅ `properties::loan_terms_rate_stays_within_policy_bounds`
(`contracts/dao/src/tests/properties.rs`)

---

### G4 — The stake boost is quadratic and capped
**Statement:** `stake_boost(s) == min(isqrt(s / STAKE_WEIGHT_UNIT), MAX_STAKE_BONUS)`,
i.e. `k` bonus votes cost `STAKE_WEIGHT_UNIT * k * k` staked tokens
(100 / 400 / 900 / 1600 / 2500). Consequences that must keep holding:
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

## Rounding invariants

### R1 — Rounding direction favours the treasury
**Statement:** Every division site across the contracts is explicitly specified so that
rounding favours the DAO treasury:
- **Member debts and penalties round UP (ceil division):** Borrowers pay at least the
  exact interest or penalty, and integer truncation cannot be exploited to evade obligations.
  Specifically: `(a * b + denom - 1) / denom`.
- **Member payouts and loan disbursements round DOWN (floor division):** Treasury outflows
  never exceed available or allocated funds, and sub-token remainders are retained by
  the DAO treasury. Specifically: `a * b / denom`.
- **Consensus quorum thresholds round UP (ceil division):** Approval bars always meet or
  exceed the required consensus percentage. Specifically: `(base * threshold + BP - 1) / BP`.
- **Staking bonuses round DOWN (floor division):** Sub-unit stakes do not grant voting
  power before `isqrt`. Specifically: `staked / STAKE_WEIGHT_UNIT`.

### Rounding and division inventory

| # | Location | Operation / Expression | Rounding | Category | Treasury Rationale | Test Coverage |
|---|----------|------------------------|----------|----------|-------------------|---------------|
| 1 | `contracts/dao/src/loans.rs:25` | `amount * BASIS_POINTS / treasury` | **Floor** | Pricing ratio | Ratio of loan amount to treasury; capped at `BASIS_POINTS`. | ✅ `properties::loan_terms_rate_stays_within_policy_bounds` |
| 2 | `contracts/dao/src/loans.rs:34` | `loan_ratio * spread / BASIS_POINTS` | **Floor** | Pricing rate | Added interest rate scaling within policy spread. | ✅ `properties::loan_terms_rate_stays_within_policy_bounds` |
| 3 | `contracts/dao/src/loans.rs:43` | `(amount * rate + BASIS_POINTS - 1) / BASIS_POINTS` | **Ceil (UP)** | Member Debt | Loan interest owed to treasury; rounds up so treasury receives full interest. | ✅ `tests::loans::loan_interest_rounds_up_to_favour_treasury` |
| 4 | `contracts/dao/src/loans.rs:121` | `treasury * max_loan_to_treasury_ratio / BASIS_POINTS` | **Floor (DOWN)** | Treasury Outflow | Max loan disbursement cap; rounds down to protect treasury reserves. | ✅ `tests::loans::loan_exceeds_treasury_ratio` |
| 5 | `contracts/dao/src/loans.rs:524` | `(outstanding * default_penalty_bps + BASIS_POINTS - 1) / BASIS_POINTS` | **Ceil (UP)** | Member Debt | Overdue penalty added on late repayment; rounds up to favour treasury. | ✅ `tests::loans::overdue_penalty_rounds_up_to_favour_treasury` |
| 6 | `contracts/dao/src/loans.rs:643` | `(contribution * default_penalty_bps + BASIS_POINTS - 1) / BASIS_POINTS` | **Ceil (UP)** | Member Debt | Default penalty slashed from borrower contribution; rounds up to protect treasury. | ✅ `tests::loans::default_penalty_rounds_up_to_favour_treasury` |
| 7 | `contracts/dao/src/loans.rs:686` | `total_interest / active_members` | **Floor (DOWN)** | Member Payout | Per-member yield distribution; rounds down and remainder is stored in `yield_remainder` (Invariant A1). | ✅ `properties::distributed_interest_never_exceeds_collected` |
| 8 | `contracts/dao/src/membership.rs:154` | `treasury * contribution / total_contributions` | **Floor (DOWN)** | Member Payout | Pro-rata exit share; rounds down so total exit shares never exceed treasury (Invariant A2). | ✅ `tests::membership::exit_share_rounds_down_to_favour_treasury`, `properties::exit_shares_never_exceed_treasury` |
| 9 | `contracts/dao/src/util.rs:129` | `staked / STAKE_WEIGHT_UNIT` | **Floor (DOWN)** | Governance | Stake weight bonus units before `isqrt`; prevents partial units from granting unearned votes. | ✅ `properties::boost_is_constant_within_each_square_band` |
| 10 | `contracts/dao/src/util.rs:185` | `(base * threshold_bps + BASIS_POINTS - 1) / BASIS_POINTS` | **Ceil (UP)** | Governance | Minimum required votes for consensus quorum; rounds up to strictly enforce voting threshold. | ✅ `tests::loans::loan_proposal_uses_dynamic_quorum_threshold` |

---

## Notes for contributors

1. Before changing any code path that touches treasury accounting, staking,
   member lifecycle, or voting weight, verify that all invariants above still hold.
2. If you add a new invariant (a new "must always be true" statement), add it here
   and either add a property test for it or open a tracking issue.
3. Untested invariants (**❌**) are the highest-priority candidates for new property
   tests — they represent the shortest path from "working code" to "a bug nobody
   noticed."

---

## Rounding policy (issue #325)

Treasury/member accounting uses explicit directions. All amounts and
intermediate products are non-negative at these division points.

| Division site | Direction | Justification |
| --- | --- | --- |
| `loans.rs::quote_terms`: loan size / treasury | **Ceil** | Never understate the borrower risk tier |
| `loans.rs::quote_terms`: rate spread / basis points | **Ceil** | Never undercharge interest rate |
| `loans.rs::quote_terms`: principal * rate / basis points | **Ceil** | Member debt rounds up |
| `loans.rs::request_loan_in_asset`: max available loan | **Floor** | Never authorize more treasury outflow than collateral ratio |
| `loans.rs::repay_loan_partial`: overdue penalty / basis points | **Ceil** | Member debt rounds up |
| `loans.rs::default_loan`: contribution slash / basis points | **Ceil** | Avoid under-penalizing default; cap at member contribution |
| `loans.rs::distribute_interest`: interest / member count | **Floor** | Member payouts never exceed collected interest; remainder carried forward |
| `membership.rs::calculate_exit_share`: treasury * contribution / total | **Floor** | Member payout cannot exceed treasury claim |
| `util.rs::stake_boost`: stake / weight unit | **Floor** | Incomplete unit earns no bonus |
| `util.rs::required_votes`: active members * threshold / 10,000 | **Ceil** | Fractional vote cannot satisfy threshold |

Where `checked_mul` overflows in a loan quote, its existing fail-safe
fallback remains. Do not replace rounding with floating-point conversions.