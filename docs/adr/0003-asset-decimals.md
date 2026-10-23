# ADR-0003: Decimal-Aware Loan Policy and Asset Normalization

- **Status:** Accepted
- **Date:** 2026-10-24

## Context

OurDAO originally assumed a single-token treasury operating exclusively on Stellar's native 7-decimal precision (stroops, where $1.00 = 10^7$ units). With the introduction of multi-token support (#236), whitelisted tokens may implement different decimal precisions. For example, standard USD stablecoins such as USDC implement 6 decimals ($1.00 = 10^6$ units), while certain wrapped tokens implement 18 decimals ($1.00 = 10^{18}$ units).

If policy checks, interest rate curves, or loan size limits assume base-token decimals (7 decimals):
1. A 6-decimal asset loan of $500 (500,000,000 units) compared directly against a base-scale calculation would appear 10x smaller than its real-world value, artificially depressing interest rates and bypassing treasury exposure caps.
2. Fixed policy amounts (such as `membership_contribution` or minimum loan limits) defined in base-token precision would demand 10x more or less real value depending on the token's decimal precision.

## Decision

We adopt **Dynamic Decimal Awareness and Scale-Invariant Ratio Normalization** across all loan lifecycle, interest curve, and treasury exposure entrypoints:

1. **Asset-Native Scale Invariance**:
   - The interest rate curve (`quote_terms`) and treasury exposure limit (`max_loan_to_treasury_ratio`) operate directly on the loan asset's native amount and the treasury balance held in that specific asset (`util::treasury_balance_in_asset`).
   - Because both `amount` and `treasury` are denominated in the same token, the ratio:
     $$\text{loan\_ratio} = \min\left(\text{BASIS\_POINTS}, \left\lceil \frac{\text{amount} \times \text{BASIS\_POINTS}}{\text{treasury}} \right\rceil\right)$$
     is scale-invariant and produces identical basis-point fractions for equivalent real-value proportions across any decimal precision (e.g., 6, 7, or 18 decimals).
2. **Decimal Querying & Normalization Engine**:
   - Asset decimal precision is queried dynamically from the token contract via the SEP-41 `decimals()` method (`util::token_client_for(env, asset).decimals()`).
   - A helper `normalize_decimal_amount(amount, from_decimals, to_decimals)` allows exact scaling between precisions using $10^{|\Delta \text{decimals}|}$.
   - For absolute amounts specified in base-token scale, amounts are scaled to/from the target asset's precision.
3. **Public ABI Compatibility**:
   - `calculate_loan_terms_in_asset(amount, asset)` prices terms directly against the asset's treasury and verifies whitelisting status.
   - `calculate_loan_terms(amount)` remains fully backwards-compatible for single-token integrations, pricing against base token decimals.
   - `edit_loan_proposal` strictly enforces `max_loan_to_treasury_ratio` against the asset's active treasury balance.

## Alternatives considered

- **Per-Asset Loan Policy Configuration**:
  - Store a dedicated `LoanPolicy` or cap mapping for each whitelisted asset (`Map<Address, AssetPolicy>`).
  - *Rejected*: Substantially increases storage footprint, complicates admin governance votes, and creates risk of inconsistent parameters across assets.
- **Fixed Hardcoded 7-Decimal Assumption**:
  - Disallow any token whose decimals differ from 7.
  - *Rejected*: Incompatible with major real-world Stellar ecosystem assets, notably USDC (6 decimals).

## Consequences

- Real-value loans across assets with different decimals (e.g. 6 decimals vs 7 decimals) receive identical ratio checks and identical interest rate pricing.
- No breaking ABI changes to existing entrypoints or the `LoanPolicy` public struct.
- Complete unit and integration test coverage verifies 6-decimal and 7-decimal assets in `contracts/dao/src/tests/loans.rs`.
