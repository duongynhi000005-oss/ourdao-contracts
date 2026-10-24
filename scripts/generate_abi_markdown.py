#!/usr/bin/env python3
"""
generate_abi_markdown.py — Generates the complete Markdown ABI tables from contracts/dao/src/lib.rs.
Covers all 78 public entrypoints exactly once.
"""

import os
import re
import sys

def parse_lib_methods(lib_path):
    with open(lib_path, 'r', encoding='utf-8') as f:
        content = f.read()

    # Find impl OurDao block
    impl_start = content.find('impl OurDao')
    if impl_start == -1:
        raise ValueError("Could not find 'impl OurDao' in lib.rs")
    
    contract_code = content[impl_start:]

    # Match each pub fn
    pattern = re.compile(
        r'(?P<doc>(?:[ \t]*///[^\n]*\n)*)[ \t]*pub fn (?P<name>\w+)\s*\((?P<args>[^)]*)\)(?:\s*->\s*(?P<ret>[^{]+))?',
        re.MULTILINE
    )

    methods = {}
    for m in pattern.finditer(contract_code):
        name = m.group('name')
        raw_doc = m.group('doc') or ''
        doc_lines = [line.strip()[3:].strip() for line in raw_doc.strip().split('\n') if line.strip().startswith('///')]
        doc = ' '.join(doc_lines).strip()
        
        raw_args = m.group('args') or ''
        # Clean up args: split by comma, strip whitespace, remove env: Env or _env: Env
        args = []
        for arg in raw_args.split(','):
            arg = ' '.join(arg.split()).strip()
            if not arg or arg.startswith('env:') or arg.startswith('_env:') or arg.startswith('&env'):
                continue
            # Keep param name or param: Type
            # In README ABI style: method(param1, param2)
            param_name = arg.split(':')[0].strip()
            args.append(param_name)
        
        args_str = ', '.join(args)
        
        raw_ret = m.group('ret') or ''
        raw_ret = ' '.join(raw_ret.split()).strip()
        
        # Determine concise return type for ABI table
        ret_str = ''
        if raw_ret:
            if raw_ret.startswith('Result<'):
                inner = raw_ret[7:-1].rsplit(',', 1)[0].strip()
                if inner != '()' and inner != '':
                    ret_str = f" -> {inner}"
            elif raw_ret != '()':
                ret_str = f" -> {raw_ret}"
        
        sig = f"`{name}({args_str}){ret_str}`"
        methods[name] = {
            'sig': sig,
            'doc': doc,
            'raw_ret': raw_ret
        }
    return methods

def get_catalog_data():
    return [
        ("Lifecycle / governance", [
            ("initialize", "One-time setup: bootstrap admins, consensus threshold (basis points), membership fee, token address, and loan policy."),
            ("add_admin", "Admin-only. Adds a new address to the admin set."),
            ("remove_admin", "Admin-only. Removes an admin; the last remaining admin can never be removed."),
            ("set_pauser", "Admin-only. Designates an emergency pauser address."),
            ("revoke_pauser", "Admin-only. Revokes the designated emergency pauser."),
            ("set_consensus_threshold", "Admin-only. Updates the consensus threshold in basis points (1–10,000)."),
            ("propose_policy_update", "Admin-only. Proposes a new loan policy subject to a 48-hour timelock delay."),
            ("execute_policy_update", "Admin-only. Executes a pending policy update after the 48-hour timelock duration has elapsed."),
            ("cancel_policy_update", "Admin-only. Cancels a pending policy update proposal before execution."),
            ("set_loan_policy", "Admin-only. Direct policy update (subject to timelock in production)."),
            ("pause", "Emergency pause. Blocks all mutating operations instantly."),
            ("unpause", "Admin-only. Lifts the emergency pause."),
        ]),
        ("Membership & delegation", [
            ("register_member", "Pays the membership fee and registers as an active member in the DAO."),
            ("exit_dao", "Withdraws pro-rata treasury share, stake, and accrued yield. Blocked while member has an active loan."),
            ("claim_rewards", "Withdraws accrued loan-interest yield distributions in the base token."),
            ("delegate_vote", "Delegates voting power (base vote + staking boost) to another member."),
        ]),
        ("Lending", [
            ("request_loan", "Opens a new loan proposal in the base token (3-day editing window)."),
            ("request_loan_in_asset", "Opens a loan proposal denominated in a whitelisted non-base asset."),
            ("edit_loan_proposal", "Borrower-only. Amends loan amount during the editing window."),
            ("cancel_loan_proposal", "Borrower-only. Cancels loan proposal during the editing window."),
            ("vote_on_loan_proposal", "Casts a vote on an active loan proposal; auto-approves and disburses when threshold is met."),
            ("vote_batch", "Atomic batch voting across multiple loan proposals in a single transaction."),
            ("disburse_approved_loan", "Permissionless keeper call. Disburses principal for an approved loan waiting on treasury liquidity."),
            ("repay_loan", "Collects the full outstanding loan balance in the base token."),
            ("repay_loan_in_asset", "Repays a loan in its designated whitelisted asset."),
            ("repay_loan_partial", "Applies a partial repayment: covers accrued interest first, then reduces principal."),
            ("mark_loan_defaulted", "Permissionless keeper call. Marks overdue loan defaulted after due time plus grace period, slashing contribution."),
            ("expire_loan_proposal", "Permissionless keeper call. Persists expired state for loan proposal that failed to reach quorum."),
        ]),
        ("Treasury", [
            ("propose_treasury_withdrawal", "Proposes treasury withdrawal in base token; `private = true` routes through commit-reveal."),
            ("propose_withdrawal_in_asset", "Proposes treasury withdrawal in a whitelisted non-base asset."),
            ("vote_on_treasury_proposal", "Casts open vote on treasury proposal; auto-executes when threshold is met."),
            ("cancel_treasury_proposal", "Proposer-only. Cancels proposal while pending within the editing period."),
            ("expire_treasury_proposal", "Permissionless keeper call. Persists expired status for treasury proposal past voting deadline."),
            ("execute_treasury_proposal", "Permissionless keeper call. Executes approved withdrawal that was waiting on treasury liquidity."),
        ]),
        ("Multi-token whitelist", [
            ("propose_token_addition", "Admin-only. Proposes adding an extra asset to the whitelist behind a 48-hour timelock."),
            ("execute_token_addition", "Admin-only. Executes whitelisting of proposed token once timelock expires."),
            ("remove_token", "Admin-only. Removes token from whitelist; existing loans remain repayable but new loans/withdrawals blocked."),
            ("is_token_whitelisted", "View. Returns whether `token` is currently whitelisted."),
            ("get_whitelisted_tokens", "View. Returns array of all extra whitelisted treasury assets."),
        ]),
        ("Staking & voting power", [
            ("stake", "Stakes DAO tokens to boost voting weight quadratically (`1 + min(isqrt(stake / 100), 5)`)."),
            ("unstake", "Unstakes and withdraws staked tokens (blocked while proposals are in editing/voting)."),
            ("get_stake", "View. Returns a member's currently staked balance."),
            ("get_voting_weight", "View. Returns a member's total voting weight including staking boost and delegations."),
            ("get_stake_weight_unit", "View. Returns constant tokens per unit step for quadratic staking math."),
            ("get_max_stake_bonus", "View. Returns maximum bonus votes achievable from staking (5)."),
        ]),
        ("Name registry", [
            ("register_name", "Registers a 1:1 human-readable handle (3–32 chars); automatically releases previous name if held."),
            ("resolve_name", "View. Resolves a registered handle to its owner address."),
            ("name_of", "View. Reverse lookup: returns registered name for an address."),
        ]),
        ("Commit-reveal voting", [
            ("commit_treasury_vote", "Submits blinded commitment `sha256(support ++ salt)` during private proposal voting phase."),
            ("reveal_treasury_vote", "Reveals vote choice and salt during reveal window; verified commitment tallies into proposal."),
        ]),
        ("Content-hash proposal documents", [
            ("attach_document", "Proposer-only. Anchors an off-chain content hash (IPFS CID, max 64 bytes) during editing phase."),
            ("get_document", "View. Returns document content hash attached to a proposal."),
        ]),
        ("Views & state queries", [
            ("get_member", "View. Returns member record (join time, status, contribution, loan counts)."),
            ("get_member_loan_stats", "View. Returns member's loan history (active loans, repaid loans, total loans)."),
            ("get_loan", "View. Returns active/repaid/defaulted loan record."),
            ("get_loan_proposal", "View. Returns loan proposal (auto-refreshes expired phase based on ledger timestamp)."),
            ("get_treasury_proposal", "View. Returns treasury proposal record."),
            ("get_loan_proposal_count", "View. Returns total lifetime loan proposals created."),
            ("get_treasury_proposal_count", "View. Returns total lifetime treasury proposals created."),
            ("get_loan_policy", "View. Returns active loan policy parameters."),
            ("get_pending_policy_update", "View. Returns pending policy update proposal and execution timestamp if queued."),
            ("get_admins", "View. Returns list of all current contract admin addresses."),
            ("is_admin", "View. Returns whether address is an admin."),
            ("is_member", "View. Returns whether address is a member."),
            ("is_eligible_for_loan", "View. Returns `Ok(())` if member meets cooldown, membership duration, and active loan criteria."),
            ("get_treasury_balance", "View. Returns total unencumbered base token treasury balance."),
            ("get_treasury_balance_in_asset", "View. Returns treasury balance held in a specified whitelisted asset."),
            ("get_total_members", "View. Returns lifetime total unique registered members count."),
            ("get_active_members", "View. Returns current active members count."),
            ("get_consensus_threshold", "View. Returns consensus threshold in basis points."),
            ("get_token", "View. Returns base governance token contract address."),
            ("is_paused", "View. Returns whether contract is currently paused."),
            ("get_version", "View. Returns contract package semantic version string."),
            ("get_pending_yield", "View. Returns claimable yield accrued to member from loan repayments."),
            ("has_voted", "View. Returns whether voter has already cast or revealed a vote on a proposal."),
            ("calculate_loan_terms", "View. Quotes interest rate and total repayment for a given loan principal."),
            ("calculate_exit_share", "View. Quotes pro-rata treasury withdrawal share for a member."),
        ]),
        ("Maintenance", [
            ("bump_dao_ttl", "Permissionless keeper call. Extends storage TTL for contract instance and code."),
        ])
    ]

def generate_markdown(lib_path):
    parsed = parse_lib_methods(lib_path)
    catalog = get_catalog_data()
    
    seen = set()
    output = []
    output.append("## Public interface (ABI)\n")
    output.append("All 78 entrypoints are on the `OurDao` contract (`lib.rs`). Errors are the numeric [`Error`](#error-codes) codes below.\n")

    for section_title, items in catalog:
        output.append(f"**{section_title}**\n")
        output.append("| Method | Description |")
        output.append("|---|---|")
        for fn_name, desc in items:
            if fn_name not in parsed:
                raise ValueError(f"Function {fn_name} in catalog but not in lib.rs!")
            sig = parsed[fn_name]['sig']
            output.append(f"| {sig} | {desc} |")
            seen.add(fn_name)
        output.append("")

    all_parsed = set(parsed.keys())
    missing = all_parsed - seen
    if missing:
        raise ValueError(f"Functions in lib.rs missing from catalog: {missing}")

    return "\n".join(output)

if __name__ == '__main__':
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    lib_path = os.path.join(root, 'contracts', 'dao', 'src', 'lib.rs')
    md = generate_markdown(lib_path)
    
    if len(sys.argv) > 1 and sys.argv[1] == '--stdout':
        print(md)
    else:
        out_dir = os.path.join(root, 'contract')
        os.makedirs(out_dir, exist_ok=True)
        out_file = os.path.join(out_dir, 'abi-table.md')
        with open(out_file, 'w', encoding='utf-8', newline='') as f:
            f.write(md + '\n')
        print(f"Generated ABI table ({len(parse_lib_methods(lib_path))} methods) -> {out_file}")
