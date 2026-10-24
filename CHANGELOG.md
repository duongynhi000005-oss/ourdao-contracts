# Changelog

All notable changes to `ourdao-contracts` are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims to follow [Semantic Versioning](https://semver.org/). The workspace version in `Cargo.toml` has been `0.1.0` since the first commit; entries below are grouped under **Unreleased** until a version is cut, at which point the version in `Cargo.toml` should be bumped alongside a dated heading here.

## [Unreleased]

### Changed
- Replace magic numbers in loans.rs and treasury.rs with named constants from types.rs (BPS_CEIL_INCREMENT, MAX_METADATA_CID_LEN, MAX_PROPOSAL_REASON_LEN, PRIVATE_PROPOSAL_PERIOD_MULTIPLIER) (#250).
- Explicitly enforce and document that pausers cannot unpause (`admin::unpause` requires admin authorization and rejects non-admin pausers with `Error::NotAdmin`), separating emergency halt and resume privileges (#319).
- `loan_req`, `loan_appr`, `loan_rpy` and `interest` events now carry the loan's `asset` as a trailing field (#343). Existing positional fields are unchanged, so indexers that read by position keep working; `interest` is only emitted for base-token repayments.
- `exit_dao` / `calculate_exit_share` are now O(1): a maintained `ActiveContributions` total replaces the per-member scan (#353).
- `pause` and `unpause` events now include the caller (#346).
- `set_consensus_threshold` and `execute_policy_update` events now emit old and new values (#347).
- `staked` and `unstaked` events now include the resulting voting weight (#348).

### Added
- Credit loan repayment interest per-asset with dedicated yield accumulators and snapshots (`get_pending_yield_in_asset`, `claim_rewards_in_asset`), ensuring repayment in secondary asset B does not inflate base-token yield (#271).
- Stateful random-sequence test harness covering joins, exits, stakes, unstakes, loans, repayments, defaults, and treasury proposals across single and multi-token assets with continuous invariant checking (#295).
- Property test proving loan.id == proposal.id under random sequences of approvals, rejections, expiries, and cancellations across multiple members and assets (#292).
- Property test proving a member never has two active loans across random request, approve, partial repay, full repay, and default sequences (#291).
- Property test proving the sum of all member exit shares across any sequence of joins, slashes, and exits never exceeds the treasury balance (256+ cases) (#289).
- Property test proving staked funds are strictly excluded from spendable treasury and can never be consumed or disbursed by any loan (#290).
- Test suite verifying many tiny partial repayments (including single-unit stroops) across base and multi-token assets sum exactly to total_repayment (#300).
- Equivalence test suites proving single-token entrypoints (`request_loan`, `repay_loan`, `propose_treasury_withdrawal`) produce identical state transitions, values, and events as their multi-token `*_in_asset(base)` counterparts (#285).
- `scripts/deploy-testnet.sh` deploys a second test asset (`OURTESTB`) on testnet and proposes it for the treasury whitelist; `DEPLOYMENTS.md` tracks it (#342).
- `get_member_overview(member)` view returning the member record, stake, pending yield, loan stats, registered name and delegate in one call (#340).
- `get_delegate(delegator)` view returning the address a member delegated their vote to, or `None` (#341).
- `docs/events.schema.json`: machine-readable event schema covering all contract event topics (#344).
- `treasury::tally` and `loans::vote_on_loan_proposal` read the active-member count (and policy/threshold) once per call (#355).
- `bench-cost.sh` now runs a real `test_budget_scaling` benchmark that also covers `request_loan_in_asset`, `repay_loan_in_asset` and `propose_withdrawal_in_asset` (#354).

### Fixed
- Audit all eight DAO token-transfer call sites and align state effects before external token interactions (#317).
- Enforce and regression-test admin-only `unpause`: the dedicated pauser may pause but cannot resume the DAO (#319).
- Fixed `vote_batch` to authenticate the voter once for the batch instead of re-authorizing per proposal vote, preventing auth collisions (#308).
- Bind private vote commitments to voter and proposal to prevent copied-hash reveals (#324).
- Enforce a three-open-proposal quota per member across treasury and loan proposals (#326).
- Prevent unstaking during open loan/treasury ballots and private commitments (#330).
- Apply treasury-favouring rounding for debt and interest calculations; document every division (#325).
- Added regression coverage that `reveal_vote` removes the stored commitment (#356).
- Validate consensus/treasury thresholds and quorum_bps are within 1..=10000: reject zero quorum_bps in proposed policy updates and verify set_consensus_threshold bounds (#320).


### Added
- `make costs` and `tests/costs.rs`: per-entrypoint CPU, memory, ledger I/O, event size and fee estimates measured against the optimized wasm, plus how votes and `exit_dao` scale with membership; `make size` reports raw and optimized wasm size (#357, #358).
- Multi-token treasury (#236): `request_loan_in_asset`, `repay_loan_in_asset`, `propose_withdrawal_in_asset`, `get_treasury_balance_in_asset`, and the token whitelist (`propose_token_addition` / `execute_token_addition` behind the 48h timelock, `remove_token`, `is_token_whitelisted`, `get_whitelisted_tokens`). The base token is always whitelisted. New append-only error codes 83–87 and events `loan_asst`, `tre_asset`, `tok_prop`, `tok_add`, `tok_rem`.
- `cancel_loan_proposal` and `cancel_treasury_proposal` entrypoints for the existing #185 cancel logic.
- CI: a fast `check` job (`cargo check --locked --all-targets`) that `build-and-test` and `reproducible-build` depend on, so a broken merge fails within minutes (#238).
- Regression coverage ensuring `get_version` stays aligned with the Cargo package version (#372).
- `repay_loan_partial` for partial loan repayment.
- Loan and proposal lifecycle expiry, a `has_voted` view, name validation and an O(1) pull-based yield accumulator.
- `expire_treasury_proposal`, exposed in `lib.rs`; treasury proposals now expire after `VOTING_PERIOD`.
- Commit-reveal phase guards for private proposals (#57).
- `NotEditingPhase` and `DocumentTooLarge` errors; document hashes are limited to 64 bytes (#58, #59).
- `YieldRemainder` storage: `distribute_interest` carries rounding remainders and emits an event (#60).
- Loan terms are re-quoted at disbursement (#61).
- `bench-cost.sh` and a budget scaling benchmark; resource costs and the membership ceiling documented (#138).
- `generate-abi.sh` for a wasm-derived interface (#135).
- Property tests for exit-share, interest and staking-weight math.
- `util::isqrt`: an overflow-free integer square root over the whole `i128` range, backing the quadratic staking curve (#182).
- CI: `cargo clippy`, `cargo audit` and a `cargo-llvm-cov` coverage floor of 95% (#62).
- CI: MSRV job tests minimum supported Rust version (1.84) across all targets (#381).
- CI: markdown link checker validates all `.md` files for broken references (#383).
- PR template: checkboxes for "single issue only" and "ABI/event changes" (#382).

### Changed
- Pending yield has a single definition (`membership::compute_pending_yield`) shared by `get_pending_yield`, `claim_rewards` and `exit_dao` (#92).
- Loan proposals that time out without quorum are now stored as `status = Expired` (previously `Rejected`), matching `phase` and the `loan_exp` event; early rejection stays `Rejected` / `loan_rej` (#86).
- Defaulting a loan forfeits the borrower's unclaimed yield; yield stays equal-split and exit share stays contribution-weighted, now documented in the README (#89).
- **ABI:** config and registry views (`get_token`, `get_admins`, `is_admin`, `get_loan_policy`, `get_consensus_threshold`, `get_treasury_balance`, `calculate_loan_terms`, `calculate_exit_share`, `resolve_name`, `name_of`, `get_document`) now return `Result` and yield `NotInitialized` on an uninitialized contract; new `is_initialized` view (#88).
- `Member.join_ledger` renamed to `join_time` (#55).
- Staking boost is quadratic instead of linear: `voting_weight` is now `1 + min(isqrt(stake / 100), 5)`, so the *k*-th bonus vote costs `k² × 100` staked tokens and a whale can no longer buy proportional control (#182).
- Admin policy bounds tightened (#53, #54); loan eligibility checks and the `is_eligible_for_loan` signature corrected (#52, #56).
- Proposal document attachment is restricted to the proposal's own owner, and rejected after the editing phase (#59).

### Fixed
- Release wasm size: `crate-type = ["cdylib", "rlib"]` made `cargo build` skip `-C lto`, so `lto = true` never applied to `make build`, CI or `release.yml` builds. With `cdylib` only, plus `DataKey` kept out of the contract spec and a few host-side `Vec` lookups, the release wasm shrinks from 110,988 to 68,329 bytes and the optimized wasm from 85,833 to 58,372, and CPU per call on the optimized wasm drops by 4-14%. No entrypoint, event, error or public type changed (#357).
- `main` compiles again after the corrupted merge of #216 (#235); `cargo fmt` and `cargo clippy -D warnings` pass again.
- `total_members` is now decremented on exit, so exit shares no longer underpay (#11).
- Stake locking and `due_time` on loan approval (#46, #51); reveal period end (#45).
- CI workflow and formatting violations.

### Documentation
- README "Resource costs" table, with the membership ceiling corrected from the measured footprint: `exit_dao` exceeds the SDK's 100-entry mainnet snapshot at 86 members and votes at 90 (#358).
- ADR-0004 (loan due-date extension proposals) and ADR-0005 (interest discount from member loan history) (#359, #360).
- Removed the orphaned, never-compiled `src/test.rs`; its four unique tests were ported to `src/tests/` (#237).
- `SECURITY.md` with a coordinated disclosure policy.
- `docs/MIGRATION.md`, `docs/INVARIANTS.md`, `docs/LOAN_POLICY_GUIDE.md` and the pause/unpause incident runbook.
- `docs/THREAT_MODEL.md`: actors, trust assumptions and known gaps (#133).