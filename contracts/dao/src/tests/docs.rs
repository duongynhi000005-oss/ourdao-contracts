use soroban_sdk::Bytes;

use super::common::*;
use crate::storage::ProposalKind;
use crate::Error;

#[test]
fn content_hash_document() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    let pid = s.client.request_loan(&member, &500, &None);

    let cid = Bytes::from_array(&s.env, b"QmExampleCid1234567890");
    s.client
        .attach_document(&member, &ProposalKind::Loan, &pid, &cid);
    assert_eq!(s.client.get_document(&ProposalKind::Loan, &pid), Some(cid));
}

// ==================== issue #302: document hash size limit ====================
#[test]
fn document_hash_exactly_64_bytes_accepted() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    let pid = s.client.request_loan(&member, &500, &None);

    // Create exactly 64 byte hash.
    let hash_64 = Bytes::from_slice(
        &s.env,
        &[
            0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
            46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63,
        ],
    );
    assert_eq!(hash_64.len(), 64);

    let res = s
        .client
        .try_attach_document(&member, &ProposalKind::Loan, &pid, &hash_64);
    assert!(res.is_ok());
    assert_eq!(
        s.client.get_document(&ProposalKind::Loan, &pid),
        Some(hash_64)
    );
}

#[test]
fn document_hash_65_bytes_rejected() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    let pid = s.client.request_loan(&member, &500, &None);

    // Create 65 byte hash.
    let hash_65 = Bytes::from_slice(
        &s.env,
        &[
            0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
            46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64,
        ],
    );
    assert_eq!(hash_65.len(), 65);

    let res = s
        .client
        .try_attach_document(&member, &ProposalKind::Loan, &pid, &hash_65);
    assert_eq!(res, Err(Ok(Error::DocumentTooLarge)));
}
