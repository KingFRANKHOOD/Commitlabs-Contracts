#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, contracterror, symbol_short, Address, Env, String, Vec, Symbol};
use shared_utils::{BatchMode, BatchResultVoid, BatchError, BatchProcessor};

// ============================================================================
// Error Types
// ============================================================================

/// Contract errors for structured error handling
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    /// Contract has not been initialized
    NotInitialized = 1,
    /// Contract has already been initialized
    AlreadyInitialized = 2,
    /// NFT with the given token_id does not exist
    TokenNotFound = 3,
    /// Invalid token_id
    InvalidTokenId = 4,
    /// Caller is not the owner of the NFT
    NotOwner = 5,
    /// Caller is not authorized to perform this action
    NotAuthorized = 6,
    /// Transfer is not allowed (e.g. restricted)
    TransferNotAllowed = 7,
    /// NFT has already been settled
    AlreadySettled = 8,
    /// Commitment has not expired yet
    NotExpired = 9,
    /// Invalid duration (must be > 0)
    InvalidDuration = 10,
    /// Invalid max loss percent (must be 0-100)
    InvalidMaxLoss = 11,
    /// Invalid commitment type (must be safe, balanced, or aggressive)
    InvalidCommitmentType = 12,
    /// Invalid amount (must be > 0)
    InvalidAmount = 13,
    /// Reentrancy detected
    ReentrancyDetected = 14,
}

// ============================================================================
// Data Types
// ============================================================================

/// Metadata associated with a commitment NFT
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

/// The Commitment NFT structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitmentNFT {
    pub owner: Address,
    pub token_id: u32,
    pub metadata: CommitmentMetadata,
    pub is_active: bool,
    pub early_exit_penalty: u32,
}

/// Parameters for batch NFT transfer operations
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferParams {
    pub from: Address,
    pub to: Address,
    pub token_id: u32,
}

/// Storage keys for the contract
#[contracttype]
pub enum DataKey {
    /// Admin address (singleton)
    Admin,
    /// Counter for generating unique token IDs / Total supply
    TokenCounter,
    /// NFT data storage (token_id -> CommitmentNFT)
    NFT(u32),
    /// Owner balance count (Address -> u32)
    OwnerBalance(Address),
    /// Owner tokens list (Address -> Vec<u32>)
    OwnerTokens(Address),
    /// List of all token IDs (Vec<u32>)
    TokenIds,
    /// Authorized commitment_core contract address (for settlement)
    CoreContract,
    /// Authorized minter addresses (from upstream)
    AuthorizedMinter(Address),
    /// Active status (token_id -> bool)
    ActiveStatus(u32),
    /// Reentrancy guard flag
    ReentrancyGuard,
}

// Events
const MINT: soroban_sdk::Symbol = symbol_short!("mint");

#[cfg(test)]
mod tests;

// ============================================================================
// Contract Implementation
// ============================================================================

#[contract]
pub struct CommitmentNFTContract;

#[contractimpl]
impl CommitmentNFTContract {
    /// Initialize the NFT contract
    pub fn initialize(e: Env, admin: Address) -> Result<(), ContractError> {
        // Check if already initialized
        if e.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }

        // Store admin address
        e.storage().instance().set(&DataKey::Admin, &admin);

        // Initialize token counter to 0
        e.storage().instance().set(&DataKey::TokenCounter, &0u32);

        // Initialize empty token IDs vector
        let token_ids: Vec<u32> = Vec::new(&e);
        e.storage().instance().set(&DataKey::TokenIds, &token_ids);

        Ok(())
    }

    /// Validate commitment type
    fn is_valid_commitment_type(e: &Env, commitment_type: &String) -> bool {
        let safe = String::from_str(e, "safe");
        let balanced = String::from_str(e, "balanced");
        let aggressive = String::from_str(e, "aggressive");
        *commitment_type == safe || *commitment_type == balanced || *commitment_type == aggressive
    }

    /// Set the authorized commitment_core contract address for settlement
    /// Only the admin can call this function
    pub fn set_core_contract(e: Env, core_contract: Address) -> Result<(), ContractError> {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotInitialized)?;
        admin.require_auth();

        e.storage()
            .instance()
            .set(&DataKey::CoreContract, &core_contract);

        // Emit event for access control change
        e.events()
            .publish((Symbol::new(&e, "CoreContractSet"),), (core_contract,));

        Ok(())
    }

    /// Get the authorized commitment_core contract address
    pub fn get_core_contract(e: Env) -> Result<Address, ContractError> {
        e.storage()
            .instance()
            .get(&DataKey::CoreContract)
            .ok_or(ContractError::NotInitialized)
    }

    /// Get the admin address
    pub fn get_admin(e: Env) -> Result<Address, ContractError> {
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotInitialized)
    }

    // ========================================================================
    // NFT Minting
    // ========================================================================

    /// Mint a new Commitment NFT
    ///
    /// # Arguments
    /// * `caller` - The address calling the mint function (must be authorized)
    /// * `owner` - The address that will own the NFT
    /// * `commitment_id` - Unique identifier for the commitment
    /// * `duration_days` - Duration of the commitment in days
    /// * `max_loss_percent` - Maximum allowed loss percentage (0-100)
    /// * `commitment_type` - Type of commitment ("safe", "balanced", "aggressive")
    /// * `initial_amount` - Initial amount committed
    /// * `asset_address` - Address of the asset contract
    ///
    /// # Returns
    /// The token_id of the newly minted NFT
    /// 
    /// # Reentrancy Protection
    /// Uses checks-effects-interactions pattern. This function only writes to storage
    /// and doesn't make external calls, but still protected for consistency.
    pub fn mint(
        e: Env,
        owner: Address,
        commitment_id: String,
        duration_days: u32,
        max_loss_percent: u32,
        commitment_type: String,
        initial_amount: i128,
        asset_address: Address,
        early_exit_penalty: u32,
    ) -> Result<u32, ContractError> {
        // Reentrancy protection
        let guard: bool = e.storage()
            .instance()
            .get(&DataKey::ReentrancyGuard)
            .unwrap_or(false);
        
        if guard {
            return Err(ContractError::ReentrancyDetected);
        }
        e.storage().instance().set(&DataKey::ReentrancyGuard, &true);

        // CHECKS: Verify contract is initialized
        if !e.storage().instance().has(&DataKey::Admin) {
            e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
            return Err(ContractError::NotInitialized);
        }

        // Validate inputs
        if duration_days == 0 {
            e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
            return Err(ContractError::InvalidDuration);
        }
        if max_loss_percent > 100 {
            e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
            return Err(ContractError::InvalidMaxLoss);
        }
        if !Self::is_valid_commitment_type(&e, &commitment_type) {
            e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
            return Err(ContractError::InvalidCommitmentType);
        }
        if initial_amount <= 0 {
            e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
            return Err(ContractError::InvalidAmount);
        }

        // EFFECTS: Update state
        // Generate unique token_id
        let token_id: u32 = e.storage().instance().get(&DataKey::TokenCounter).unwrap_or(0);
        let next_token_id = token_id + 1;
        e.storage().instance().set(&DataKey::TokenCounter, &next_token_id);

        // Calculate timestamps
        let created_at = e.ledger().timestamp();
        let seconds_per_day: u64 = 86400;
        let expires_at = created_at + (duration_days as u64 * seconds_per_day);

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

        // Create CommitmentNFT
        let nft = CommitmentNFT {
            owner: owner.clone(),
            token_id,
            metadata,
            is_active: true,
            early_exit_penalty,
        };

        // Store NFT data
        e.storage().persistent().set(&DataKey::NFT(token_id), &nft);

        // Update owner balance
        let current_balance: u32 = e.storage().persistent().get(&DataKey::OwnerBalance(owner.clone())).unwrap_or(0);
        e.storage().persistent().set(&DataKey::OwnerBalance(owner.clone()), &(current_balance + 1));

        // Update owner tokens list
        let mut owner_tokens: Vec<u32> = e.storage().persistent().get(&DataKey::OwnerTokens(owner.clone())).unwrap_or(Vec::new(&e));
        owner_tokens.push_back(token_id);
        e.storage().persistent().set(&DataKey::OwnerTokens(owner.clone()), &owner_tokens);

        // Add token_id to the list of all tokens
        let mut token_ids: Vec<u32> = e.storage().instance().get(&DataKey::TokenIds).unwrap_or(Vec::new(&e));
        token_ids.push_back(token_id);
        e.storage().instance().set(&DataKey::TokenIds, &token_ids);

        // Clear reentrancy guard
        e.storage().instance().set(&DataKey::ReentrancyGuard, &false);

        // Emit mint event
        e.events().publish(
            (symbol_short!("Mint"), token_id, owner.clone()),
            (commitment_id, e.ledger().timestamp()),
        );

        Ok(token_id)
    }

    // ========================================================================
    // NFT Query Functions
    // ========================================================================

    /// Get NFT metadata by token_id
    pub fn get_metadata(e: Env, token_id: u32) -> Result<CommitmentNFT, ContractError> {
        e.storage()
            .persistent()
            .get(&DataKey::NFT(token_id))
            .ok_or(ContractError::TokenNotFound)
    }


    /// Get owner of NFT
    pub fn owner_of(e: Env, token_id: u32) -> Result<Address, ContractError> {
        let nft: CommitmentNFT = e
            .storage()
            .persistent()
            .get(&DataKey::NFT(token_id))
            .ok_or(ContractError::TokenNotFound)?;

        Ok(nft.owner)
    }

    /// Transfer NFT to new owner
    /// 
    /// # Reentrancy Protection
    /// Uses checks-effects-interactions pattern. This function only writes to storage
    /// and doesn't make external calls, but still protected for consistency.
    pub fn transfer(e: Env, from: Address, to: Address, token_id: u32) -> Result<(), ContractError> {
        // Reentrancy protection
        let guard: bool = e.storage()
            .instance()
            .get(&DataKey::ReentrancyGuard)
            .unwrap_or(false);
        
        if guard {
            return Err(ContractError::ReentrancyDetected);
        }
        e.storage().instance().set(&DataKey::ReentrancyGuard, &true);

        // CHECKS: Require authorization from the sender
        from.require_auth();

        // Get the NFT
        let mut nft: CommitmentNFT = e
            .storage()
            .persistent()
            .get(&DataKey::NFT(token_id))
            .ok_or_else(|| {
                e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
                ContractError::TokenNotFound
            })?;

        // Verify ownership
        if nft.owner != from {
            e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
            return Err(ContractError::NotOwner);
        }

        // Check if NFT is still active (active NFTs may have transfer restrictions)
        // For now, we allow transfers regardless of active status
        // Uncomment below to restrict transfers of active NFTs:
        // if nft.is_active {
        //     e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
        //     return Err(ContractError::TransferNotAllowed);
        // }

        // EFFECTS: Update state
        // Update owner
        nft.owner = to.clone();
        e.storage().persistent().set(&DataKey::NFT(token_id), &nft);

        // OPTIMIZATION: Batch read balances before updating
        let (from_balance, to_balance) = {
            let from_bal = e.storage().persistent().get(&DataKey::OwnerBalance(from.clone())).unwrap_or(0u32);
            let to_bal = e.storage().persistent().get(&DataKey::OwnerBalance(to.clone())).unwrap_or(0u32);
            (from_bal, to_bal)
        };
        
        // Update balance counts
        if from_balance > 0 {
            e.storage().persistent().set(&DataKey::OwnerBalance(from.clone()), &(from_balance - 1));
        }
        e.storage().persistent().set(&DataKey::OwnerBalance(to.clone()), &(to_balance + 1));

        // Update owner tokens lists
        let mut from_tokens: Vec<u32> = e.storage().persistent().get(&DataKey::OwnerTokens(from.clone())).unwrap_or(Vec::new(&e));
        if let Some(index) = from_tokens.iter().position(|id| id == token_id) {
            from_tokens.remove(index as u32);
        }
        e.storage().persistent().set(&DataKey::OwnerTokens(from.clone()), &from_tokens);

        let mut to_tokens: Vec<u32> = e.storage().persistent().get(&DataKey::OwnerTokens(to.clone())).unwrap_or(Vec::new(&e));
        to_tokens.push_back(token_id);
        e.storage().persistent().set(&DataKey::OwnerTokens(to.clone()), &to_tokens);

        // Clear reentrancy guard
        e.storage().instance().set(&DataKey::ReentrancyGuard, &false);

        // Emit transfer event
        e.events().publish(
            (symbol_short!("Transfer"), from, to),
            (token_id, e.ledger().timestamp()),
        );

        Ok(())
    }

    /// Check if NFT is active
    pub fn is_active(e: Env, token_id: u32) -> Result<bool, ContractError> {
        let nft: CommitmentNFT = e
            .storage()
            .persistent()
            .get(&DataKey::NFT(token_id))
            .ok_or(ContractError::TokenNotFound)?;

        Ok(nft.is_active)
    }

    /// Get total supply of NFTs minted
    pub fn total_supply(e: Env) -> u32 {
        e.storage().instance().get(&DataKey::TokenCounter).unwrap_or(0)
    }

    /// Get NFT count for a specific owner
    pub fn balance_of(e: Env, owner: Address) -> u32 {
        e.storage()
            .persistent()
            .get(&DataKey::OwnerBalance(owner))
            .unwrap_or(0)
    }

    /// Get all NFTs metadata (for frontend)
    pub fn get_all_metadata(e: Env) -> Vec<CommitmentNFT> {
        let token_ids: Vec<u32> = e
            .storage()
            .instance()
            .get(&DataKey::TokenIds)
            .unwrap_or(Vec::new(&e));

        let mut nfts: Vec<CommitmentNFT> = Vec::new(&e);

        for token_id in token_ids.iter() {
            if let Some(nft) = e.storage().persistent().get::<DataKey, CommitmentNFT>(&DataKey::NFT(token_id)) {
                nfts.push_back(nft);
            }
        }

        nfts
    }

    /// Get all NFTs owned by a specific address
    pub fn get_nfts_by_owner(e: Env, owner: Address) -> Vec<CommitmentNFT> {
        let token_ids: Vec<u32> = e
            .storage()
            .persistent()
            .get(&DataKey::OwnerTokens(owner))
            .unwrap_or(Vec::new(&e));

        let mut owned_nfts: Vec<CommitmentNFT> = Vec::new(&e);

        for token_id in token_ids.iter() {
            if let Some(nft) = e.storage().persistent().get::<DataKey, CommitmentNFT>(&DataKey::NFT(token_id)) {
                owned_nfts.push_back(nft);
            }
        }

        owned_nfts
    }

    // ========================================================================
    // Settlement (Issue #5 - Main Implementation)
    // ========================================================================

    /// Mark NFT as settled (after maturity)
    /// 
    /// # Reentrancy Protection
    /// Uses checks-effects-interactions pattern. This function only writes to storage
    /// and doesn't make external calls, but still protected for consistency.
    pub fn settle(e: Env, token_id: u32) -> Result<(), ContractError> {
        // Reentrancy protection
        let guard: bool = e.storage()
            .instance()
            .get(&DataKey::ReentrancyGuard)
            .unwrap_or(false);
        
        if guard {
            return Err(ContractError::ReentrancyDetected);
        }
        e.storage().instance().set(&DataKey::ReentrancyGuard, &true);

        // CHECKS: Get the NFT
        let mut nft: CommitmentNFT = e
            .storage()
            .persistent()
            .get(&DataKey::NFT(token_id))
            .ok_or_else(|| {
                e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
                ContractError::TokenNotFound
            })?;

        // Check if already settled
        if !nft.is_active {
            e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
            return Err(ContractError::AlreadySettled);
        }

        // Verify the commitment has expired
        let current_time = e.ledger().timestamp();
        if current_time < nft.metadata.expires_at {
            e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
            return Err(ContractError::NotExpired);
        }

        // EFFECTS: Update state
        // Mark as inactive (settled)
        nft.is_active = false;
        e.storage().persistent().set(&DataKey::NFT(token_id), &nft);

        // Clear reentrancy guard
        e.storage().instance().set(&DataKey::ReentrancyGuard, &false);

        // Emit settle event
        e.events().publish(
            (symbol_short!("Settle"), token_id),
            e.ledger().timestamp(),
        );

        Ok(())
    }

    /// Check if an NFT has expired (based on time)
    pub fn is_expired(e: Env, token_id: u32) -> Result<bool, ContractError> {
        let nft: CommitmentNFT = e
            .storage()
            .persistent()
            .get(&DataKey::NFT(token_id))
            .ok_or(ContractError::TokenNotFound)?;

        let current_time = e.ledger().timestamp();
        Ok(current_time >= nft.metadata.expires_at)
    }

    /// Check if a token exists
    pub fn token_exists(e: Env, token_id: u32) -> bool {
        e.storage().persistent().has(&DataKey::NFT(token_id))
    }

    // ========================================================================
    // Batch Operations
    // ========================================================================

    /// Batch transfer multiple NFTs in a single transaction
    ///
    /// # Arguments
    /// * `params_list` - Vector of TransferParams for each transfer
    /// * `mode` - BatchMode::Atomic or BatchMode::BestEffort
    ///
    /// # Returns
    /// BatchResult with empty results and any errors
    ///
    /// # Gas Optimization
    /// - Batch reads of NFT data
    /// - Aggregate balance updates per owner (single write per owner)
    /// - Batch owner list updates
    pub fn batch_transfer(
        e: Env,
        params_list: Vec<TransferParams>,
        mode: BatchMode,
    ) -> BatchResultVoid {
        // Reentrancy protection
        let guard: bool = e.storage()
            .instance()
            .get(&DataKey::ReentrancyGuard)
            .unwrap_or(false);

        if guard {
            let mut errors = Vec::new(&e);
            errors.push_back(BatchError {
                index: 0,
                error_code: ContractError::ReentrancyDetected as u32,
                context: String::from_str(&e, "reentrancy_detected"),
            });
            return BatchResultVoid::failure(&e, errors);
        }
        e.storage().instance().set(&DataKey::ReentrancyGuard, &true);

        // Validate batch size
        let batch_size = params_list.len();
        let contract_name = String::from_str(&e, "commitment_nft");
        if let Err(error_code) = BatchProcessor::enforce_batch_limits(&e, batch_size, Some(contract_name)) {
            e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
            let mut errors = Vec::new(&e);
            errors.push_back(BatchError {
                index: 0,
                error_code,
                context: String::from_str(&e, "batch_size_validation"),
            });
            return BatchResultVoid::failure(&e, errors);
        }

        let mut errors = Vec::new(&e);
        let mut results = Vec::new(&e);

        // Track balance changes per address (optimization)
        use soroban_sdk::Map;
        let mut balance_deltas: Map<Address, i32> = Map::new(&e);
        let mut owner_tokens_updates: Map<Address, Vec<u32>> = Map::new(&e);

        // Process each transfer
        for i in 0..batch_size {
            let params = params_list.get(i).unwrap();

            // Require authorization from sender
            params.from.require_auth();

            // Get NFT
            let mut nft: CommitmentNFT = match e.storage().persistent().get(&DataKey::NFT(params.token_id)) {
                Some(nft) => nft,
                None => {
                    if mode == BatchMode::Atomic {
                        e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
                        errors.push_back(BatchError {
                            index: i,
                            error_code: ContractError::TokenNotFound as u32,
                            context: String::from_str(&e, "token_not_found"),
                        });
                        return BatchResultVoid::failure(&e, errors);
                    } else {
                        errors.push_back(BatchError {
                            index: i,
                            error_code: ContractError::TokenNotFound as u32,
                            context: String::from_str(&e, "token_not_found"),
                        });
                        continue;
                    }
                }
            };

            // Verify ownership
            if nft.owner != params.from {
                if mode == BatchMode::Atomic {
                    e.storage().instance().set(&DataKey::ReentrancyGuard, &false);
                    errors.push_back(BatchError {
                        index: i,
                        error_code: ContractError::NotOwner as u32,
                        context: String::from_str(&e, "not_owner"),
                    });
                    return BatchResultVoid::failure(&e, errors);
                } else {
                    errors.push_back(BatchError {
                        index: i,
                        error_code: ContractError::NotOwner as u32,
                        context: String::from_str(&e, "not_owner"),
                    });
                    continue;
                }
            }

            // Update NFT owner
            nft.owner = params.to.clone();
            e.storage().persistent().set(&DataKey::NFT(params.token_id), &nft);

            // Track balance changes (aggregate updates)
            let from_delta = balance_deltas.get(params.from.clone()).unwrap_or(0) - 1;
            balance_deltas.set(params.from.clone(), from_delta);

            let to_delta = balance_deltas.get(params.to.clone()).unwrap_or(0) + 1;
            balance_deltas.set(params.to.clone(), to_delta);

            // Track owner token list updates
            if !owner_tokens_updates.contains_key(params.from.clone()) {
                let from_tokens: Vec<u32> = e.storage()
                    .persistent()
                    .get(&DataKey::OwnerTokens(params.from.clone()))
                    .unwrap_or(Vec::new(&e));
                owner_tokens_updates.set(params.from.clone(), from_tokens);
            }

            if !owner_tokens_updates.contains_key(params.to.clone()) {
                let to_tokens: Vec<u32> = e.storage()
                    .persistent()
                    .get(&DataKey::OwnerTokens(params.to.clone()))
                    .unwrap_or(Vec::new(&e));
                owner_tokens_updates.set(params.to.clone(), to_tokens);
            }

            // Update token lists
            let mut from_tokens = owner_tokens_updates.get(params.from.clone()).unwrap();
            if let Some(index) = from_tokens.iter().position(|id| id == params.token_id) {
                from_tokens.remove(index as u32);
            }
            owner_tokens_updates.set(params.from.clone(), from_tokens);

            let mut to_tokens = owner_tokens_updates.get(params.to.clone()).unwrap();
            to_tokens.push_back(params.token_id);
            owner_tokens_updates.set(params.to.clone(), to_tokens);

            results.push_back(());

            // Emit transfer event
            e.events().publish(
                (symbol_short!("Transfer"), params.from.clone(), params.to.clone()),
                (params.token_id, e.ledger().timestamp()),
            );
        }

        // Apply balance changes (aggregate updates - optimization)
        for (address, delta) in balance_deltas.iter() {
            let current_balance: i32 = e.storage()
                .persistent()
                .get(&DataKey::OwnerBalance(address.clone()))
                .unwrap_or(0) as i32;
            let new_balance = (current_balance + delta).max(0) as u32;
            e.storage().persistent().set(&DataKey::OwnerBalance(address), &new_balance);
        }

        // Apply owner token list updates
        for (address, tokens) in owner_tokens_updates.iter() {
            e.storage().persistent().set(&DataKey::OwnerTokens(address), &tokens);
        }

        // Clear reentrancy guard
        e.storage().instance().set(&DataKey::ReentrancyGuard, &false);

        // Emit batch event
        e.events().publish(
            (Symbol::new(&e, "BatchTransfer"), batch_size),
            (results.len(), errors.len(), e.ledger().timestamp()),
        );

        BatchResultVoid::partial(results.len(), errors)
    }
}

#[cfg(all(test, feature = "benchmark"))]
mod benchmarks;
