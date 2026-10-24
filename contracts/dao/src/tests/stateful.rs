extern crate std;

use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, String};
use std::vec;

use super::common::*;
use crate::types::MemberStatus;
use crate::OurDaoClient;

// ==================== issue #295: stateful random-sequence harness ====================
//
// Continuous random-sequence fuzzer covering all core entrypoints across the DAO lifecycle:
// join, exit, stake, unstake, request loan, vote on loan, repay loan (full & partial),
// default loan, propose treasury withdrawal, vote on treasury, execute treasury, and advance time.
//
// Invariants verified after EVERY single step:
// 1. Treasury Solvency: Contract balances & spendable treasury are non-negative across all assets.
// 2. Staked Funds Invariant: Spendable base treasury strictly equals raw balance - TotalStaked.
//    Staked funds are never lent or spent; raw token balance >= TotalStaked.
// 3. Single Active Loan: Every member has active_loans <= 1 and has_active_loan == (active_loans == 1).
// 4. Exit Share Conservation: Sum of all active exit shares <= spendable treasury balance.
// 5. Member Registry Parity: Recorded active & total members strictly match storage records.
// 6. Loan ID Parity: For all loans in storage, loan.id strictly matches its proposal id.

/// Action space covering all major DAO entrypoints and lifecycle transitions.
#[derive(Clone, Debug)]
pub enum DaoAction {
    Join {
        actor_idx: usize,
    },
    Exit {
        actor_idx: usize,
    },
    Stake {
        actor_idx: usize,
        amount: i128,
    },
    Unstake {
        actor_idx: usize,
        amount: i128,
    },
    RequestLoan {
        actor_idx: usize,
        amount: i128,
        is_asset2: bool,
    },
    VoteLoan {
        voter_idx: usize,
        proposal_idx: usize,
        support: bool,
    },
    RepayLoan {
        actor_idx: usize,
        loan_idx: usize,
        is_partial: bool,
        partial_amount: i128,
    },
    DefaultLoan {
        loan_idx: usize,
    },
    ProposeTreasury {
        actor_idx: usize,
        amount: i128,
        is_asset2: bool,
    },
    VoteTreasury {
        voter_idx: usize,
        proposal_idx: usize,
        support: bool,
    },
    ExecuteTreasury {
        proposal_idx: usize,
    },
    AdvanceTime {
        secs: u64,
    },
}

/// Strategy generating uniformly distributed random DAO actions.
fn action_strategy() -> impl Strategy<Value = DaoAction> {
    prop_oneof![
        (0usize..5).prop_map(|actor_idx| DaoAction::Join { actor_idx }),
        (0usize..5).prop_map(|actor_idx| DaoAction::Exit { actor_idx }),
        (0usize..5, 10i128..=500i128)
            .prop_map(|(actor_idx, amount)| DaoAction::Stake { actor_idx, amount }),
        (0usize..5, 10i128..=500i128)
            .prop_map(|(actor_idx, amount)| DaoAction::Unstake { actor_idx, amount }),
        (0usize..5, 50i128..=500i128, any::<bool>()).prop_map(|(actor_idx, amount, is_asset2)| {
            DaoAction::RequestLoan {
                actor_idx,
                amount,
                is_asset2,
            }
        }),
        (0usize..5, 0usize..20, any::<bool>()).prop_map(|(voter_idx, proposal_idx, support)| {
            DaoAction::VoteLoan {
                voter_idx,
                proposal_idx,
                support,
            }
        }),
        (0usize..5, 0usize..20, any::<bool>(), 10i128..=300i128).prop_map(
            |(actor_idx, loan_idx, is_partial, partial_amount)| DaoAction::RepayLoan {
                actor_idx,
                loan_idx,
                is_partial,
                partial_amount,
            }
        ),
        (0usize..20).prop_map(|loan_idx| DaoAction::DefaultLoan { loan_idx }),
        (0usize..5, 50i128..=500i128, any::<bool>()).prop_map(|(actor_idx, amount, is_asset2)| {
            DaoAction::ProposeTreasury {
                actor_idx,
                amount,
                is_asset2,
            }
        }),
        (0usize..5, 0usize..20, any::<bool>()).prop_map(|(voter_idx, proposal_idx, support)| {
            DaoAction::VoteTreasury {
                voter_idx,
                proposal_idx,
                support,
            }
        }),
        (0usize..20).prop_map(|proposal_idx| DaoAction::ExecuteTreasury { proposal_idx }),
        (10u64..=100_000u64).prop_map(|secs| DaoAction::AdvanceTime { secs }),
    ]
}

/// Helper reading PROPTEST_CASES environment variable for local deep-fuzzing runs,
/// defaulting to 20 cases for fast execution (< 60s) in CI.
fn get_proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(20)
}

/// Core invariant validation routine evaluated after every single action in the harness.
fn check_invariants(
    env: &Env,
    client: &OurDaoClient,
    token: &token::Client,
    token2: &token::Client,
    actors: &[Address],
) -> Result<(), TestCaseError> {
    // 1. Treasury Solvency
    let raw_base_balance = token.balance(&client.address);
    let spendable_base = client.get_treasury_balance();
    prop_assert!(
        raw_base_balance >= 0,
        "Raw base balance cannot be negative: {}",
        raw_base_balance
    );
    prop_assert!(
        spendable_base >= 0,
        "Spendable base treasury cannot be negative: {}",
        spendable_base
    );

    let raw_token2_balance = token2.balance(&client.address);
    let spendable_token2 = client.get_treasury_balance_in_asset(&token2.address);
    prop_assert!(
        raw_token2_balance >= 0,
        "Raw asset2 balance cannot be negative: {}",
        raw_token2_balance
    );
    prop_assert!(
        spendable_token2 >= 0,
        "Spendable asset2 treasury cannot be negative: {}",
        spendable_token2
    );

    // 2. Staked Funds Invariant
    let total_staked = env.as_contract(&client.address, || crate::storage::get_total_staked(env));
    prop_assert!(
        total_staked >= 0,
        "Total staked cannot be negative: {}",
        total_staked
    );
    prop_assert_eq!(
        spendable_base,
        raw_base_balance - total_staked,
        "Spendable base treasury must strictly equal raw balance minus total staked"
    );
    prop_assert!(
        raw_base_balance >= total_staked,
        "Raw balance must never drop below total staked funds"
    );

    // Sum of individual actor stakes equals contract total_staked
    let mut sum_individual_stakes = 0i128;
    for actor in actors {
        let st = client.get_stake(actor);
        prop_assert!(st >= 0, "Member stake cannot be negative");
        sum_individual_stakes += st;
    }
    prop_assert_eq!(
        sum_individual_stakes,
        total_staked,
        "Sum of individual actor stakes must equal contract total staked"
    );

    // 3. Member Invariants
    let mut actual_active_members = 0u32;
    let mut actual_total_members = 0u32;
    let mut sum_exit_shares = 0i128;

    for actor in actors {
        if let Some(m) = client.get_member(actor) {
            actual_total_members += 1;
            if m.status == MemberStatus::ActiveMember {
                actual_active_members += 1;

                // Invariant: Member cannot have more than 1 active loan
                prop_assert!(
                    m.active_loans <= 1,
                    "Member cannot have more than 1 active loan"
                );
                prop_assert_eq!(
                    m.has_active_loan,
                    m.active_loans == 1,
                    "has_active_loan flag must match active_loans == 1"
                );

                // Exit share calculation
                let share = client.calculate_exit_share(actor);
                prop_assert!(share >= 0, "Exit share cannot be negative");
                sum_exit_shares += share;
            } else {
                prop_assert!(
                    !m.has_active_loan,
                    "Inactive member must not have active loan"
                );
            }
        }
    }

    // 4. Exit Shares Conservation Invariant
    prop_assert!(
        sum_exit_shares <= spendable_base,
        "Sum of all active exit shares ({}) must not exceed spendable treasury ({})",
        sum_exit_shares,
        spendable_base
    );

    // 5. Active and Total Member Counts Parity
    let recorded_active = client.get_active_members();
    prop_assert_eq!(
        recorded_active,
        actual_active_members,
        "Recorded active members count ({}) must match reality ({})",
        recorded_active,
        actual_active_members
    );
    let recorded_total = client.get_total_members();
    prop_assert_eq!(
        recorded_total,
        actual_total_members,
        "Recorded total members count ({}) must match reality ({})",
        recorded_total,
        actual_total_members
    );
    prop_assert!(recorded_active <= recorded_total);

    // 6. Storage Loan ID Parity
    let total_loan_props = client.get_loan_proposal_count();
    env.as_contract(&client.address, || {
        for id in 0..total_loan_props {
            if let Some(loan) = crate::storage::get_loan(env, id) {
                assert_eq!(loan.id, id, "Storage loan.id must match proposal.id key");
            }
        }
    });

    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: get_proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Executes arbitrary interleaved random sequences across all DAO entrypoints
    /// and asserts universal system invariants after every step.
    #[test]
    fn stateful_random_sequence_harness_test(
        actions in prop::collection::vec(action_strategy(), 15..=30),
    ) {
        let s = setup(3);

        // Pre-fund DAO treasury
        mint_asset(&s, &s.token.address, &s.client.address, 1_000_000);

        // Whitelist secondary Stellar asset to test multi-token support (#236)
        let token2 = whitelist_second_asset(&s, 500_000);

        // Define pool of 5 actors (3 initial active members, 2 initially unregistered candidates)
        let actors: [Address; 5] = [
            s.members.get(0).unwrap(),
            s.members.get(1).unwrap(),
            s.members.get(2).unwrap(),
            Address::generate(&s.env),
            Address::generate(&s.env),
        ];

        // Ensure all actors have ample funds for fees, stakes, and repayments
        for actor in &actors {
            mint_asset(&s, &s.token.address, actor, 500_000);
            mint_asset(&s, &token2.address, actor, 500_000);
        }

        // Check initial state invariants
        check_invariants(&s.env, &s.client, &s.token, &token2, &actors)?;

        let mut loan_proposals: std::vec::Vec<u32> = std::vec::Vec::new();
        let mut treasury_proposals: std::vec::Vec<u32> = std::vec::Vec::new();

        for action in actions {
            match action {
                DaoAction::Join { actor_idx } => {
                    let actor = &actors[actor_idx % 5];
                    let _ = s.client.try_register_member(actor);
                }
                DaoAction::Exit { actor_idx } => {
                    let actor = &actors[actor_idx % 5];
                    let _ = s.client.try_exit_dao(actor);
                }
                DaoAction::Stake { actor_idx, amount } => {
                    let actor = &actors[actor_idx % 5];
                    let _ = s.client.try_stake(actor, &amount);
                }
                DaoAction::Unstake { actor_idx, amount } => {
                    let actor = &actors[actor_idx % 5];
                    let _ = s.client.try_unstake(actor, &amount);
                }
                DaoAction::RequestLoan {
                    actor_idx,
                    amount,
                    is_asset2,
                } => {
                    let actor = &actors[actor_idx % 5];
                    let res = if is_asset2 {
                        s.client.try_request_loan_in_asset(actor, &amount, &token2.address, &None)
                    } else {
                        s.client.try_request_loan(actor, &amount, &None)
                    };
                    if let Ok(Ok(pid)) = res {
                        loan_proposals.push(pid);
                    }
                }
                DaoAction::VoteLoan {
                    voter_idx,
                    proposal_idx,
                    support,
                } => {
                    if !loan_proposals.is_empty() {
                        let pid = loan_proposals[proposal_idx % loan_proposals.len()];
                        let voter = &actors[voter_idx % 5];
                        let _ = s.client.try_vote_on_loan_proposal(voter, &pid, &support);
                    }
                }
                DaoAction::RepayLoan {
                    actor_idx,
                    loan_idx,
                    is_partial,
                    partial_amount,
                } => {
                    if !loan_proposals.is_empty() {
                        let pid = loan_proposals[loan_idx % loan_proposals.len()];
                        let actor = &actors[actor_idx % 5];
                        if is_partial {
                            let _ = s.client.try_repay_loan_partial(actor, &pid, &partial_amount);
                        } else {
                            let _ = s.client.try_repay_loan(actor, &pid);
                        }
                    }
                }
                DaoAction::DefaultLoan { loan_idx } => {
                    if !loan_proposals.is_empty() {
                        let pid = loan_proposals[loan_idx % loan_proposals.len()];
                        let _ = s.client.try_mark_loan_defaulted(&pid);
                    }
                }
                DaoAction::ProposeTreasury {
                    actor_idx,
                    amount,
                    is_asset2,
                } => {
                    let actor = &actors[actor_idx % 5];
                    let dest = &actors[(actor_idx + 1) % 5];
                    let reason = String::from_str(&s.env, "harness_withdrawal");
                    let res = if is_asset2 {
                        s.client.try_propose_withdrawal_in_asset(
                            actor,
                            &amount,
                            &token2.address,
                            dest,
                            &reason,
                            &false,
                        )
                    } else {
                        s.client.try_propose_treasury_withdrawal(
                            actor,
                            &amount,
                            dest,
                            &reason,
                            &false,
                        )
                    };
                    if let Ok(Ok(tpid)) = res {
                        treasury_proposals.push(tpid);
                    }
                }
                DaoAction::VoteTreasury {
                    voter_idx,
                    proposal_idx,
                    support,
                } => {
                    if !treasury_proposals.is_empty() {
                        let tpid = treasury_proposals[proposal_idx % treasury_proposals.len()];
                        let voter = &actors[voter_idx % 5];
                        let _ = s.client.try_vote_on_treasury_proposal(voter, &tpid, &support);
                    }
                }
                DaoAction::ExecuteTreasury { proposal_idx } => {
                    if !treasury_proposals.is_empty() {
                        let tpid = treasury_proposals[proposal_idx % treasury_proposals.len()];
                        let _ = s.client.try_execute_treasury_proposal(&tpid);
                    }
                }
                DaoAction::AdvanceTime { secs } => {
                    advance(&s.env, secs);
                }
            }

            // Continuous Invariant Checking: must pass after EVERY single step
            check_invariants(&s.env, &s.client, &s.token, &token2, &actors)?;
        }
    }
}
