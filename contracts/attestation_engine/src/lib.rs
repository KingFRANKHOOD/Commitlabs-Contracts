#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Vec, Map, Symbol, symbol_short};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attestation {
    pub commitment_id: String,
    pub timestamp: u64,
    pub attestation_type: String, // "health_check", "violation", "fee_generation", "drawdown"
    pub data: Map<String, String>, // Flexible data structure
    pub is_compliant: bool,
    pub verified_by: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthMetrics {
    pub commitment_id: String,
    pub current_value: i128,
    pub initial_value: i128,
    pub drawdown_percent: i128,
    pub fees_generated: i128,
    pub volatility_exposure: i128,
    pub last_attestation: u64,
    pub compliance_score: u32, // 0-100
}

// Storage keys
const ADMIN: Symbol = symbol_short!("ADMIN");
const COMMITMENT_CORE: Symbol = symbol_short!("CORE");
const ATTESTATIONS: Symbol = symbol_short!("ATTEST");
const HEALTH_METRICS: Symbol = symbol_short!("HEALTH");

#[contract]
pub struct AttestationEngineContract;

#[contractimpl]
impl AttestationEngineContract {
    /// Initialize the attestation engine
    pub fn initialize(e: Env, admin: Address, commitment_core: Address) {
        if e.storage().instance().has(&ADMIN) {
            panic!("already initialized");
        }
        e.storage().instance().set(&ADMIN, &admin);
        e.storage().instance().set(&COMMITMENT_CORE, &commitment_core);
    }

    /// Record an attestation for a commitment
    pub fn attest(
        e: Env,
        commitment_id: String,
        attestation_type: String,
        data: Map<String, String>,
        verified_by: Address,
    ) {
        // Verify caller is authorized (admin or verified_by)
        verified_by.require_auth();
        let admin: Address = e.storage().instance().get(&ADMIN).unwrap();
        if verified_by != admin {
            // In a real implementation, you might check if verified_by is an authorized verifier
        }

        // Create attestation record
        let timestamp = e.ledger().timestamp();
        let is_compliant = true; // Default, can be calculated from data
        let attestation = Attestation {
            commitment_id: commitment_id.clone(),
            timestamp,
            attestation_type: attestation_type.clone(),
            data: data.clone(),
            is_compliant,
            verified_by: verified_by.clone(),
        };

        // Store attestation
        let mut attestations: Vec<Attestation> = e.storage().persistent()
            .get(&(ATTESTATIONS, commitment_id.clone()))
            .unwrap_or_else(|| Vec::new(&e));
        attestations.push_back(attestation.clone());
        e.storage().persistent().set(&(ATTESTATIONS, commitment_id.clone()), &attestations);

        // Update health metrics
        Self::update_health_metrics(&e, &commitment_id);

        // Emit attestation event
        e.events().publish((symbol_short!("attest"), commitment_id), (attestation_type, verified_by, timestamp));
    }

    /// Get all attestations for a commitment
    pub fn get_attestations(e: Env, commitment_id: String) -> Vec<Attestation> {
        e.storage().persistent()
            .get(&(ATTESTATIONS, commitment_id))
            .unwrap_or_else(|| Vec::new(&e))
    }

    /// Get current health metrics for a commitment
    pub fn get_health_metrics(e: Env, commitment_id: String) -> HealthMetrics {
        let id_clone = commitment_id.clone();
        e.storage().persistent()
            .get(&(HEALTH_METRICS, commitment_id))
            .unwrap_or_else(|| {
                HealthMetrics {
                    commitment_id: id_clone,
                    current_value: 0,
                    initial_value: 0,
                    drawdown_percent: 0,
                    fees_generated: 0,
                    volatility_exposure: 0,
                    last_attestation: 0,
                    compliance_score: 0,
                }
            })
    }

    /// Verify commitment compliance
    pub fn verify_compliance(e: Env, commitment_id: String) -> bool {
        let metrics = Self::get_health_metrics(e.clone(), commitment_id.clone());
        let attestations = Self::get_attestations(e.clone(), commitment_id.clone());

        // Check if any attestations indicate non-compliance
        for att in attestations.iter() {
            if !att.is_compliant {
                return false;
            }
        }

        // Check drawdown limits
        if metrics.drawdown_percent > 100 {
            return false;
        }

        true
    }

    /// Record fee generation
    pub fn record_fees(e: Env, commitment_id: String, fee_amount: i128) {
        if fee_amount <= 0 {
            panic!("fee_amount must be positive");
        }

        // Update fees_generated in health metrics
        let mut metrics = Self::get_health_metrics(e.clone(), commitment_id.clone());
        metrics.fees_generated += fee_amount;
        metrics.last_attestation = e.ledger().timestamp();
        e.storage().persistent().set(&(HEALTH_METRICS, commitment_id.clone()), &metrics);

        // Create fee attestation
        let mut data = Map::new(&e);
        // Store fee amount as a simple representation
        // In no_std, we simplify by using a fixed string format
        data.set(String::from_str(&e, "fee_amount"), String::from_str(&e, "fee"));
        Self::attest(e.clone(), commitment_id.clone(), String::from_str(&e, "fee_generation"), data, e.storage().instance().get(&ADMIN).unwrap());

        // Emit fee event
        e.events().publish((symbol_short!("fees"), commitment_id), (fee_amount, metrics.fees_generated));
    }

    /// Record drawdown event
    pub fn record_drawdown(e: Env, commitment_id: String, drawdown_percent: i128) {
        // Update drawdown_percent in health metrics
        let mut metrics = Self::get_health_metrics(e.clone(), commitment_id.clone());
        metrics.drawdown_percent = drawdown_percent;
        metrics.last_attestation = e.ledger().timestamp();
        e.storage().persistent().set(&(HEALTH_METRICS, commitment_id.clone()), &metrics);

        // Create drawdown attestation
        let mut data = Map::new(&e);
        // Store drawdown as a simple representation
        data.set(String::from_str(&e, "drawdown"), String::from_str(&e, "drawdown"));
        let is_compliant = drawdown_percent <= 100;
        Self::attest(e.clone(), commitment_id.clone(), String::from_str(&e, "drawdown"), data, e.storage().instance().get(&ADMIN).unwrap());

        // Emit drawdown event
        e.events().publish((symbol_short!("drawdown"), commitment_id), (drawdown_percent, is_compliant));
    }

    /// Calculate compliance score (0-100)
    pub fn calculate_compliance_score(e: Env, commitment_id: String) -> u32 {
        let metrics = Self::get_health_metrics(e.clone(), commitment_id.clone());
        let attestations = Self::get_attestations(e.clone(), commitment_id.clone());
        let mut score = 100u32;

        // Deduct points for non-compliant attestations
        let mut violations = 0;
        for att in attestations.iter() {
            if !att.is_compliant {
                violations += 1;
            }
        }
        if violations > 0 {
            score = score.saturating_sub(violations * 10);
        }

        // Deduct points for excessive drawdown
        if metrics.drawdown_percent > 50 {
            score = score.saturating_sub(20);
        } else if metrics.drawdown_percent > 25 {
            score = score.saturating_sub(10);
        }

        // Store updated score
        let mut updated_metrics = metrics;
        updated_metrics.compliance_score = score;
        e.storage().persistent().set(&(HEALTH_METRICS, commitment_id), &updated_metrics);

        score
    }

    /// Helper function to update health metrics
    fn update_health_metrics(e: &Env, commitment_id: &String) {
        let attestations = Self::get_attestations(e.clone(), commitment_id.clone());
        let timestamp = e.ledger().timestamp();

        let mut metrics = e.storage().persistent()
            .get(&(HEALTH_METRICS, commitment_id.clone()))
            .unwrap_or_else(|| {
                HealthMetrics {
                    commitment_id: commitment_id.clone(),
                    current_value: 0,
                    initial_value: 0,
                    drawdown_percent: 0,
                    fees_generated: 0,
                    volatility_exposure: 0,
                    last_attestation: timestamp,
                    compliance_score: 100,
                }
            });

        metrics.last_attestation = timestamp;
        e.storage().persistent().set(&(HEALTH_METRICS, commitment_id.clone()), &metrics);
    }
}

#[cfg(test)]
mod tests;
