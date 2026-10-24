# LoanPolicy Parameter Guide

`LoanPolicy` has thirteen tunable parameters that control who can borrow, how
much, at what rate, and what happens when a loan goes wrong. This document
explains each parameter: what it controls, its units, its interactions with
other parameters, and a suggested starting value.

The final section provides a worked example for a small DAO and calls out
pathological combinations explicitly.

---

## Parameter reference

### `min_membership_duration: u64`
**Units:** seconds since Unix epoch, stored in `Member.join_time`.
**What it controls:** How long a member must have been in the DAO before
they can request a loan.  
**Interactions:** Read against `member.join_time` at loan-request time.
Setting this above the typical membership age of your DAO members will make
lending impossible in practice until enough time has passed.  
**Suggested start:** `2_592_000` (30 days). Long enough to filter drive-by
borrowers; short enough not to strand early members.

---

### `membership_contribution: i128`
**Units:** token stroops (the DAO's native token, smallest unit).  
**What it controls:** The minimum contribution a member must have on record
to be eligible for a loan. Set this to the membership fee to require that a
member has not been slashed below the threshold.  
**Interactions:** Read against `member.contribution` at loan-request time.
If a member's contribution has been reduced by a default penalty, they may
fall below this threshold.  
**Suggested start:** equal to the membership fee (e.g., `1_000`).

---

### `max_loan_duration: u64`
**Units:** seconds.  
**What it controls:** The maximum repayment term a borrower can request.  
**Interactions:** Combined with `default_grace_period`, this sets the outer
boundary on how long a defaulted loan can sit before it becomes markable.
A very long `max_loan_duration` plus a long `grace_period` can leave bad
debt open for years.  
**Suggested start:** `2_592_000` (30 days).  
**Validation:** Must be non-zero. `validate_policy` does not cap this, so
choose carefully.

---

### `min_interest_rate: u32`
**Units:** basis points (1 bp = 0.01%; 10_000 bp = 100%).  
**What it controls:** The floor of the linear interest-rate curve. A small loan
relative to the treasury is charged this minimum interest rate.  
**Interactions:** Paired with `max_interest_rate` to define the rate spread.
The property test `loan_terms_rate_stays_within_policy_bounds` verifies that the
quoted rate is always within `[min, max]`.  
**Constraint:** `min_interest_rate <= max_interest_rate`; must be ≤ 10_000 (100%).  
**Suggested start:** `500` (5%).

---

### `max_interest_rate: u32`
**Units:** basis points (1 bp = 0.01%; 10_000 bp = 100%).  
**What it controls:** The ceiling of the linear interest-rate curve. A loan reaching
`max_loan_to_treasury_ratio` is charged this maximum interest rate. Any loan amount
in between is interpolated linearly and clamped to this range.  
**Interactions:** Sets the upper bound on borrowing yield.  
**Constraint:** `max_interest_rate >= min_interest_rate`; must be ≤ 10_000 (100%).  
**Suggested start:** `2_000` (20%).

---

### `cooldown_period: u64`
**Units:** seconds.  
**What it controls:** How long a borrower must wait after their last loan
before requesting another. Read against `member.last_loan_time`.  
**Interactions:** `last_loan_time` is a Unix timestamp. A `cooldown_period`
of 0 allows back-to-back borrowing.  
**Suggested start:** `2_592_000` (30 days). Prevents serial borrowers from
monopolising treasury liquidity.

---

### `max_loan_to_treasury_ratio: u32`
**Units:** basis points of the treasury balance at loan-request time.  
**What it controls:** The ceiling on any single loan as a fraction of the
live treasury.  
**Pathological value — `0`:** A ratio of 0 makes **every loan impossible**
while still passing `validate_policy` (see [#53]). Do not set this to 0.  
**Suggested start:** `2_000` (20%). Limits exposure to any single borrower.

---

### `default_grace_period: u64`
**Units:** seconds.  
**What it controls:** Extra time after `Loan.due_time` before an admin can
call `mark_loan_defaulted`. This gives a late borrower a window to repay
without being defaulted.  
**Interactions:** During a pause, `mark_loan_defaulted` is blocked, so a
loan can silently pass `due_time + grace_period` and become instantly
markable on unpause. See [INCIDENT_RUNBOOK.md](./INCIDENT_RUNBOOK.md).  
**Suggested start:** `259_200` (3 days).

---

### `default_penalty_bps: u32`
**Units:** basis points of `member.contribution`.  
**What it controls:** How much of a defaulting borrower's on-record contribution
is slashed on default. This amount is burned (removed from the treasury's
accounting of their stake), not redistributed.  
**Interactions:** Slashing can reduce `member.contribution` below
`membership_contribution`, making the member ineligible for future loans.  
**Suggested start:** `2_000` (20%).  
**Constraint:** Must be ≤ 10_000 (100%).

---

### `editing_period: u64`
**Units:** seconds.  
**What it controls:** How long a loan proposal stays in the `Editing` phase
before it advances to `Voting`. The borrower can edit terms during this window.  
**Interactions:** `editing_period + voting_period` determines the total elapsed
time from proposal to decision. Capital is effectively committed during this
window because `disburse_approved_loan` can only be called after voting closes.  
**Constraint:** Must be non-zero and ≤ `30 * 24 * 60 * 60` (30 days).  
**Suggested start:** `259_200` (3 days).

---

### `voting_period: u64`
**Units:** seconds.  
**What it controls:** How long voting is open on a proposal.  
**Interactions:** See `editing_period` above.  
**Constraint:** Must be non-zero and ≤ 30 days.  
**Suggested start:** `259_200` (3 days).

---

### `treasury_threshold: u32`
**Units:** basis points of the total weighted vote.  
**What it controls:** The approval threshold for treasury-withdrawal proposals
(not loan proposals — those use `quorum_bps` or `consensus_threshold`).  
**Constraint:** Must be non-zero and ≤ 10_000 (100%).  
**Suggested start:** `5_100` (51%).

---

### `quorum_bps: u32`
**Units:** basis points of the total weighted vote (10_000 bp = 100%).  
**What it controls:** The approval quorum threshold specifically required for loan
proposals (`LoanProposal`). When greater than zero, it overrides the global
`consensus_threshold` (`loans::vote_on_loan_proposal`). When set to `0`, it falls
back to the global `consensus_threshold` (`storage::get_threshold(env)`).  
**Interactions:** Allows the DAO to configure dedicated governance gating for
lending distinct from general proposal consensus or treasury withdrawal thresholds.
Backwards-compatible with older zero-initialized policies.  
**Constraint:** Must be ≤ 10_000 (100%). Enforced by `validate_policy`.  
**Suggested start:** `6_000` (60%). Ensures robust majority support before capital
is disbursed.

---

## Interest curve formula and spread

`calculate_loan_terms` computes a utilization-sensitive rate from the requested
loan amount and the current lendable treasury balance.

Let:

- `A` = requested loan amount
- `T` = current treasury balance, excluding staked principal
- `B` = `10_000` basis points
- `r_min` = `min_interest_rate`
- `r_max` = `max_interest_rate`
- `s = r_max - r_min` = configured rate spread

The contract first calculates the loan ratio:

```text
loan_ratio = min(A * B / T, B)
```

For an empty treasury, `loan_ratio` is treated as `B`, which quotes the
maximum rate. The interest rate is then:

```text
rate = min(r_min + loan_ratio * s / B, r_max)
total_repayment = A + A * rate / B
```

With a 5% floor and 20% ceiling, the spread is 1,500 bp:

| Loan / treasury | Ratio (bp) | Quoted rate | Meaning |
|---:|---:|---:|---|
| 0% | 0 | 5.00% | Minimum policy rate |
| 10% | 1,000 | 6.50% | Low utilization |
| 20% | 2,000 | 8.00% | Typical conservative ceiling |
| 50% | 5,000 | 12.50% | Material concentration risk |
| 100%+ | 10,000 | 20.00% | Maximum policy rate |

This curve prices larger treasury exposures more aggressively without adding a
discontinuous rate jump. Governance should consider both the spread and the
maximum permitted loan-to-treasury ratio: a high maximum ratio with a narrow
spread can underprice concentration risk.

---

## Worked interest-rate examples

These examples apply the formula above to three loan sizes against the same
treasury. They use the policy from the small-DAO example below, with
`min_interest_rate = 500` and `max_interest_rate = 2_000` (spread `s = 1_500`),
and a treasury balance `T = 1_000_000` stroops.

Each row follows the same steps:

1. `loan_ratio = A * 10_000 / 1_000_000`
2. `rate = 500 + loan_ratio * 1_500 / 10_000`, capped at `2_000`
3. `interest = A * rate / 10_000`
4. `total_repayment = A + interest`

### Example A: small loan (5% of treasury)

| Step | Value |
|---|---|
| Requested amount `A` | 50,000 |
| `loan_ratio` | 500 bp (5%) |
| Added rate `500 * 1_500 / 10_000` | 75 bp |
| Quoted rate `500 + 75` | **575 bp (5.75%)** |
| Interest `50_000 * 575 / 10_000` | 2,875 |
| `total_repayment` | **52,875** |

### Example B: typical loan (20% of treasury)

| Step | Value |
|---|---|
| Requested amount `A` | 200,000 |
| `loan_ratio` | 2,000 bp (20%) |
| Added rate `2_000 * 1_500 / 10_000` | 300 bp |
| Quoted rate `500 + 300` | **800 bp (8.00%)** |
| Interest `200_000 * 800 / 10_000` | 16,000 |
| `total_repayment` | **216,000** |

This loan sits exactly at the `max_loan_to_treasury_ratio` in the small-DAO
profile, so it is the largest loan that profile allows.

### Example C: large loan (60% of treasury)

| Step | Value |
|---|---|
| Requested amount `A` | 600,000 |
| `loan_ratio` | 6,000 bp (60%) |
| Added rate `6_000 * 1_500 / 10_000` | 900 bp |
| Quoted rate `500 + 900` | **1,400 bp (14.00%)** |
| Interest `600_000 * 1_400 / 10_000` | 84,000 |
| `total_repayment` | **684,000** |

This loan is well above the small-DAO ratio cap of 20%. It is included to show
how the rate rises with concentration. Under the small-DAO profile it exceeds
the 20% `max_loan_to_treasury_ratio` and would be refused with
`ExceedsTreasuryRatio`.

### Reading the three examples together

| Loan | Share of treasury | Rate | Interest | Total repaid |
|---:|---:|---:|---:|---:|
| 50,000 | 5% | 5.75% | 2,875 | 52,875 |
| 200,000 | 20% | 8.00% | 16,000 | 216,000 |
| 600,000 | 60% | 14.00% | 84,000 | 684,000 |

The rate rises by 225 bp from A to B and by 600 bp from B to C. Interest rises
faster than the loan size because the rate itself rises with concentration.
Governance should check that the rate at the maximum permitted ratio is high
enough to compensate for the concentration risk it allows.

To reproduce these numbers on-chain, call `calculate_loan_terms(amount)` with a
treasury balance of `1_000_000` and the policy above. The expected
`interest_rate` and `total_repayment` values must match the tables.

---

## Default penalty mechanics

After `due_time + default_grace_period`, anyone may call
`mark_loan_defaulted`. The contract then:

1. marks the active loan `Defaulted`;
2. computes `penalty = contribution * default_penalty_bps / 10_000`;
3. caps the penalty at the member's recorded contribution;
4. subtracts the penalty from the member and total-contribution accounting; and
5. clears `has_active_loan`, allowing the member to exit with their reduced
   economic claim.

The penalty does **not** repay missing principal and is not a substitute for
liquidity reserves. It is a governance deterrent and loss-allocation tool.

| Penalty | Member contribution | Amount slashed | Contribution remaining |
|---:|---:|---:|---:|
| 10% | 1,000 | 100 | 900 |
| 20% | 1,000 | 200 | 800 |
| 50% | 1,000 | 500 | 500 |
| 100% | 1,000 | 1,000 | 0 |

A penalty that leaves contribution below `membership_contribution` also makes
the member ineligible for another loan until their contribution again satisfies
policy.

---

## Governance tuning guidelines

When changing lending policy, governors should evaluate parameters as one risk
budget rather than independently:

1. **Set exposure first.** Choose `max_loan_to_treasury_ratio` from the maximum
   single-borrower loss the DAO can absorb without impairing operations.
2. **Price that exposure.** Set the min/max interest spread so the maximum
   permitted exposure is meaningfully more expensive than a small loan.
3. **Match duration to liquidity.** Longer loan durations lock treasury
   liquidity for longer and should generally be paired with lower exposure
   limits or a larger liquid reserve.
4. **Treat penalties as deterrence, not collateral.** A contribution penalty
   can be much smaller than outstanding principal; do not assume it makes the
   treasury whole.
5. **Keep grace periods operationally realistic.** The grace period should be
   long enough for accidental late repayment, but short enough that bad debt is
   surfaced promptly.
6. **Model correlated borrowing.** The per-loan ratio limits one loan, not the
   sum of all active loans. Governance should monitor aggregate outstanding
   principal off-chain when setting aggressive ratios.
7. **Stage large parameter changes.** For material changes, publish the proposed
   before/after values and run the worked calculations above before voting.

### Suggested risk profiles

| Profile | Max loan / treasury | Rate floor → ceiling | Grace period | Default penalty |
|---|---:|---:|---:|---:|
| Conservative | 10% | 5% → 25% | 2 days | 30% |
| Balanced | 20% | 5% → 20% | 3 days | 20% |
| Growth-oriented | 30% | 4% → 18% | 5 days | 15% |

These are starting points, not guarantees of solvency. A DAO should adapt them
to treasury volatility, borrower concentration, governance participation, and
the liquidity needs of its members.

---

## Pathological combinations

| Combination | Effect |
|---|---|
| `max_loan_to_treasury_ratio = 0` | Rejected by `validate_policy`; governance must choose a non-zero exposure ceiling. |
| `editing_period + voting_period` > members' patience | Proposals expire before gathering quorum; effective lending rate approaches zero. |
| `max_loan_duration` >> `default_grace_period` | A defaulted loan can sit in limbo for the full loan duration before anyone can act. |
| `min_membership_duration` > age of the DAO | No member is old enough to borrow. |
| `cooldown_period` > `max_loan_duration` | A borrower cannot request a second loan until the cooldown clears, even if they repaid early. Usually fine; document if intentional. |
| `quorum_bps = 0` | Falls back to global `consensus_threshold`; safe and backwards-compatible, but loans lack tailored approval gating. |
| `quorum_bps = 10_000` | Requires 100% unanimous member vote to pass any loan; a single inactive or dissenting member permanently deadlocks lending. |

Which parameters are effectively permanent once obligations exist: any parameter
that is read live against open loans (`max_loan_duration`, `min_interest_rate`,
`max_interest_rate`, `default_grace_period`, `default_penalty_bps`) is safe to
change for future loans but affects how existing open loans appear relative to
the new policy. A retroactive-policy issue is tracked separately in this repo.

---

## Worked example: small DAO (5–20 members)

```rust
LoanPolicy {
    min_membership_duration: 2_592_000,   // 30 days — filters drive-by joiners
    membership_contribution: 1_000,       // equal to the membership fee
    max_loan_duration: 2_592_000,         // 30 days
    min_interest_rate: 500,               // 5%  — floor for any loan
    max_interest_rate: 2_000,             // 20% — ceiling for max-ratio loans
    cooldown_period: 2_592_000,           // 30 days — one loan per month max
    max_loan_to_treasury_ratio: 2_000,    // 20% of treasury per loan
    default_grace_period: 259_200,        // 3 days past due_time
    default_penalty_bps: 2_000,           // 20% contribution slashed on default
    editing_period: 259_200,              // 3 days editing
    voting_period: 259_200,               // 3 days voting
    treasury_threshold: 5_100,            // 51% approval for treasury withdrawals
    quorum_bps: 6_000,                    // 60% approval for loan proposals (overrides consensus_threshold)
}
```

**Reasoning:**
- 30-day membership minimum and cooldown protect against hit-and-run borrowers.
- 20% treasury ratio means a single loan can't empty the DAO, but is large
  enough to be useful.
- 5–20% rate range rewards the DAO fairly while staying below predatory rates.
- 3-day editing + voting keeps decisions moving without rushing members.
- 3-day grace period gives honest borrowers a short buffer without leaving bad
  debt open for long.
- 51% threshold for treasury withdrawals matches the consensus threshold,
  keeping governance consistent.
- 60% loan quorum (`quorum_bps`) requires solid member alignment before disbursing loan capital.

---

## References

- `contracts/dao/src/types.rs:58–76` — `LoanPolicy` struct definition
- `contracts/dao/src/loans.rs` — `quorum_bps` evaluation and `consensus_threshold` fallback
- `contracts/dao/src/admin.rs:8–26` — `validate_policy`
- `scripts/deploy-testnet.sh` — deploy script (add a default policy here)
- Open issues: [#53](https://github.com/ourdao/ourdao-contracts/issues/53),
  [#54](https://github.com/ourdao/ourdao-contracts/issues/54),
  [#55](https://github.com/ourdao/ourdao-contracts/issues/55)