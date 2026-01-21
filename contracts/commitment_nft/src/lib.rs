#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol, symbol_short};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitmentMetadata {
    pub commitment_id: String,
    pub duration_days: u32,
    pub max_loss_percent: u32,
    pub commitment_type: String, // "safe", "balanced", "aggressive"
    pub created_at: u64,
    pub expires_at: u64,
    pub initial_amount: i128,
    pub asset_address: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitmentNFT {
    pub owner: Address,
    pub token_id: u32,
    pub metadata: CommitmentMetadata,
    pub is_active: bool,
    pub early_exit_penalty: u32,
}

// Storage keys
const ADMIN: Symbol = symbol_short!("ADMIN");
const TOKEN_COUNTER: Symbol = symbol_short!("TOKEN_CNT");
const OWNER: Symbol = symbol_short!("OWNER");
const NFT_DATA: Symbol = symbol_short!("NFT_DATA");
const IS_ACTIVE: Symbol = symbol_short!("IS_ACTIVE");

#[contract]
pub struct CommitmentNFTContract;

#[contractimpl]
impl CommitmentNFTContract {
    /// Initialize the NFT contract
    pub fn initialize(e: Env, admin: Address) {
        if e.storage().instance().has(&ADMIN) {
            panic!("already initialized");
        }
        e.storage().instance().set(&ADMIN, &admin);
        e.storage().instance().set(&TOKEN_COUNTER, &0u32);
    }

    /// Mint a new Commitment NFT
    pub fn mint(
        e: Env,
        owner: Address,
        commitment_id: String,
        duration_days: u32,
        max_loss_percent: u32,
        commitment_type: String,
        initial_amount: i128,
        asset_address: Address,
    ) -> u32 {
        // Verify caller is admin
        let admin: Address = e.storage().instance().get(&ADMIN).unwrap();
        admin.require_auth();

        // Generate unique token_id
        let mut counter: u32 = e.storage().instance().get(&TOKEN_COUNTER).unwrap_or(0);
        counter += 1;
        e.storage().instance().set(&TOKEN_COUNTER, &counter);
        let token_id = counter;

        // Calculate expires_at from duration_days
        let created_at = e.ledger().timestamp();
        let expires_at = created_at + (duration_days as u64 * 86400);

        // Create CommitmentMetadata
        let metadata = CommitmentMetadata {
            commitment_id: commitment_id.clone(),
            duration_days,
            max_loss_percent,
            commitment_type,
            created_at,
            expires_at,
            initial_amount,
            asset_address,
        };

        // Store NFT data
        let nft = CommitmentNFT {
            owner: owner.clone(),
            token_id,
            metadata: metadata.clone(),
            is_active: true,
            early_exit_penalty: 0,
        };
        
        e.storage().persistent().set(&(NFT_DATA, token_id), &nft);
        e.storage().persistent().set(&(OWNER, token_id), &owner);
        e.storage().persistent().set(&(IS_ACTIVE, token_id), &true);

        // Emit mint event
        e.events().publish((symbol_short!("mint"), token_id), (owner, metadata));

        token_id
    }

    /// Get NFT metadata by token_id
    pub fn get_metadata(e: Env, token_id: u32) -> CommitmentMetadata {
        let nft: CommitmentNFT = e.storage().persistent().get(&(NFT_DATA, token_id))
            .unwrap_or_else(|| panic!("NFT not found"));
        nft.metadata
    }

    /// Get owner of NFT
    pub fn owner_of(e: Env, token_id: u32) -> Address {
        e.storage().persistent().get(&(OWNER, token_id))
            .unwrap_or_else(|| panic!("NFT not found"))
    }

    /// Transfer NFT to new owner
    pub fn transfer(e: Env, from: Address, to: Address, token_id: u32) {
        // Verify ownership
        let owner: Address = e.storage().persistent().get(&(OWNER, token_id))
            .unwrap_or_else(|| panic!("NFT not found"));
        if owner != from {
            panic!("not owner");
        }
        from.require_auth();

        // Check if transfer is allowed (not locked/active)
        let is_active: bool = e.storage().persistent().get(&(IS_ACTIVE, token_id))
            .unwrap_or(false);
        if !is_active {
            panic!("NFT is not active");
        }

        // Update owner
        e.storage().persistent().set(&(OWNER, token_id), &to);
        let mut nft: CommitmentNFT = e.storage().persistent().get(&(NFT_DATA, token_id)).unwrap();
        nft.owner = to.clone();
        e.storage().persistent().set(&(NFT_DATA, token_id), &nft);

        // Emit transfer event
        e.events().publish((symbol_short!("transfer"), token_id), (from, to));
    }

    /// Check if NFT is active
    pub fn is_active(e: Env, token_id: u32) -> bool {
        e.storage().persistent().get(&(IS_ACTIVE, token_id))
            .unwrap_or(false)
    }

    /// Mark NFT as settled (after maturity)
    pub fn settle(e: Env, token_id: u32) {
        let admin: Address = e.storage().instance().get(&ADMIN).unwrap();
        admin.require_auth();

        let mut nft: CommitmentNFT = e.storage().persistent().get(&(NFT_DATA, token_id))
            .unwrap_or_else(|| panic!("NFT not found"));

        // Verify expiration
        if e.ledger().timestamp() < nft.metadata.expires_at {
            panic!("commitment not expired");
        }

        // Mark as inactive
        nft.is_active = false;
        e.storage().persistent().set(&(IS_ACTIVE, token_id), &false);
        e.storage().persistent().set(&(NFT_DATA, token_id), &nft);

        // Emit settle event
        e.events().publish((symbol_short!("settle"), token_id), ());
    }
}

#[cfg(test)]
mod tests;
