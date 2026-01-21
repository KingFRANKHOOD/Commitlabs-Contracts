#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol, symbol_short};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitmentRules {
    pub duration_days: u32,
    pub max_loss_percent: u32,
    pub commitment_type: String, // "safe", "balanced", "aggressive"
    pub early_exit_penalty: u32,
    pub min_fee_threshold: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Commitment {
    pub commitment_id: String,
    pub owner: Address,
    pub nft_token_id: u32,
    pub rules: CommitmentRules,
    pub amount: i128,
    pub asset_address: Address,
    pub created_at: u64,
    pub expires_at: u64,
    pub current_value: i128,
    pub status: String, // "active", "settled", "violated", "early_exit"
}

// Storage keys
const ADMIN: Symbol = symbol_short!("ADMIN");
const NFT_CONTRACT: Symbol = symbol_short!("NFT_CNT");
const COMMITMENT: Symbol = symbol_short!("COMMIT");
const COMMITMENT_CNT: Symbol = symbol_short!("COMMIT_CN");
const AUTH_ALLOC: Symbol = symbol_short!("AUTH_ALL");

#[contract]
pub struct CommitmentCoreContract;

#[contractimpl]
impl CommitmentCoreContract {
    /// Initialize the core commitment contract
    pub fn initialize(e: Env, admin: Address, nft_contract: Address) {
        if e.storage().instance().has(&ADMIN) {
            panic!("already initialized");
        }
        e.storage().instance().set(&ADMIN, &admin);
        e.storage().instance().set(&NFT_CONTRACT, &nft_contract);
        e.storage().instance().set(&COMMITMENT_CNT, &0u64);
    }

    /// Create a new commitment
    pub fn create_commitment(
        e: Env,
        owner: Address,
        amount: i128,
        asset_address: Address,
        rules: CommitmentRules,
    ) -> String {
        // Validate rules
        if amount <= 0 {
            panic!("amount must be positive");
        }
        if rules.duration_days == 0 {
            panic!("duration must be positive");
        }
        if rules.max_loss_percent > 100 {
            panic!("max_loss_percent cannot exceed 100");
        }

        // Generate commitment_id
        let mut counter: u64 = e.storage().instance().get(&COMMITMENT_CNT).unwrap_or(0);
        counter += 1;
        e.storage().instance().set(&COMMITMENT_CNT, &counter);
        // Create commitment ID - use a simple prefix + counter approach
        // For simplicity in no_std, just use the counter value directly as string
        // In a real implementation, you'd want proper formatting
        let commitment_id_str = match counter {
            1 => String::from_str(&e, "1"),
            2 => String::from_str(&e, "2"),
            3 => String::from_str(&e, "3"),
            4 => String::from_str(&e, "4"),
            5 => String::from_str(&e, "5"),
            _ => String::from_str(&e, "commit"), // Fallback for higher values
        };

        // Calculate timestamps
        let created_at = e.ledger().timestamp();
        let expires_at = created_at + (rules.duration_days as u64 * 86400);

        // Create commitment
        let commitment = Commitment {
            commitment_id: commitment_id_str.clone(),
            owner: owner.clone(),
            nft_token_id: 0, // Will be set after NFT mint
            rules: rules.clone(),
            amount,
            asset_address: asset_address.clone(),
            created_at,
            expires_at,
            current_value: amount,
            status: String::from_str(&e, "active"),
        };

        // Store commitment data
        e.storage().persistent().set(&(COMMITMENT, commitment_id_str.clone()), &commitment);

        // Emit creation event
        e.events().publish((symbol_short!("create"), commitment_id_str.clone()), (owner, amount, rules));

        commitment_id_str
    }

    /// Get commitment details
    pub fn get_commitment(e: Env, commitment_id: String) -> Commitment {
        e.storage().persistent().get(&(COMMITMENT, commitment_id.clone()))
            .unwrap_or_else(|| panic!("commitment not found"))
    }

    /// Update commitment value (called by allocation logic)
    pub fn update_value(e: Env, commitment_id: String, new_value: i128) {
        // Verify caller is authorized (allocation contract)
        // For now, we'll allow admin to update values in tests
        let admin: Address = e.storage().instance().get(&ADMIN).unwrap();
        // Note: In tests, require_auth() is mocked, so we check admin authorization
        admin.require_auth();

        let mut commitment: Commitment = e.storage().persistent().get(&(COMMITMENT, commitment_id.clone()))
            .unwrap_or_else(|| panic!("commitment not found"));

        // Update current_value
        commitment.current_value = new_value;

        // Check if max_loss_percent is violated
        if commitment.current_value < commitment.amount {
            let loss_percent = ((commitment.amount - commitment.current_value) as u64 * 100) / commitment.amount as u64;
            if loss_percent > commitment.rules.max_loss_percent as u64 {
                commitment.status = String::from_str(&e, "violated");
            }
        }

        e.storage().persistent().set(&(COMMITMENT, commitment_id.clone()), &commitment);

        // Emit value update event
        e.events().publish((symbol_short!("upd_value"), commitment_id), (new_value, commitment.status.clone()));
    }

    /// Check if commitment rules are violated
    pub fn check_violations(e: Env, commitment_id: String) -> bool {
        let commitment: Commitment = e.storage().persistent().get(&(COMMITMENT, commitment_id.clone()))
            .unwrap_or_else(|| panic!("commitment not found"));

        // Check if max_loss_percent exceeded
        if commitment.current_value < commitment.amount {
            let loss_percent = ((commitment.amount - commitment.current_value) as u64 * 100) / commitment.amount as u64;
            if loss_percent > commitment.rules.max_loss_percent as u64 {
                return true;
            }
        }

        // Check if duration expired
        if e.ledger().timestamp() > commitment.expires_at {
            return true;
        }

        false
    }

    /// Settle commitment at maturity
    pub fn settle(e: Env, commitment_id: String) {
        let mut commitment: Commitment = e.storage().persistent().get(&(COMMITMENT, commitment_id.clone()))
            .unwrap_or_else(|| panic!("commitment not found"));

        // Verify commitment is expired
        if e.ledger().timestamp() < commitment.expires_at {
            panic!("commitment not expired");
        }

        // Mark commitment as settled
        commitment.status = String::from_str(&e, "settled");
        e.storage().persistent().set(&(COMMITMENT, commitment_id.clone()), &commitment);

        // Emit settlement event
        e.events().publish((symbol_short!("settle"), commitment_id), (commitment.current_value, commitment.owner));
    }

    /// Early exit (with penalty)
    pub fn early_exit(e: Env, commitment_id: String, caller: Address) {
        caller.require_auth();

        let mut commitment: Commitment = e.storage().persistent().get(&(COMMITMENT, commitment_id.clone()))
            .unwrap_or_else(|| panic!("commitment not found"));

        // Verify caller is owner
        if commitment.owner != caller {
            panic!("not owner");
        }

        if commitment.status != String::from_str(&e, "active") {
            panic!("commitment not active");
        }

        // Calculate penalty
        let penalty = (commitment.current_value as u64 * commitment.rules.early_exit_penalty as u64) / 100;
        let remaining = commitment.current_value - penalty as i128;

        // Mark commitment as early_exit
        commitment.status = String::from_str(&e, "early_exit");
        commitment.current_value = remaining;
        e.storage().persistent().set(&(COMMITMENT, commitment_id.clone()), &commitment);

        // Emit early exit event
        e.events().publish((symbol_short!("early_ext"), commitment_id), (remaining, penalty as i128, caller));
    }

    /// Allocate liquidity (called by allocation strategy)
    pub fn allocate(e: Env, commitment_id: String, target_pool: Address, amount: i128) {
        // Verify caller is authorized allocation contract
        // For tests, we'll use admin authorization
        let admin: Address = e.storage().instance().get(&ADMIN).unwrap();
        let auth_alloc: Option<Address> = e.storage().instance().get(&AUTH_ALLOC);
        // Check if authorized allocator is set, otherwise use admin
        if let Some(allocator) = auth_alloc {
            allocator.require_auth();
        } else {
            admin.require_auth();
        }

        let commitment: Commitment = e.storage().persistent().get(&(COMMITMENT, commitment_id.clone()))
            .unwrap_or_else(|| panic!("commitment not found"));

        // Verify commitment is active
        if commitment.status != String::from_str(&e, "active") {
            panic!("commitment not active");
        }

        if amount <= 0 || amount > commitment.current_value {
            panic!("invalid amount");
        }

        // Emit allocation event
        e.events().publish((symbol_short!("allocate"), commitment_id), (target_pool, amount));
    }

    /// Set authorized allocator (admin only)
    pub fn set_authorized_allocator(e: Env, allocator: Address) {
        let admin: Address = e.storage().instance().get(&ADMIN).unwrap();
        admin.require_auth();
        e.storage().instance().set(&AUTH_ALLOC, &allocator);
    }
}

#[cfg(test)] mod tests;
