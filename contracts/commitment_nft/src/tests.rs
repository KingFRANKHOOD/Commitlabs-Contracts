#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::{Address as _, Events, Ledger}, Address, Env, String};

// Test helpers and fixtures
pub struct TestFixture {
    pub env: Env,
    pub contract_id: Address,
    pub client: CommitmentNFTContractClient<'static>,
    pub admin: Address,
    pub owner: Address,
    pub user1: Address,
    pub user2: Address,
}

impl TestFixture {
    pub fn setup() -> Self {
        let env = Env::default();
        let contract_id = env.register_contract(None, CommitmentNFTContract);
        let client = CommitmentNFTContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);

        // Initialize contract
        client.initialize(&admin);

        TestFixture {
            env,
            contract_id,
            client,
            admin,
            owner,
            user1,
            user2,
        }
    }

    pub fn create_test_metadata(&self) -> (String, u32, u32, String, i128, Address) {
        (
            String::from_str(&self.env, "test_commitment_1"),
            30,
            10,
            String::from_str(&self.env, "safe"),
            1000_0000000,
            Address::generate(&self.env),
        )
    }
}

// Unit Tests for Commitment NFT Contract

#[test]
fn test_initialize() {
    let fixture = TestFixture::setup();
    // Verify initialization succeeded by checking we can mint
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    fixture.env.mock_all_auths();
    
    
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );
    assert_eq!(token_id, 1);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_twice_should_fail() {
    let fixture = TestFixture::setup();
    fixture.client.initialize(&fixture.admin);
}

#[test]
fn test_mint() {
    let fixture = TestFixture::setup();
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    
    fixture.env.mock_all_auths();
    
    
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );

    assert_eq!(token_id, 1);

    // Verify metadata
    let metadata = fixture.client.get_metadata(&token_id);
    assert_eq!(metadata.commitment_id, commitment_id);
    assert_eq!(metadata.duration_days, duration);
    assert_eq!(metadata.max_loss_percent, max_loss);
    assert_eq!(metadata.commitment_type, c_type);
    assert_eq!(metadata.initial_amount, amount);
    assert_eq!(metadata.asset_address, asset);

    // Verify owner
    fixture.env.mock_all_auths();
    let owner = fixture.client.owner_of(&token_id);
    assert_eq!(owner, fixture.owner);

    // Verify active status
    fixture.env.mock_all_auths();
    assert!(fixture.client.is_active(&token_id));
}

#[test]
fn test_mint_multiple() {
    let fixture = TestFixture::setup();
    fixture.env.mock_all_auths();
    
    
    for i in 0..5 {
        let commitment_id = String::from_str(&fixture.env, "commitment_test");
        let token_id = fixture.client.mint(&fixture.owner,
            &commitment_id,
            &&30,
            &&10,
            &String::from_str(&fixture.env, "aggressive"),
            &1000_0000000,
            &&Address::generate(&fixture.env),
        );
        assert_eq!(token_id, i + 1);
    }
}

#[test]
fn test_get_metadata() {
    let fixture = TestFixture::setup();
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    
    fixture.env.mock_all_auths();
    
    
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );

    let metadata = fixture.client.get_metadata(&token_id);
    
    assert_eq!(metadata.commitment_id, commitment_id);
    assert!(metadata.expires_at >= metadata.created_at);
}

#[test]
#[should_panic(expected = "NFT not found")]
fn test_get_metadata_nonexistent_token() {
    let fixture = TestFixture::setup();
    fixture.client.get_metadata(&999);
}

#[test]
fn test_owner_of() {
    let fixture = TestFixture::setup();
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    
    fixture.env.mock_all_auths();
    
    
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );

    fixture.env.mock_all_auths();
    let owner = fixture.client.owner_of(&token_id);
    assert_eq!(owner, fixture.owner);
}

#[test]
#[should_panic(expected = "NFT not found")]
fn test_owner_of_nonexistent_token() {
    let fixture = TestFixture::setup();
    fixture.env.mock_all_auths();
    fixture.client.owner_of(&999);
}

#[test]
fn test_transfer() {
    let fixture = TestFixture::setup();
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    
    fixture.env.mock_all_auths();
    
    
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );

    // Transfer to user1
    fixture.env.mock_all_auths();
    
    
    fixture.client.transfer(&fixture.owner, &fixture.user1, &token_id);

    fixture.env.mock_all_auths();
    let new_owner = fixture.client.owner_of(&token_id);
    assert_eq!(new_owner, fixture.user1);
}

#[test]
#[should_panic(expected = "not owner")]
fn test_transfer_by_non_owner() {
    let fixture = TestFixture::setup();
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    
    fixture.env.mock_all_auths();
    
    
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );

    // Try to transfer as user1 (not owner)
    fixture.env.mock_all_auths();
    
    
    fixture.client.transfer(&fixture.user1, &fixture.user2, &token_id);
}

#[test]
fn test_is_active() {
    let fixture = TestFixture::setup();
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    
    fixture.env.mock_all_auths();
    
    
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );

    fixture.env.mock_all_auths();
    assert!(fixture.client.is_active(&token_id));
}

#[test]
fn test_is_active_nonexistent_token() {
    let fixture = TestFixture::setup();
    fixture.env.mock_all_auths();
    assert!(!fixture.client.is_active(&999));
}

#[test]
fn test_settle() {
    let fixture = TestFixture::setup();
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    
    fixture.env.mock_all_auths();
    
    
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );

    // Fast forward time to after expiration
    let metadata = fixture.client.get_metadata(&token_id);
    fixture.env.ledger().with_mut(|li| {
        li.timestamp = metadata.expires_at + 1;
    });

    fixture.env.mock_all_auths();
    

    fixture.client.settle(&token_id);

    assert!(!fixture.client.is_active(&token_id));
}

#[test]
#[should_panic(expected = "commitment not expired")]
fn test_settle_before_expiration() {
    let fixture = TestFixture::setup();
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    
    fixture.env.mock_all_auths();
    
    
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );

    fixture.env.mock_all_auths();
    

    fixture.client.settle(&token_id);
}

#[test]
#[should_panic(expected = "NFT not found")]
fn test_settle_nonexistent_token() {
    let fixture = TestFixture::setup();
    fixture.env.mock_all_auths();
    
    
    fixture.client.settle(&999);
}

#[test]
#[should_panic(expected = "NFT is not active")]
fn test_transfer_after_settle() {
    let fixture = TestFixture::setup();
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    
    fixture.env.mock_all_auths();
    
    
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );

    // Fast forward time and settle
    let metadata = fixture.client.get_metadata(&token_id);
    let metadata = fixture.client.get_metadata(&token_id);
    fixture.env.ledger().with_mut(|li| {
        li.timestamp = metadata.expires_at + 1;
    });

    fixture.env.mock_all_auths();
    
    fixture.client.settle(&token_id);

    // Try to transfer after settlement
    fixture.env.mock_all_auths();
    
    
    fixture.client.transfer(&fixture.owner, &fixture.user1, &token_id);
}

// Edge Case Tests

#[test]
fn test_mint_with_zero_duration() {
    let fixture = TestFixture::setup();
    let commitment_id = String::from_str(&fixture.env, "test_commitment");
    
    fixture.env.mock_all_auths();
    
    
    // Zero duration should be allowed (contract doesn't validate)
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &&0,
        &&10,
        &String::from_str(&fixture.env, "aggressive"),
        &1000_0000000,
        &&Address::generate(&fixture.env),
    );
    assert_eq!(token_id, 1);
}

#[test]
fn test_mint_with_max_values() {
    let fixture = TestFixture::setup();
    let commitment_id = String::from_str(&fixture.env, "test_commitment");
    
    fixture.env.mock_all_auths();
    
    
    // Test with max values
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &u32::MAX,
        &&100,
        &String::from_str(&fixture.env, "aggressive"),
        &i128::MAX,
        &&Address::generate(&fixture.env),
    );
    assert_eq!(token_id, 1);
}

// Event Emission Tests

#[test]
fn test_mint_emits_event() {
    let fixture = TestFixture::setup();
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    
    fixture.env.mock_all_auths();
    
    
    let _token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );

    // Check events
    let events = fixture.env.events().all();
    assert!(events.len() > 0);
    // The event should contain mint information
}

#[test]
fn test_transfer_emits_event() {
    let fixture = TestFixture::setup();
    let (commitment_id, duration, max_loss, c_type, amount, asset) = fixture.create_test_metadata();
    
    fixture.env.mock_all_auths();
    
    
    let token_id = fixture.client.mint(&fixture.owner,
        &commitment_id,
        &duration,
        &max_loss,
        &c_type,
        &amount,
        &asset,
    );

    fixture.env.mock_all_auths();
    

    fixture.client.transfer(&fixture.owner, &fixture.user1, &token_id);

    // Check events
    let events = fixture.env.events().all();
    assert!(events.len() > 1); // Mint + Transfer events
}