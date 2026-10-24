# ADR-0001: Multi-Asset Treasury Exit Share Settlement

- **Status:** Proposed
- **Date:** 2026-10-24
- **Deciders:** OurDAO Core Architecture Team
- **Related Issues:** #236, #269, #271, #282

## Context

In OurDAO, when an active member leaves the DAO via `membership::exit_dao`, they receive an exit share calculated as:
$$\text{share} = \frac{\text{spendable\_treasury} \times \text{member.contribution}}{\text{total\_active\_contributions}}$$

Currently, `calculate_exit_share` only inspects `util::treasury_balance(env)`, which solely accounts for the base token. With the implementation of the multi-token treasury (#236), the DAO's treasury may hold diverse whitelisted tokens from loan repayments, treasury donations, or protocol fees.

If `exit_dao` only pays out base tokens:
- Exiting members forfeit their economic entitlement to secondary assets held by the treasury.
- Remaining members receive an unfair windfall of non-base assets.
- If the DAO holds low base token liquidity but substantial secondary asset reserves, exit payouts could fail with `Error::InsufficientTreasury`.

## Options Evaluated

### Option 1: Pro-Rata Multi-Asset Basket
When exiting, the member receives their exact proportional share $\frac{\text{contribution}}{\text{total\_contributions}}$ across **all** whitelisted assets held in the treasury, plus any unclaimed yield.

- **Formula per asset $A$:**
  $$\text{Payout}_A = \frac{\text{TreasuryBalance}_A \times \text{member.contribution}}{\text{total\_active\_contributions}} + \text{PendingYield}_A(\text{member})$$
- **Pros:** 100% economically neutral; perfectly fair; no oracle dependencies or price slippage.
- **Cons:** Triggers $N$ token transfer invocations (where $N$ is the number of whitelisted assets with non-zero balances).

### Option 2: Base-Token Only Exit Share + Accrued Yield in Non-Base Assets
The core equity exit share is denominated strictly in the base token (since membership contributions are strictly base-token denominated per #280). For non-base assets, the exiting member receives **only** their earned, unclaimed loan interest yield.

- **Pros:** Minimal gas consumption; preserves secondary treasury assets for future lending operations.
- **Cons:** If secondary assets were deposited directly into treasury (e.g. via donations or grants), exiting members do not share in those non-yield treasury assets.

### Option 3: Member Choice / Liquidation via DEX
Member selects which asset they wish to receive, and the DAO automatically converts secondary holdings on an external DEX.

- **Pros:** Clean single-token receipt for the member.
- **Cons:** High execution risk, sandwich attacks, slippage loss, and dependencies on external AMM liquidity pools.

## Recommendation

We recommend **Option 1 (Pro-Rata Multi-Asset Basket)** combined with the hard whitelist cap of 10 tokens established in #282:

1. **Contribution Basis:** Membership contributions are strictly tracked in base tokens (`record.contribution`), representing the member's proportional governance equity fraction $\alpha = \frac{\text{contribution}}{\text{total\_active\_contributions}}$.
2. **Execution:**
   - For each whitelisted asset $A$:
     - Available spendable balance is computed as $\text{RawBalance}_A - \text{TotalStaked}_A - \text{ReservedLoanCommitments}_A$.
     - $\text{Share}_A = \lfloor \text{Available}_A \times \alpha \rfloor$.
     - $\text{Yield}_A = \text{compute\_pending\_yield\_for}(env, member, A)$.
     - Total transfer amount for asset $A$ is $\text{Share}_A + \text{Yield}_A$.
     - If total payout $> 0$, transfer to member is executed.
3. **Soroban Resource Limits:** Iterating up to 10 whitelisted tokens stays comfortably below 25% of the Soroban CPU and memory instruction limits.

## Worked Example

Suppose:
- Total active contributions = 10,000 BaseToken
- Exiting member contribution = 1,000 BaseToken ($\alpha = 10\%$)
- Treasury balances:
  - BaseToken spendable: 50,000 (Member receives 5,000 BaseToken)
  - Asset B spendable: 20,000 (Member receives 2,000 Asset B)
  - Asset C spendable: 0 (No transfer)
- Member has 150 Asset B in unclaimed loan yield.
- **Total received:** 5,000 BaseToken + 2,150 Asset B.

## Consequences

- **Security:** Complete preservation of member equity without arbitrage opportunities.
- **Prerequisite:** Whitelist token count must be strictly capped (Issue #282) to prevent out-of-gas DoS.
