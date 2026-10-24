# Pause Functionality Implementation

## Problem Statement
The original pause implementation only blocked 8 state-changing operations while allowing critical treasury-draining operations to continue. This inverted the intent of an emergency stop, which should freeze all value transfers when an exploit is discovered.

## Changes Made

### Added Pause Checks to Critical Operations:
1. `exit_dao` - prevents members from withdrawing treasury funds
2. `claim_rewards` - prevents distribution of yield earnings  
3. `stake` - prevents new stake deposits
4. `unstake` - prevents stake withdrawals
5. `edit_loan_proposal` - prevents modification of loan proposals
6. `mark_loan_defaulted` - prevents slashing member contributions
7. `register_name` - prevents name registry changes
8. `attach_document` - prevents proposal document changes
9. `disburse_approved_loan` - prevents loan disbursements
10. `execute_approved` (treasury) - prevents treasury withdrawals
11. `expire_loan_proposal` - prevents proposal expiration

### Intentional Exceptions (No Pause Check):
1. `repay_loan` / `repay_loan_partial` - Borrowers should be able to repay loans even during emergency pauses to avoid being forced into default through no fault of their own.

2. Admin functions (`unpause`, `add_admin`, `remove_admin`, `set_pauser`, `revoke_pauser`, `set_consensus_threshold`, `set_policy`) - Governance operations remain available to control the pause state and its authorities.

### Already Had Pause Checks:
1. `register_member`
2. `request_loan` 
3. `vote_on_loan_proposal`
4. `propose_withdrawal`
5. `vote` (treasury)
6. `commit_vote`
7. `reveal_vote`

## Technical Implementation
- All pause checks use `util::require_not_paused(env)?` which calls `storage::is_paused(env)`
- The `Error::Paused` error is returned when contract is paused
- Admin can call `pause()` and `unpause()`; the assigned pauser can call `pause()`

## Emergency Stop Behavior
An admin can assign one dedicated pauser with `set_pauser()` and remove that
assignment with `revoke_pauser()`. Only admins can change the pauser assignment.
The assigned pauser can call `pause()` without receiving any governance powers.
Admins can also call `pause()` directly. Only an admin can call `unpause()`.

When `pause()` is called by an admin or the assigned pauser:
- All new value transfers out of the contract are blocked (exit_dao, claim_rewards, unstake, disburse_approved_loan, execute_approved)
- All state-changing operations are blocked (except repay_loan and admin functions)
- The contract can be resumed with `unpause()` only by an admin
- This ensures that if an exploit is discovered, admins can freeze the contract before value leaves

## Verification
All existing tests pass after implementing these changes. The pause functionality now correctly blocks all critical operations that could drain treasury or modify contract state during an emergency stop.

## Emergency Pauser Role (#184)

### Purpose & Motivation
In high-severity security incidents or active exploits, waiting for multi-party admin quorum or consensus delays can result in irreversible treasury draining. The **Emergency Pauser** role introduces an isolated operational key designed for fast, single-operator, or automated emergency halting without granting broader governance authority.

### Authority & Lifecycle Matrix

| Operation | Entrypoint | Permitted Callers | Behavior & Constraints |
|---|---|---|---|
| **Assign Pauser** | `set_pauser(caller, pauser)` | Admins only (`require_admin`) | Assigns or rotates the active emergency pauser in `DataKey::Pauser`. Emits `pauser_set` event. Fails with `AlreadyPauser` (81) if target is already the pauser. |
| **Revoke Pauser** | `revoke_pauser(caller)` | Admins only (`require_admin`) | Clears `DataKey::Pauser`. Emits `pauser_rev` event. Fails with `NotPauser` (82) if no pauser is currently configured. |
| **Trigger Pause** | `pause(caller)` | Any Admin OR Assigned Pauser | Sets `DataKey::Paused = true`. Emits `paused(caller)`. Freezes all value outflows, proposal creations, voting, and staking. |
| **Resume DAO** | `unpause(caller)` | Admins only (`require_admin`) | Sets `DataKey::Paused = false`. Emits `unpaused(caller)`. **Strictly denied to pauser (#319)** (`Error::NotAdmin`). |

### Untrusted Boundary & Security Invariants
1. **Asymmetric Permissions**: The pauser role is strictly write-only with respect to the halted state:
   - The pauser CAN call `pause(caller)` to freeze operations.
   - The pauser **CANNOT** call `unpause(caller)` (#319) — resuming the contract is exclusive to DAO admins.
   - The pauser CANNOT call `set_pauser` or `revoke_pauser`.
   - The pauser CANNOT propose, vote on, or execute policy updates, add/remove admins, or whitelist tokens.
   - The pauser CANNOT touch, disburse, or withdraw treasury funds.
2. **Denial-of-Service Containment**: If an emergency pauser key is compromised or malfunctions, an attacker can only halt new disbursements and proposals. Admins can immediately:
   - Call `unpause(admin)` to resume normal operations.
   - Call `revoke_pauser(admin)` to permanently revoke the compromised address.
3. **Borrower Protection**: Loan repayments (`repay_loan`, `repay_loan_partial`) are unconditionally exempt from pause checks. Halting the contract never traps borrowers or causes forced loan defaults.
