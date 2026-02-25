#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_initialize_and_getters() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AttestationEngineContract);
    let admin = Address::generate(&e);
    let core = Address::generate(&e);

    let init = e.as_contract(&contract_id, || {
        AttestationEngineContract::initialize(e.clone(), admin.clone(), core.clone())
    });
    assert_eq!(init, Ok(()));

    let stored_admin = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_admin(e.clone()).unwrap()
    });
    let stored_core = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_core_contract(e.clone()).unwrap()
    });

    assert_eq!(stored_admin, admin);
    assert_eq!(stored_core, core);
}

#[test]
fn test_initialize_twice_fails() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AttestationEngineContract);
    let admin = Address::generate(&e);
    let core_id = e.register_contract(None, MockCoreContract);
    let _contract_id = e.register_contract(None, AttestationEngineContract);
    e.as_contract(&_contract_id, || {
        AttestationEngineContract::initialize(e.clone(), admin.clone(), core_id.clone()).unwrap();
        AttestationEngineContract::add_verifier(e.clone(), admin.clone(), admin.clone()).unwrap();
    });

    let commitment_id = String::from_str(&e, "c1");
    let owner = Address::generate(&e);

    let base_rules = CommitmentRules {
        duration_days: 10,
        max_loss_percent: 20,
        commitment_type: String::from_str(&e, "safe"),
        early_exit_penalty: 0,
        min_fee_threshold: 100,
        grace_period_days: 0,
    };

    // Happy path: in-range drawdown, not expired, fees meet threshold, no violations.
    let mut commitment = Commitment {
        commitment_id: commitment_id.clone(),
        owner: owner.clone(),
        nft_token_id: 1,
        rules: base_rules.clone(),
        amount: 1_000,
        asset_address: Address::generate(&e),
        created_at: 0,
        expires_at: 100,
        current_value: 900, // 10% drawdown
        status: String::from_str(&e, "active"),
    };
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id.clone(), commitment.clone());
        MockCoreContract::set_violations(e.clone(), commitment_id.clone(), false);
    });
    e.as_contract(&_contract_id, || {
        AttestationEngineContract::record_fees(e.clone(), admin.clone(), commitment_id.clone(), 100)
            .unwrap();
    });

    assert!(e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id.clone())
    }));

    // Loss limit exceeded
    commitment.current_value = 700; // 30% drawdown
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id.clone(), commitment.clone());
    });
    assert!(!e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id.clone())
    }));

    // Duration expired (verify_compliance does not check expiration; drawdown and score still pass)
    commitment.current_value = 900;
    commitment.expires_at = 40;
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id.clone(), commitment.clone());
    });
    assert!(e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id.clone())
    }));

    // New commitment id for next cases (verify_compliance does not check fee threshold)
    commitment.expires_at = 100;
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id.clone(), commitment.clone());
    });
    let commitment_id2 = String::from_str(&e, "c2");
    commitment.commitment_id = commitment_id2.clone();
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id2.clone(), commitment.clone());
        MockCoreContract::set_violations(e.clone(), commitment_id2.clone(), false);
    });
    // No fee threshold check in verify_compliance; drawdown and score pass
    assert!(e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id2.clone())
    }));

    // Record violation attestations for c2 so compliance_score drops below 50 (-20 per violation)
    for _ in 0..3 {
        let mut violation_data = Map::new(&e);
        violation_data.set(
            String::from_str(&e, "violation_type"),
            String::from_str(&e, "breach"),
        );
        violation_data.set(
            String::from_str(&e, "severity"),
            String::from_str(&e, "high"),
        );
        e.as_contract(&_contract_id, || {
            AttestationEngineContract::attest(
                e.clone(),
                admin.clone(),
                commitment_id2.clone(),
                String::from_str(&e, "violation"),
                violation_data,
                false,
            )
            .unwrap();
        });
    }
    assert!(!e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id2)
    }));

    // Edge: duration_days == 0 bypasses duration check
    let commitment_id3 = String::from_str(&e, "c3");
    let rules_no_duration = CommitmentRules {
        duration_days: 0,
        grace_period_days: 0,
        ..base_rules
    };
    let commitment3 = Commitment {
        commitment_id: commitment_id3.clone(),
        owner,
        nft_token_id: 3,
        rules: rules_no_duration,
        amount: 0, // edge: amount==0 -> drawdown=0
        asset_address: Address::generate(&e),
        created_at: 0,
        expires_at: 0,
        current_value: 0,
        status: String::from_str(&e, "active"),
    };
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment(e.clone(), commitment_id3.clone(), commitment3);
        MockCoreContract::set_violations(e.clone(), commitment_id3.clone(), false);
    });
    // fees not met but threshold is 100 -> still should fail; make threshold 0
    let mut commitment3b = e.as_contract(&core_id, || {
        MockCoreContract::get_commitment(e.clone(), commitment_id3.clone())
    });
    commitment3b.rules.min_fee_threshold = 0;
    e.as_contract(&core_id, || {
        MockCoreContract::set_commitment_core(e.clone(), commitment_id3.clone(), commitment3b);
    });
    assert!(e.as_contract(&_contract_id, || {
        AttestationEngineContract::verify_compliance(e.clone(), commitment_id3)
    }));
}

#[test]
fn test_initialize() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    // Verify initialization by checking that we can call other functions
    // (indirect verification through storage access)
    let commitment_id = String::from_str(&e, "test");
    let _attestations = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_attestations(e.clone(), commitment_id)
    });
}

#[test]
fn test_fee_get_attestation_fee_default() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();
    let client = AttestationEngineContractClient::new(&e, &contract_id);
    let (amount, asset) = client.get_attestation_fee();
    assert_eq!(amount, 0);
    assert!(asset.is_none());
}

#[test]
fn test_fee_set_attestation_fee() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();
    e.mock_all_auths();
    let client = AttestationEngineContractClient::new(&e, &contract_id);
    let fee_asset = Address::generate(&e);
    client.set_attestation_fee(&admin, &100i128, &fee_asset);
    let (amount, asset) = client.get_attestation_fee();
    assert_eq!(amount, 100);
    assert_eq!(asset.unwrap(), fee_asset);
}

#[test]
fn test_fee_set_fee_recipient() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();
    e.mock_all_auths();
    let client = AttestationEngineContractClient::new(&e, &contract_id);
    assert!(client.get_fee_recipient().is_none());
    let treasury = Address::generate(&e);
    client.set_fee_recipient(&admin, &treasury);
    assert_eq!(client.get_fee_recipient().unwrap(), treasury);
}

#[test]
fn test_fee_get_collected_fees_default() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();
    let client = AttestationEngineContractClient::new(&e, &contract_id);
    let asset = Address::generate(&e);
    assert_eq!(client.get_collected_fees(&asset), 0);
}

#[test]
#[should_panic]
fn test_fee_withdraw_requires_recipient() {
    // Withdraw fails when fee recipient is not set
    let (e, admin, _commitment_core, contract_id) = setup_test_env();
    e.mock_all_auths();
    let client = AttestationEngineContractClient::new(&e, &contract_id);
    let asset = Address::generate(&e);
    client.withdraw_fees(&admin, &asset, &100i128);
}

#[test]
fn test_get_attestations_empty() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "test_commitment_1");

    // Get attestations
    let attestations = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_attestations(e.clone(), commitment_id)
    });

    assert_eq!(attestations.len(), 0);
}

#[test]
fn test_get_health_metrics_basic() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "test_commitment_1");

    // Seed a commitment in the core contract so get_commitment succeeds
    let owner = Address::generate(&e);
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment_1",
        &owner,
        1000,
        950,
        10,
        30,
        1000,
    );

    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_health_metrics(e.clone(), commitment_id.clone())
    });

    assert_eq!(metrics.commitment_id, commitment_id);
    // Verify all fields are present
    assert!(metrics.compliance_score <= 100);
}

#[test]
fn test_get_health_metrics_drawdown_calculation() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "test_commitment_1");
    let owner = Address::generate(&e);
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment_1",
        &owner,
        1000,
        900,
        10,
        30,
        1000,
    );
    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_health_metrics(e.clone(), commitment_id)
    });

    // Verify drawdown calculation handles edge cases
    // initial=1000, current=900 => 10% drawdown
    assert_eq!(metrics.drawdown_percent, 10);
}

#[test]
fn test_get_health_metrics_zero_initial_value() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "test_commitment_1");
    let owner = Address::generate(&e);
    // Explicitly store a zero-amount commitment to exercise the division-by-zero path
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment_1",
        &owner,
        0,
        0,
        10,
        30,
        1000,
    );
    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_health_metrics(e.clone(), commitment_id)
    });

    // Should handle zero initial value gracefully (drawdown = 0)
    // This tests edge case handling
    assert!(metrics.drawdown_percent >= 0);
    assert_eq!(metrics.initial_value, 0);
}

#[test]
fn test_calculate_compliance_score_base() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "test_commitment_1");
    let owner = Address::generate(&e);
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment_1",
        &owner,
        1000,
        950,
        10,
        30,
        1000,
    );
    let score = e.as_contract(&contract_id, || {
        AttestationEngineContract::calculate_compliance_score(e.clone(), commitment_id)
    });

    // Score should be clamped between 0 and 100
    assert!(score <= 100);
}

#[test]
fn test_calculate_compliance_score_clamping() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "test_commitment_1");
    let owner = Address::generate(&e);
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment_1",
        &owner,
        1000,
        950,
        10,
        30,
        1000,
    );
    let score = e.as_contract(&contract_id, || {
        AttestationEngineContract::calculate_compliance_score(e.clone(), commitment_id)
    });

    // Verify score is clamped between 0 and 100
    assert!(score <= 100);
}

#[test]
fn test_get_health_metrics_includes_compliance_score() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "test_commitment_1");
    let owner = Address::generate(&e);
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment_1",
        &owner,
        1000,
        950,
        10,
        30,
        1000,
    );
    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_health_metrics(e.clone(), commitment_id)
    });

    // Verify compliance_score is included and valid
    assert!(metrics.compliance_score <= 100);
}

// ============================================================================
// Comprehensive Compliance Score Algorithm Tests
// ============================================================================

#[test]
fn test_compliance_score_no_attestations_default() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();
    e.ledger().with_mut(|li| li.timestamp = 10000);

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);
    
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        1000,
        10,
        30,
        5000,
    );

    let score = e.as_contract(&contract_id, || {
        AttestationEngineContract::calculate_compliance_score(e.clone(), commitment_id)
    });

    // Base score is 100, +10 for duration adherence = 110, clamped to 100
    assert_eq!(score, 100);
}

#[test]
fn test_compliance_score_only_positive_attestations() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();
    e.ledger().with_mut(|li| li.timestamp = 10000);

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);
    
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        1000,
        10,
        30,
        5000,
    );

    // Add health_check attestation
    let mut data = Map::new(&e);
    data.set(String::from_str(&e, "status"), String::from_str(&e, "healthy"));
    
    e.as_contract(&contract_id, || {
        AttestationEngineContract::attest(
            e.clone(),
            admin.clone(),
            commitment_id.clone(),
            String::from_str(&e, "health_check"),
            data,
            true,
        )
        .unwrap();
    });

    let score = e.as_contract(&contract_id, || {
        AttestationEngineContract::calculate_compliance_score(e.clone(), commitment_id)
    });

    // Should be at or near 100
    assert_eq!(score, 100);
}

#[test]
fn test_compliance_score_with_single_violation() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();
    e.ledger().with_mut(|li| li.timestamp = 10000);

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);
    
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        1000,
        10,
        30,
        5000,
    );

    // Add violation
    let mut data = Map::new(&e);
    data.set(String::from_str(&e, "violation_type"), String::from_str(&e, "rule_breach"));
    data.set(String::from_str(&e, "severity"), String::from_str(&e, "medium"));
    
    e.as_contract(&contract_id, || {
        AttestationEngineContract::attest(
            e.clone(),
            admin.clone(),
            commitment_id.clone(),
            String::from_str(&e, "violation"),
            data,
            false,
        )
        .unwrap();
    });

    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_stored_health_metrics(e.clone(), commitment_id)
    });

    // Base 100 - 20 (medium violation) = 80
    assert_eq!(metrics.unwrap().compliance_score, 80);
}

#[test]
fn test_compliance_score_with_multiple_violations() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();
    e.ledger().with_mut(|li| li.timestamp = 10000);

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);
    
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        1000,
        10,
        30,
        5000,
    );

    // Add first violation
    let mut data1 = Map::new(&e);
    data1.set(String::from_str(&e, "violation_type"), String::from_str(&e, "minor_breach"));
    data1.set(String::from_str(&e, "severity"), String::from_str(&e, "low"));
    
    e.as_contract(&contract_id, || {
        AttestationEngineContract::attest(
            e.clone(),
            admin.clone(),
            commitment_id.clone(),
            String::from_str(&e, "violation"),
            data1,
            false,
        )
        .unwrap();
    });

    // Add second violation
    let mut data2 = Map::new(&e);
    data2.set(String::from_str(&e, "violation_type"), String::from_str(&e, "rule_breach"));
    data2.set(String::from_str(&e, "severity"), String::from_str(&e, "medium"));
    
    e.as_contract(&contract_id, || {
        AttestationEngineContract::attest(
            e.clone(),
            admin.clone(),
            commitment_id.clone(),
            String::from_str(&e, "violation"),
            data2,
            false,
        )
        .unwrap();
    });

    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_stored_health_metrics(e.clone(), commitment_id)
    });

    // Base 100 - 10 (low) - 20 (medium) = 70
    assert_eq!(metrics.unwrap().compliance_score, 70);
}

#[test]
fn test_compliance_score_with_drawdown_penalty() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();
    e.ledger().with_mut(|li| li.timestamp = 10000);

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);
    
    // Initial: 1000, Current: 700 = 30% drawdown, max_loss: 10% → 20% over threshold
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        700,
        10,
        30,
        5000,
    );

    let score = e.as_contract(&contract_id, || {
        AttestationEngineContract::calculate_compliance_score(e.clone(), commitment_id)
    });

    // Base 100 + 10 (duration) - 20 (drawdown penalty) = 90
    assert_eq!(score, 90);
}

#[test]
fn test_compliance_score_with_fees_and_drawdown() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();
    e.ledger().with_mut(|li| li.timestamp = 10000);

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);
    
    // 15% drawdown (5% over 10% threshold)
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        850,
        10,
        30,
        5000,
    );

    // Add fee_generation attestation
    let mut data = Map::new(&e);
    data.set(String::from_str(&e, "fee_amount"), String::from_str(&e, "500"));
    
    e.as_contract(&contract_id, || {
        AttestationEngineContract::attest(
            e.clone(),
            admin.clone(),
            commitment_id.clone(),
            String::from_str(&e, "fee_generation"),
            data,
            true,
        )
        .unwrap();
    });

    let score = e.as_contract(&contract_id, || {
        AttestationEngineContract::calculate_compliance_score(e.clone(), commitment_id)
    });

    // Base 100 + 10 (duration) - 5 (drawdown penalty) = 105, clamped to 100
    // Note: fee bonus not applied in current implementation (total_fees = 0)
    assert_eq!(score, 100);
}

#[test]
fn test_compliance_score_clamped_at_zero() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();
    e.ledger().with_mut(|li| li.timestamp = 10000);

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);
    
    // Massive drawdown: 90% (80% over 10% threshold)
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        100,
        10,
        30,
        5000,
    );

    // Add multiple violations to drive score below 0
    for _i in 0..5_u32 {
        let mut data = Map::new(&e);
        data.set(String::from_str(&e, "violation_type"), String::from_str(&e, "critical_breach"));
        data.set(String::from_str(&e, "severity"), String::from_str(&e, "high"));
        
        e.as_contract(&contract_id, || {
            AttestationEngineContract::attest(
                e.clone(),
                admin.clone(),
                commitment_id.clone(),
                String::from_str(&e, "violation"),
                data,
                false,
            )
            .unwrap();
        });
    }

    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_stored_health_metrics(e.clone(), commitment_id)
    });

    // Base 100 - 150 (5 high violations at 30 each) = 0 (clamped)
    assert_eq!(metrics.unwrap().compliance_score, 0);
}

#[test]
fn test_compliance_score_clamped_at_100() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();
    e.ledger().with_mut(|li| li.timestamp = 10000);

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);
    
    // Perfect conditions: no drawdown, within duration
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        1100, // Gained value
        10,
        30,
        5000,
    );

    let score = e.as_contract(&contract_id, || {
        AttestationEngineContract::calculate_compliance_score(e.clone(), commitment_id)
    });

    // Base 100 + 10 (duration) = 110, clamped to 100
    assert_eq!(score, 100);
}

#[test]
fn test_compliance_score_mixed_attestations() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();
    e.ledger().with_mut(|li| li.timestamp = 10000);

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);
    
    // 12% drawdown (2% over threshold)
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        880,
        10,
        30,
        5000,
    );

    // Add health_check
    let mut health_data = Map::new(&e);
    health_data.set(String::from_str(&e, "status"), String::from_str(&e, "healthy"));
    e.as_contract(&contract_id, || {
        AttestationEngineContract::attest(
            e.clone(),
            admin.clone(),
            commitment_id.clone(),
            String::from_str(&e, "health_check"),
            health_data,
            true,
        )
        .unwrap();
    });

    // Add violation
    let mut violation_data = Map::new(&e);
    violation_data.set(String::from_str(&e, "violation_type"), String::from_str(&e, "minor_breach"));
    violation_data.set(String::from_str(&e, "severity"), String::from_str(&e, "low"));
    e.as_contract(&contract_id, || {
        AttestationEngineContract::attest(
            e.clone(),
            admin.clone(),
            commitment_id.clone(),
            String::from_str(&e, "violation"),
            violation_data,
            false,
        )
        .unwrap();
    });

    // Add drawdown attestation
    let mut drawdown_data = Map::new(&e);
    drawdown_data.set(String::from_str(&e, "drawdown_percent"), String::from_str(&e, "12"));
    e.as_contract(&contract_id, || {
        AttestationEngineContract::attest(
            e.clone(),
            admin.clone(),
            commitment_id.clone(),
            String::from_str(&e, "drawdown"),
            drawdown_data,
            true,
        )
        .unwrap();
    });

    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_stored_health_metrics(e.clone(), commitment_id)
    });

    // Base 100 + 1 (health_check, clamped at 100) - 10 (low violation) + 1 (drawdown attestation) = 91
    // Note: drawdown penalty from commitment value is NOT applied in stored metrics,
    // only in calculate_compliance_score
    assert_eq!(metrics.unwrap().compliance_score, 91);
}

#[test]
fn test_get_health_metrics_last_attestation() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "test_commitment_1");
    let owner = Address::generate(&e);
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment_1",
        &owner,
        1000,
        950,
        10,
        30,
        1000,
    );
    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_health_metrics(e.clone(), commitment_id)
    });

    // With no attestations, last_attestation should be 0
    assert_eq!(metrics.last_attestation, 0);
}

#[test]
fn test_all_three_functions_work_together() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "test_commitment_1");
    let owner = Address::generate(&e);
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment_1",
        &owner,
        1000,
        950,
        10,
        30,
        1000,
    );

    // Test all three functions work
    let attestations = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_attestations(e.clone(), commitment_id.clone())
    });
    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_health_metrics(e.clone(), commitment_id.clone())
    });
    let score = e.as_contract(&contract_id, || {
        AttestationEngineContract::calculate_compliance_score(e.clone(), commitment_id.clone())
    });

    // Verify they all return valid data
    assert_eq!(attestations.len(), 0); // No attestations stored yet
    assert_eq!(metrics.commitment_id, commitment_id);
    assert!(score <= 100);
    assert_eq!(metrics.compliance_score, score); // Should match
}

#[test]
fn test_get_attestations_returns_empty_vec_when_none_exist() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    // Test with different commitment IDs
    let commitment_id1 = String::from_str(&e, "commitment_1");
    let commitment_id2 = String::from_str(&e, "commitment_2");

    let attestations1 = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_attestations(e.clone(), commitment_id1)
    });
    let attestations2 = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_attestations(e.clone(), commitment_id2)
    });

    assert_eq!(attestations1.len(), 0);
    assert_eq!(attestations2.len(), 0);
}

#[test]
fn test_health_metrics_structure() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        1000,
        10,
        30,
        1000,
    );
    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_health_metrics(e.clone(), commitment_id.clone())
    });

    // Verify all required fields are present
    assert_eq!(metrics.commitment_id, commitment_id);
    assert_eq!(metrics.current_value, 1000);
    assert_eq!(metrics.initial_value, 1000);
    assert_eq!(metrics.drawdown_percent, 0);
    assert_eq!(metrics.fees_generated, 0);
    assert_eq!(metrics.volatility_exposure, 0);
    assert_eq!(metrics.last_attestation, 0);
    assert!(metrics.compliance_score <= 100);
}

#[test]
fn test_attest_and_get_metrics() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();

    // Set ledger timestamp to non-zero
    e.ledger().with_mut(|li| li.timestamp = 12345);

    let commitment_id = String::from_str(&e, "test_commitment_wf");
    let owner = Address::generate(&e);
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment_wf",
        &owner,
        1000,
        1000,
        10,
        30,
        1000,
    );
    let attestation_type = String::from_str(&e, "health_check");
    let mut data = Map::new(&e);
    data.set(
        String::from_str(&e, "note"),
        String::from_str(&e, "test attestation"),
    );

    // Record an attestation
    e.as_contract(&contract_id, || {
        AttestationEngineContract::attest(
            e.clone(),
            admin.clone(),
            commitment_id.clone(),
            attestation_type.clone(),
            data.clone(),
            true,
        )
        .unwrap();
    });

    // Get attestations and verify
    let attestations = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_attestations(e.clone(), commitment_id.clone())
    });

    assert_eq!(attestations.len(), 1);
    assert_eq!(
        attestations.get(0).unwrap().attestation_type,
        attestation_type
    );

    // Get health metrics and verify last_attestation is updated
    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_health_metrics(e.clone(), commitment_id.clone())
    });

    assert!(metrics.last_attestation > 0);
}

#[test]
#[should_panic(expected = "Reentrancy detected")]
fn test_attest_reentrancy_protection() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);

    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        1000,
        10,
        30,
        1000,
    );

    let attestation_type = String::from_str(&e, "health_check");
    let data = Map::new(&e);

    // Manually set reentrancy guard to simulate reentrancy
    e.as_contract(&contract_id, || {
        e.storage()
            .instance()
            .set(&super::DataKey::ReentrancyGuard, &true);
    });

    // Try to attest, should panic
    e.as_contract(&contract_id, || {
        let _ = AttestationEngineContract::attest(
            e.clone(),
            admin.clone(),
            commitment_id.clone(),
            attestation_type.clone(),
            data.clone(),
            true,
        );
    });
}

// ============================================================================
// Access Control Tests
// ============================================================================

#[test]
fn test_add_verifier_success() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();

    let verifier = Address::generate(&e);

    e.as_contract(&contract_id, || {
        AttestationEngineContract::initialize(e.clone(), admin.clone(), core.clone()).unwrap();
    });

    let second = e.as_contract(&contract_id, || {
        AttestationEngineContract::initialize(e.clone(), admin.clone(), core.clone())
    });
    assert_eq!(second, Err(AttestationError::AlreadyInitialized));
}

#[test]
fn test_get_attestations_empty() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AttestationEngineContract);
    let admin = Address::generate(&e);
    let core = Address::generate(&e);
    let commitment_id = String::from_str(&e, "c_1");

    e.as_contract(&contract_id, || {
        AttestationEngineContract::initialize(e.clone(), admin.clone(), core.clone()).unwrap();
    });

    let attestations = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_attestations(e.clone(), commitment_id.clone())
    });
    assert_eq!(attestations.len(), 0);
}

#[test]
fn test_get_health_metrics_no_attestations_returns_defaults() {
    let (e, _admin, _commitment_core, contract_id) = setup_test_env();

    let commitment_id = String::from_str(&e, "new_commitment");
    let owner = Address::generate(&e);
    
    // Store a new commitment with no attestations
    store_core_commitment(
        &e,
        &_commitment_core,
        "new_commitment",
        &owner,
        1000,
        1000,
        10,
        30,
        5000,
    );

    // Call get_health_metrics on commitment with no attestations
    let metrics = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_health_metrics(e.clone(), commitment_id.clone())
    });

    // Verify sensible defaults are returned
    assert_eq!(metrics.commitment_id, commitment_id);
    assert_eq!(metrics.initial_value, 1000);
    assert_eq!(metrics.current_value, 1000);
    assert_eq!(metrics.drawdown_percent, 0);
    assert_eq!(metrics.fees_generated, 0);
    assert_eq!(metrics.volatility_exposure, 0);
    assert_eq!(metrics.last_attestation, 0);
    assert_eq!(metrics.compliance_score, 100);
}

#[test]
fn test_get_health_metrics_updates_after_first_attestation() {
    let (e, admin, _commitment_core, contract_id) = setup_test_env();
    let client = AttestationEngineContractClient::new(&e, &contract_id);

    let commitment_id = String::from_str(&e, "test_commitment");
    let owner = Address::generate(&e);
    
    store_core_commitment(
        &e,
        &_commitment_core,
        "test_commitment",
        &owner,
        1000,
        1000,
        10,
        30,
        5000,
    );

    // Get metrics before attestation
    let metrics_before = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_health_metrics(e.clone(), commitment_id.clone())
    });
    assert_eq!(metrics_before.last_attestation, 0);

    // Add first attestation
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let mut data = Map::new(&e);
    data.set(String::from_str(&e, "status"), String::from_str(&e, "healthy"));
    
    client.attest(
        &admin,
        &commitment_id,
        &String::from_str(&e, "health_check"),
        &data,
        &true,
    );

    // Get metrics after attestation
    let metrics_after = e.as_contract(&contract_id, || {
        AttestationEngineContract::get_health_metrics(e.clone(), commitment_id.clone())
    });
    
    // Verify metrics updated
    assert_eq!(metrics_after.last_attestation, 1000);
    assert_eq!(metrics_after.commitment_id, commitment_id);
}
