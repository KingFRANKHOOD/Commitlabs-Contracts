#![cfg(test)]

use super::*;
use soroban_sdk::{symbol_short, testutils::{Address as _, Ledger}, Address, Env, String};

// Test helpers and fixtures
pub struct TestFixture {
    pub env: Env,
    pub client: CommitmentCoreContractClient<'static>,
    pub admin: Address,
    pub owner: Address,
    pub user1: Address,
    pub user2: Address,
    pub nft_contract: Address,
    pub asset_address: Address,
}

impl TestFixture {
    pub fn setup() -> Self {
        let env = Env::default();
        let contract_id = env.register_contract(None, CommitmentCoreContract);
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        let nft_contract = Address::generate(&env);
        let asset_address = Address::generate(&env);

        let contract_id = env.register_contract(None, CommitmentCoreContract);
        let client = CommitmentCoreContractClient::new(&env, &contract_id);
        client.initialize(&admin, &nft_contract);

        TestFixture {
            client,
            env,
            admin,
            owner,
            user1,
            user2,
            nft_contract,
            asset_address,
        }
    }

    pub fn create_test_rules(&self) -> CommitmentRules {
        CommitmentRules {
            duration_days: 30,
            max_loss_percent: 10,
            commitment_type: String::from_str(&self.env, "safe"),
            early_exit_penalty: 5,
            min_fee_threshold: 100_0000000,
        }
    }
}

#[test]
fn test_initialize() {
    let fixture = TestFixture::setup();
    // Test that initialization works
    // The contract is already initialized in setup()
    // Just verify we can create commitments
    let rules = fixture.create_test_rules();
    fixture.env.mock_all_auths();
    let commitment_id = fixture.client.create_commitment(
        &fixture.owner,
        &&1000_0000000,
        &fixture.asset_address,
        &&rules,
    );
    assert!(!commitment_id.is_empty());
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_twice() {
    let fixture = TestFixture::setup();
    fixture.client.initialize(&fixture.admin, &fixture.nft_contract);
}

#[test]
fn test_create_commitment() {
    let fixture = TestFixture::setup();
    let rules = fixture.create_test_rules();
    fixture.env.mock_all_auths();
    let commitment_id = fixture.client.create_commitment(
        &fixture.owner,
        &&1000_0000000,
        &fixture.asset_address,
        &&rules,
    );
    assert!(!commitment_id.is_empty());
    let commitment = fixture.client.get_commitment(&commitment_id);
    assert_eq!(commitment.owner, fixture.owner);
    assert_eq!(commitment.amount, 1000_0000000);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_create_zero_amount() {
    let fixture = TestFixture::setup();
    let rules = fixture.create_test_rules();
    fixture.env.mock_all_auths();
    // This should panic because amount is 0
    fixture.client.create_commitment(
        &fixture.owner,
        &0,  // Zero amount
        &fixture.asset_address,
        &rules,
    );
}

#[test]
#[should_panic(expected = "duration must be positive")]
fn test_create_zero_duration() {
    let fixture = TestFixture::setup();
    let mut rules = fixture.create_test_rules();
    rules.duration_days = 0;
    fixture.env.mock_all_auths();
    // This should panic because duration is 0
    fixture.client.create_commitment(
        &fixture.owner,
        &1000_0000000,
        &fixture.asset_address,
        &rules,
    );
}

#[test]
fn test_update_value() {
    let fixture = TestFixture::setup();
    let rules = fixture.create_test_rules();
    fixture.env.mock_all_auths();
    let commitment_id = fixture.client.create_commitment(
        &fixture.owner,
        &1000_0000000,
        &fixture.asset_address,
        &rules,
    );
    // Update value to 1050
    fixture.env.mock_all_auths();
    fixture.client.update_value(&commitment_id, &1050_0000000);
    
    let commitment = fixture.client.get_commitment(&commitment_id);
    assert_eq!(commitment.current_value, 1050_0000000);
}

#[test]
fn test_settle() {
    let fixture = TestFixture::setup();
    let rules = fixture.create_test_rules();
    fixture.env.mock_all_auths();
    let commitment_id = fixture.client.create_commitment(
        &fixture.owner,
        &1000_0000000,
        &fixture.asset_address,
        &rules,
    );
    let commitment = fixture.client.get_commitment(&commitment_id);
    
    // Fast forward time to after expiration
    fixture.env.ledger().with_mut(|li| {
        li.timestamp = commitment.expires_at + 1;
    });
    
    fixture.env.mock_all_auths();
    fixture.client.settle(&commitment_id);
    let settled = fixture.client.get_commitment(&commitment_id);
    assert_eq!(settled.status, String::from_str(&fixture.env, "settled"));
}

#[test]
fn test_early_exit() {
    let fixture = TestFixture::setup();
    let rules = fixture.create_test_rules();
    fixture.env.mock_all_auths();
    let commitment_id = fixture.client.create_commitment(
        &fixture.owner,
        &&1000_0000000,
        &fixture.asset_address,
        &&rules,
    );
    fixture.env.mock_all_auths();
    fixture.client.early_exit(&commitment_id, &fixture.owner);
    fixture.env.mock_all_auths();
    let commitment = fixture.client.get_commitment(&commitment_id);
    assert_eq!(commitment.status, String::from_str(&fixture.env, "early_exit"));
}
