//! Content-hash proposal metadata. Soroban has no decentralized blob store,
//! so instead of storing documents on-chain we anchor a content hash (e.g.
//! an IPFS CID or SHA-256 digest) against a proposal. The bytes live
//! off-chain; the chain proves which document a proposal referred to.

use soroban_sdk::{Address, Bytes, Env};

use crate::events;
use crate::error::Error;
use crate::storage::{self, ProposalKind};
use crate::types::ProposalPhase;
use crate::util;

fn proposal_exists(env: &Env, kind: &ProposalKind, id: u32) -> bool {
    match kind {
        ProposalKind::Loan => storage::get_loan_proposal(env, id).is_some(),
        ProposalKind::Treasury => storage::get_treasury_proposal(env, id).is_some(),
    }
}

/// Returns the address authorized to attach/overwrite this proposal's
/// document — the borrower for loan proposals, the proposer for treasury
/// proposals. `None` if the proposal doesn't exist (caller should already
/// have checked `proposal_exists`).
fn proposal_owner(env: &Env, kind: &ProposalKind, id: u32) -> Option<Address> {
    match kind {
        ProposalKind::Loan => storage::get_loan_proposal(env, id).map(|p| p.borrower),
        ProposalKind::Treasury => storage::get_treasury_proposal(env, id).map(|p| p.proposer),
    }
}

// `env.events().publish` is deprecated in soroban-sdk in favour of
// `#[contractevent]`, but migration is a coordinated, breaking wire-format
// change (#85).  Suppress per-function so unrelated deprecations still surface.
#[allow(deprecated)]
pub fn attach_document(
    env: &Env,
    caller: Address,
    kind: ProposalKind,
    proposal_id: u32,
    content_hash: Bytes,
) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    util::require_active_member(env, &caller)?;
    if content_hash.len() > 64 {
        return Err(Error::DocumentTooLarge);
    }
    if !proposal_exists(env, &kind, proposal_id) {
        return Err(Error::ProposalNotFound);
    }
    // #21 — being an active member was sufficient to overwrite ANY proposal's
    // document. Restrict to the proposal's own proposer/borrower.
    if proposal_owner(env, &kind, proposal_id) != Some(caller.clone()) {
        return Err(Error::NotProposalOwner);
    }
    // #59 — Do not allow silent swaps of the document once voting is open.
    if let ProposalKind::Loan = kind {
        if let Some(p) = storage::get_loan_proposal(env, proposal_id) {
            if p.phase != ProposalPhase::Editing {
                return Err(Error::NotInEditingPhase);
            }
        }
    }

    storage::set_doc(env, kind, proposal_id, &content_hash);
    env.events()
        .publish((events::DOC_ATTN,), (kind, proposal_id, caller));
    Ok(())
}

pub fn get_document(
    env: &Env,
    kind: ProposalKind,
    proposal_id: u32,
) -> Result<Option<Bytes>, Error> {
    util::require_initialized(env)?;
    Ok(storage::get_doc(env, kind, proposal_id))
}
