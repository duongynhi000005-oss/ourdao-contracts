use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, String};

use super::common::*;
use crate::Error;

#[test]
fn name_registry() {
    let s = setup(1);
    let owner = s.members.get(0).unwrap();
    let name = String::from_str(&s.env, "alice_dao");
    s.client.register_name(&owner, &name);
    assert_eq!(s.client.resolve_name(&name), Some(owner.clone()));
    assert_eq!(s.client.name_of(&owner), Some(name.clone()));

    // A different owner cannot claim the same name.
    let other = Address::generate(&s.env);
    let res = s.client.try_register_name(&other, &name);
    assert_eq!(res, Err(Ok(Error::NameTaken)));
}

#[test]
fn releasing_a_name_emits_an_event() {
    use soroban_sdk::testutils::Events as _;

    let s = setup(1);
    let owner = s.members.get(0).unwrap();
    let old = String::from_str(&s.env, "alice_dao");
    let new = String::from_str(&s.env, "alice_v2");

    s.client.register_name(&owner, &old);
    // First registration frees nothing: only `name_reg` is emitted.
    assert_eq!(s.env.events().all().events().len(), 1);

    // Re-registering under a new name releases the old one: `name_rel`
    // (old name, previous owner) is emitted alongside `name_reg` (#124).
    s.client.register_name(&owner, &new);
    assert_eq!(s.env.events().all().events().len(), 2);
    assert_eq!(s.client.resolve_name(&old), None);

    // Re-registering the same name releases nothing.
    s.client.register_name(&owner, &new);
    assert_eq!(s.env.events().all().events().len(), 1);
}

// ==================== issue #3: name validation ====================
#[test]
fn name_too_short_rejected() {
    let s = setup(1);
    let owner = s.members.get(0).unwrap();
    let name = String::from_str(&s.env, "ab");
    let res = s.client.try_register_name(&owner, &name);
    assert_eq!(res, Err(Ok(Error::InvalidName)));
}

#[test]
fn name_too_long_rejected() {
    let s = setup(1);
    let owner = s.members.get(0).unwrap();
    // 33 characters.
    let name = String::from_str(&s.env, "abcdefghijklmnopqrstuvwxyz1234567");
    let res = s.client.try_register_name(&owner, &name);
    assert_eq!(res, Err(Ok(Error::InvalidName)));
}

#[test]
fn name_uppercase_rejected() {
    let s = setup(1);
    let owner = s.members.get(0).unwrap();
    let name = String::from_str(&s.env, "Alice_dao");
    let res = s.client.try_register_name(&owner, &name);
    assert_eq!(res, Err(Ok(Error::InvalidName)));
}

#[test]
fn name_dot_or_space_rejected() {
    let s = setup(1);
    let owner = s.members.get(0).unwrap();

    let dot = String::from_str(&s.env, "alice.dao");
    let res = s.client.try_register_name(&owner, &dot);
    assert_eq!(res, Err(Ok(Error::InvalidName)));

    let space = String::from_str(&s.env, "alice dao");
    let res = s.client.try_register_name(&owner, &space);
    assert_eq!(res, Err(Ok(Error::InvalidName)));
}

#[test]
fn name_leading_trailing_separator_rejected() {
    let s = setup(1);
    let owner = s.members.get(0).unwrap();

    let lead = String::from_str(&s.env, "-alice");
    let res = s.client.try_register_name(&owner, &lead);
    assert_eq!(res, Err(Ok(Error::InvalidName)));

    let trail = String::from_str(&s.env, "alice-");
    let res = s.client.try_register_name(&owner, &trail);
    assert_eq!(res, Err(Ok(Error::InvalidName)));

    let lead2 = String::from_str(&s.env, "_alice");
    let res = s.client.try_register_name(&owner, &lead2);
    assert_eq!(res, Err(Ok(Error::InvalidName)));

    let trail2 = String::from_str(&s.env, "alice_");
    let res = s.client.try_register_name(&owner, &trail2);
    assert_eq!(res, Err(Ok(Error::InvalidName)));
}

// ==================== issue #301: name release on exit ====================
#[test]
fn name_freed_after_member_exits_dao() {
    let s = setup(2);
    let member = s.members.get(0).unwrap();
    let other = s.members.get(1).unwrap();
    let name = String::from_str(&s.env, "alice");

    // Member registers a name.
    s.client.register_name(&member, &name);
    assert_eq!(s.client.name_of(&member), Some(name.clone()));
    assert_eq!(s.client.resolve_name(&name), Some(member.clone()));

    // When member exits, their name should be freed.
    s.client.exit_dao(&member);
    assert_eq!(s.client.name_of(&member), None);
    assert_eq!(s.client.resolve_name(&name), None);

    // Now the name can be claimed by another active member.
    let res = s.client.try_register_name(&other, &name);
    assert!(res.is_ok());
    assert_eq!(s.client.name_of(&other), Some(name.clone()));
    assert_eq!(s.client.resolve_name(&name), Some(other.clone()));
}

#[test]
fn name_can_be_reregistered_after_exit_and_rejoin() {
    let s = setup(1);
    let member = s.members.get(0).unwrap();
    let name = String::from_str(&s.env, "alice");

    // Register a name.
    s.client.register_name(&member, &name);
    assert_eq!(s.client.name_of(&member), Some(name.clone()));

    // Exit and rejoin.
    s.client.exit_dao(&member);
    assert_eq!(s.client.name_of(&member), None);

    s.client.register_member(&member);
    assert!(s.client.is_member(&member));

    // Member can re-register the same name after rejoining.
    s.client.register_name(&member, &name);
    assert_eq!(s.client.name_of(&member), Some(name));
}

#[test]
fn name_valid_with_digits_and_separators() {
    let s = setup(1);
    let owner = s.members.get(0).unwrap();
    let name = String::from_str(&s.env, "alice-123_dao");
    s.client.register_name(&owner, &name);
    assert_eq!(s.client.resolve_name(&name), Some(owner.clone()));
}
