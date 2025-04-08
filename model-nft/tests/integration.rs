//! Umazen Integration Tests - End-to-End System Validation

use {
    anchor_lang::{prelude::*, InstructionData, ToAccountMetas},
    anchor_spl::token,
    solana_program_test::*,
    solana_sdk::{
        instruction::Instruction, signature::Keypair, signer::Signer, system_program,
        transaction::Transaction,
    },
    umazen_program::{
        self,
        accounts as umazen_accounts,
        instructions as umazen_instructions,
        ModelMetadata,
        ComputeRequirements,
        ValidationError,
    },
};

const MODEL_HASH: [u8; 32] = [0x11; 32]; // Sample hash

async fn setup_context() -> ProgramTestContext {
    let mut program = ProgramTest::new(
        "umazen_program",
        umazen_program::ID,
        processor!(umazen_program::entry),
    );

    // Add required supporting programs
    program.add_program("spl_token", token::ID, None);
    program.add_program("spl_associated_token_account", anchor_spl::associated_token::ID, None);

    program.start_with_context().await
}

#[tokio::test]
async fn test_full_model_lifecycle() {
    let mut context = setup_context().await;
    
    // Initialize DAO
    let dao = Keypair::new();
    let dao_metadata = ModelMetadata {
        metadata_uri: "QmTestCID".to_string(),
        royalty_basis_points: 1000,
        architecture: "ResNet-50".to_string(),
        model_hash: MODEL_HASH,
        compute_requirements: ComputeRequirements {
            min_vram: 8000,
            min_compute_capability: 7.5,
            required_instructions: vec!["tensorcores".to_string()],
        },
    };

    // 1. Mint Model NFT
    let mint_ix = umazen_instructions::MintModelNFT {
        metadata: dao_metadata.clone(),
    };
    
    let accounts = umazen_accounts::MintModelNFT {
        dao: dao.pubkey(),
        model_nft: Pubkey::new_unique(),
        system_program: system_program::id(),
        token_program: token::ID,
        metadata_program: mpl_token_metadata::ID,
        rent: anchor_lang::solana_program::sysvar::rent::ID,
    };

    let tx = Transaction::new_signed_with_payer(
        &[Instruction {
            program_id: umazen_program::ID,
            accounts: accounts.to_account_metas(None),
            data: mint_ix.data(),
        }],
        Some(&context.payer.pubkey()),
        &[&context.payer, &dao],
        context.last_blockhash,
    );

    context.banks_client.process_transaction(tx).await.unwrap();

    // Verify NFT state
    let nft_account = context.banks_client
        .get_account(accounts.model_nft)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(nft_account.owner, token::ID);

    // 2. Start Training Job
    let trainer = Keypair::new();
    let training_data_hash = [0x22; 32];
    
    let start_training_ix = umazen_instructions::StartTraining {
        model: accounts.model_nft,
        data_hash: training_data_hash,
        epochs: 10,
    };

    // Execute training start
    // ... (similar account setup and transaction execution)

    // 3. Submit Inference Request
    let user = Keypair::new();
    let input_data = vec![0.5; 128]; // Sample input
    
    let infer_ix = umazen_instructions::RequestInference {
        model: accounts.model_nft,
        input: input_data.clone(),
    };

    // Process inference
    // ... (validate fee payments and result storage)

    // 4. Claim Royalties
    let claim_ix = umazen_instructions::ClaimRoyalties {
        model: accounts.model_nft,
    };

    // Execute claim
    // ... (verify DAO treasury balance increase)
}

#[tokio::test]
async fn test_invalid_model_mint() {
    let mut context = setup_context().await;

    let invalid_metadata = ModelMetadata {
        metadata_uri: "InvalidCIDFormat".to_string(), // Missing Q prefix
        // ... other fields
    };

    let mint_ix = umazen_instructions::MintModelNFT {
        metadata: invalid_metadata,
    };

    // Attempt transaction
    let tx = Transaction::new_signed_with_payer(
        // ... account setup
    );

    let result = context.banks_client.process_transaction(tx).await;
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().unwrap(),
        TransactionError::InstructionError(
            0,
            InstructionError::Custom(ValidationError::InvalidCidFormat as u32)
        )
    );
}

#[tokio::test]
async fn test_unauthorized_model_update() {
    let mut context = setup_context().await;
    
    // Initial mint (similar to first test)
    // ...

    let attacker = Keypair::new();
    let malicious_metadata = ModelMetadata {
        metadata_uri: "QmHacked".to_string(),
        // ... altered fields
    };

    let update_ix = umazen_instructions::UpdateModelMetadata {
        model: accounts.model_nft,
        new_metadata: malicious_metadata,
    };

    // Sign with attacker instead of DAO
    let tx = Transaction::new_signed_with_payer(
        // ... account setup with attacker
    );

    let result = context.banks_client.process_transaction(tx).await;
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().unwrap(),
        TransactionError::InstructionError(
            0,
            InstructionError::Custom(ValidationError::DaoApprovalRequired as u32)
        )
    );
}

#[tokio::test]
async fn test_staking_rewards_accumulation() {
    let mut context = setup_context().await;
    
    // Setup model and staker
    // ...

    // Stake model
    let stake_ix = umazen_instructions::StakeModel {
        model: model_pubkey,
        duration: 86400, // 1 day
    };

    // Process staking
    // ...

    // Advance time by 2 days
    context.warp_to_slot(context.slot() + 172800).unwrap();

    // Claim rewards
    let claim_ix = umazen_instructions::ClaimRewards {
        stake_account: stake_pubkey,
    };

    // Verify reward distribution
    // ...
}

#[tokio::test]
async fn test_zkp_verification_flow() {
    let mut context = setup_context().await;
    
    // Generate ZK proof for model execution
    let (proof, public_inputs) = generate_sample_proof();

    let verify_ix = umazen_instructions::VerifyInference {
        model: model_pubkey,
        proof,
        public_inputs,
    };

    // Execute verification
    // ...

    // Check verification state
    let verification_account = // ... get account
    assert!(verification_account.is_verified);
}

// Helper function for ZKP tests
fn generate_sample_proof() -> (Vec<u8>, Vec<u8>) {
    // Mock implementation for testing
    (vec![0x99; 128], vec![0x55; 32])
}
