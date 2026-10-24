<!--
Please read CONTRIBUTING.md before opening this PR.
PRs against an unassigned issue will be closed with a pointer back to it.
-->

## What this changes

<!-- One or two sentences. What behavior is different after this PR? -->

## Why

<!-- The diff shows what changed. Explain why this is the right change. -->

Closes #<!-- issue number -->

## Testing

<!-- Name the test(s) you added or updated, and what would break without this change. -->

- [ ] Added or updated a test that fails without this change
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo test --locked` passes
- [ ] `cargo build --locked --target wasm32v1-none --release` succeeds
- [ ] CI is green on the latest commit
- [ ] This PR addresses a single issue only

## Contract checklist

<!-- Delete any line that genuinely doesn't apply. -->

- [ ] No existing error code in `error.rs` was renumbered, reused, or reordered
- [ ] Any new state-changing entrypoint emits an event (and the PR description says so, so the indexer can follow)
- [ ] New storage reads/writes extend TTL, consistent with existing modules
- [ ] Any change to `require_auth` behavior is explained below
- [ ] No unrelated dependency bumps; `Cargo.lock` changes are intentional
- [ ] Changes to ABI or events: README and schema files updated to match

### Authorization changes

<!-- If this PR adds, removes, or relaxes any require_auth: who can now do what they couldn't before, and why is that correct? Write "None" if not applicable. -->

None

## Anything reviewers should look at closely

<!-- Optional. Point at the part you're least sure about. -->
