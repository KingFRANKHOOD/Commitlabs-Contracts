#![cfg(test)]

use super::*;
use soroban_sdk::{symbol_short, testutils::{Address as _, Ledger}, Address, Env, String, Map};

pub struct TestFixture {
    pub env: Env,
    pub client: AttestationEngineContractClient<'static>,
    pub admin: Address,
    pub commitment_core: Address,
    pub verifier: Address,
}

impl TestFixture {
    pub fn setup() -> Self {
        let env = Env::default();
        let admin = Address::generate(&env);
        let commitment_core = Address::generate(&env);
        let verifier = Address::generate(&env);
        let contract_id = env.register_contract(None, AttestationEngineContract);
        let contract_id = env.register_contract(None, AttestationEngineContract);
        let client = AttestationEngineContractClient::new(&env, &contract_id);
        client.initialize(&admin, &commitment_core);
        TestFixture { env, client, admin, commitment_core, verifier }
    }
    pub fn create_test_data(&self) -> Map<String, String> {
        let mut data = Map::new(&self.env);
        data.set(String::from_str(&self.env, "value"), String::from_str(&self.env, "1000"));
        data
    }
}

#[test]
fn test_initialize() {
    let fixture = TestFixture::setup();
    let commitment_id = String::from_str(&fixture.env, "test_commitment_1");
    let data = fixture.create_test_data();
    fixture.env.mock_all_auths();
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_twice() {
    let fixture = TestFixture::setup();
    fixture.client.initialize(&fixture.admin, &fixture.commitment_core);
}

#[test]
fn test_attest() {
    let fixture = TestFixture::setup();
    let commitment_id = String::from_str(&fixture.env, "test_commitment_1");
    let data = fixture.create_test_data();
    fixture.env.mock_all_auths();
    
    // First attest
    let attestation_type = String::from_str(&fixture.env, "health_check");
    fixture.client.attest(&commitment_id, &attestation_type, &data, &fixture.verifier);
    
    let attestations = fixture.client.get_attestations(&commitment_id);
    assert_eq!(attestations.len(), 1);
}

#[test]
fn test_record_fees() {
    let fixture = TestFixture::setup();
    let commitment_id = String::from_str(&fixture.env, "test_commitment_1");
    fixture.env.mock_all_auths();
    
    // Record fees first
    fixture.client.record_fees(&commitment_id, &100_0000000);
    
    let metrics = fixture.client.get_health_metrics(&commitment_id);
    assert_eq!(metrics.fees_generated, 100_0000000);
}

#[test]
#[should_panic(expected = "fee_amount must be positive")]
fn test_record_fees_zero() {
    let fixture = TestFixture::setup();
    let commitment_id = String::from_str(&fixture.env, "test_commitment_1");
    fixture.env.mock_all_auths();
    
    // Try to record zero fees - should panic
    fixture.client.record_fees(&commitment_id, &0);
}

#[test]
fn test_record_drawdown() {
    let fixture = TestFixture::setup();
    let commitment_id = String::from_str(&fixture.env, "test_commitment_1");
    fixture.env.mock_all_auths();
    
    // Record drawdown first
    fixture.client.record_drawdown(&commitment_id, &5i128);
    
    let metrics = fixture.client.get_health_metrics(&commitment_id);
    assert_eq!(metrics.drawdown_percent, 5i128);
}

#[test]
fn test_verify_compliance() {
    let fixture = TestFixture::setup();
    let commitment_id = String::from_str(&fixture.env, "test_commitment_1");
    let data = fixture.create_test_data();
    fixture.env.mock_all_auths();
    fixture.env.mock_all_auths();
    let is_compliant = fixture.client.verify_compliance(&commitment_id);
    assert!(is_compliant);
}

#[test]
fn test_calculate_compliance_score() {
    let fixture = TestFixture::setup();
    let commitment_id = String::from_str(&fixture.env, "test_commitment_1");
    let data = fixture.create_test_data();
    fixture.env.mock_all_auths();
    fixture.env.mock_all_auths();
    let score = fixture.client.calculate_compliance_score(&commitment_id);
    assert!(score > 0 && score <= 100);
}
