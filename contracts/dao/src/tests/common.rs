use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{token, Address, Env, Vec};

use crate::error::Error;
use crate::types::LoanPolicy;
use crate::{OurDao, OurDaoClient};

pub const FEE: i128 = 1_000;
pub const MINT: i128 = 1_000_000;
pub const EDITING: u64 = 3 * 24 * 60 * 60;
pub const VOTING_PERIOD: u64 = 3 * 24 * 60 * 60;
pub const LOAN_DURATION: u64 = 30 * 24 * 60 * 60;

#[soroban_sdk::contracttype]
#[derive(Clone)]
pub enum RejectingTokenKey {
    Balance(Address),
    RejectTransfers,
}

#[soroban_sdk::contract]
pub struct RejectingToken;

#[soroban_sdk::contractimpl]
impl RejectingToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        let key = RejectingTokenKey::Balance(to);
        let current: i128 = env.storage().instance().get(&key).unwrap_or(0);
        env.storage().instance().set(&key, &(current + amount));
    }

    pub fn set_reject_transfers(env: Env, reject: bool) {
        env.storage()
            .instance()
            .set(&RejectingTokenKey::RejectTransfers, &reject);
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .instance()
            .get(&RejectingTokenKey::Balance(id))
            .unwrap_or(0)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let reject: bool = env
            .storage()
            .instance()
            .get(&RejectingTokenKey::RejectTransfers)
            .unwrap_or(false);
        if reject {
            panic!("mock token transfer rejected");
        }
        if amount < 0 {
            panic!("negative transfer");
        }

        let from_key = RejectingTokenKey::Balance(from);
        let to_key = RejectingTokenKey::Balance(to);
        let from_balance: i128 = env.storage().instance().get(&from_key).unwrap_or(0);
        if from_balance < amount {
            panic!("insufficient balance");
        }
        let to_balance: i128 = env.storage().instance().get(&to_key).unwrap_or(0);
        env.storage()
            .instance()
            .set(&from_key, &(from_balance - amount));
        env.storage()
            .instance()
            .set(&to_key, &(to_balance + amount));
    }
}

pub struct RejectingSetup<'a> {
    pub env: Env,
    pub client: OurDaoClient<'a>,
    pub token: RejectingTokenClient<'a>,
    pub members: Vec<Address>,
}

pub fn rejecting_setup(num_members: u32) -> RejectingSetup<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(RejectingToken, ());
    let token = RejectingTokenClient::new(&env, &token_id);
    let admin = Address::generate(&env);
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);

    let mut admins = Vec::new(&env);
    admins.push_back(admin);
    client.initialize(&admins, &5_100u32, &FEE, &token_id, &policy());

    let mut members = Vec::new(&env);
    for _ in 0..num_members {
        let member = Address::generate(&env);
        token.mint(&member, &MINT);
        client.register_member(&member);
        members.push_back(member);
    }

    RejectingSetup {
        env,
        client,
        token,
        members,
    }
}

pub struct Setup<'a> {
    pub env: Env,
    pub client: OurDaoClient<'a>,
    pub token: token::Client<'a>,
    pub admin: Address,
    pub members: Vec<Address>,
}

pub fn policy() -> LoanPolicy {
    LoanPolicy {
        min_membership_duration: 0,
        membership_contribution: FEE,
        max_loan_duration: 30 * 24 * 60 * 60,
        min_interest_rate: 500,   // 5%
        max_interest_rate: 2_000, // 20%
        cooldown_period: 0,
        max_loan_to_treasury_ratio: 5_000, // 50%
        default_grace_period: 0,
        default_penalty_bps: 2_000, // 20%
        editing_period: EDITING,
        voting_period: VOTING_PERIOD,
        treasury_threshold: 5_100, // 51%
        quorum_bps: 0,
    }
}

pub fn setup(num_members: u32) -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = sac.address();
    let token = token::Client::new(&env, &token_id);
    let token_mint = token::StellarAssetClient::new(&env, &token_id);

    let admin = Address::generate(&env);
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);

    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    client.initialize(&admins, &5_100u32, &FEE, &token_id, &policy());

    let mut members = Vec::new(&env);
    for _ in 0..num_members {
        let m = Address::generate(&env);
        token_mint.mint(&m, &MINT);
        client.register_member(&m);
        members.push_back(m);
    }

    Setup {
        env,
        client,
        token,
        admin,
        members,
    }
}

pub fn advance(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| li.timestamp += secs);
}

// Mock token builders with different decimals

pub fn create_token_with_decimals(env: &Env, decimals: u32) -> token::Client<'_> {
    let issuer = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_id = sac.address();
    let client = token::Client::new(env, &token_id);

    // Set decimals through metadata
    env.storage()
        .instance()
        .set(&("decimals", &token_id), &decimals);

    client
}

pub fn create_token_0_decimals(env: &Env) -> token::Client<'_> {
    create_token_with_decimals(env, 0)
}

pub fn create_token_6_decimals(env: &Env) -> token::Client<'_> {
    create_token_with_decimals(env, 6)
}

pub fn create_token_7_decimals(env: &Env) -> token::Client<'_> {
    create_token_with_decimals(env, 7)
}

pub fn create_token_18_decimals(env: &Env) -> token::Client<'_> {
    create_token_with_decimals(env, 18)
}

// Fluent DAO setup builder

pub struct DAOBuilder {
    num_members: u32,
    policy: Option<LoanPolicy>,
    env: Option<Env>,
}

impl DAOBuilder {
    pub fn new() -> Self {
        DAOBuilder {
            num_members: 0,
            policy: None,
            env: None,
        }
    }

    pub fn with_members(mut self, count: u32) -> Self {
        self.num_members = count;
        self
    }

    pub fn with_policy(mut self, policy: LoanPolicy) -> Self {
        self.policy = Some(policy);
        self
    }

    pub fn with_env(mut self, env: Env) -> Self {
        self.env = Some(env);
        self
    }

    pub fn build(self) -> Setup<'static> {
        let env = self.env.unwrap_or_default();
        env.mock_all_auths();

        let token_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_id = sac.address();
        let token = token::Client::new(&env, &token_id);
        let token_mint = token::StellarAssetClient::new(&env, &token_id);

        let admin = Address::generate(&env);
        let contract_id = env.register(OurDao, ());
        let client = OurDaoClient::new(&env, &contract_id);

        let policy = self.policy.unwrap_or_else(policy);
        let mut admins = Vec::new(&env);
        admins.push_back(admin.clone());
        client.initialize(&admins, &5_100u32, &FEE, &token_id, &policy);

        let mut members = Vec::new(&env);
        for _ in 0..self.num_members {
            let m = Address::generate(&env);
            token_mint.mint(&m, &MINT);
            client.register_member(&m);
            members.push_back(m);
        }

        Setup {
            env,
            client,
            token,
            admin,
            members,
        }
    }
}

impl Default for DAOBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// Approved-but-unfundable path (#108): a proposal passes its vote while the
// treasury can't cover it, so it parks in ApprovedPendingDisbursement and
// emits `loan_wait` / `tre_wait` instead of paying out.
/// True if any event from the most recent invocation has `name` as its first topic.
pub fn emitted(env: &Env, name: &str) -> bool {
    env.events().all().events().iter().any(|e| {
        let ContractEventBody::V0(body) = &e.body;
        matches!(body.topics.first(), Some(ScVal::Symbol(sym)) if sym.0.to_utf8_string_lossy() == name)
    })
}

/// Join a new member (mints their fee first), growing the treasury by FEE.
pub fn refill_treasury(s: &Setup) {
    let m = Address::generate(&s.env);
    token::StellarAssetClient::new(&s.env, &s.token.address).mint(&m, &MINT);
    s.client.register_member(&m);
}

/// Registers a second Stellar asset, whitelists it through the timelocked
/// admin flow and seeds the DAO treasury with `funding` of it (#236).
pub fn whitelist_second_asset<'a>(s: &Setup<'a>, funding: i128) -> token::Client<'a> {
    let issuer = Address::generate(&s.env);
    let asset = s.env.register_stellar_asset_contract_v2(issuer).address();
    s.client.propose_token_addition(&s.admin, &asset);
    advance(&s.env, crate::admin::TIMELOCK_DURATION);
    s.client.execute_token_addition(&s.admin, &asset);
    token::StellarAssetClient::new(&s.env, &asset).mint(&s.client.address, &funding);
    token::Client::new(&s.env, &asset)
}

/// Mints `amount` of `asset` to `to`.
pub fn mint_asset(s: &Setup, asset: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(&s.env, asset).mint(to, &amount);
}

/// Asserts the most recent `prop_canc` event carries `(proposal_id, now)` as
/// its data under a single `prop_canc` topic.
pub fn assert_prop_cancelled_event(env: &Env, proposal_id: u32) {
    let all = env.events().all();
    let event = all
        .events()
        .iter()
        .rev()
        .find(|e| {
            let ContractEventBody::V0(body) = &e.body;
            matches!(body.topics.first(), Some(ScVal::Symbol(sym)) if sym.0.to_utf8_string_lossy() == "prop_canc")
        })
        .expect("prop_canc event not found")
        .clone();
    let ContractEventBody::V0(body) = &event.body;
    assert_eq!(body.topics.len(), 1);
    let ScVal::Vec(Some(data)) = &body.data else {
        panic!("unexpected prop_canc data: {:?}", body.data);
    };
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], ScVal::U32(proposal_id));
    assert_eq!(data[1], ScVal::U64(env.ledger().timestamp()));
}
