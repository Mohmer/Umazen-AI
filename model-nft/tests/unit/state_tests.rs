//! Umazen State Transition Tests - Program State Mutation Verification

use {
    anchor_lang::{prelude::*, system_program},
    solana_program_test::*,
    solana_sdk::{signature::Keypair, signer::Signer},
    umazen_program::{
        self,
        accounts::{InitializeModel, MintModelNFT},
        instructions::MintModelNFTArgs,
        ModelMetadata,
        ComputeRequirements,
        ValidationError,
    },
};

const MODEL_HASH: [u8; 32] = [0xde; 32];

async fn setup_program() -> (ProgramTestContext, Keypair) {
    let mut program = ProgramTest::new(
        "umazen_program",
        umazen_program::ID,
        processor!(umazen_program::entry),
    );
    
    let context = program.start_with_context().await;
    let payer = context.payer.clone();
    
    (context, payer)
}

#[tokio::test]
async fn test_model_nft_initialization() {
    let (mut context, payer) = setup_program().await;
    
    // Initialize accounts
    let model_nft = Keypair::new();
    let dao = Keypair::new();
    let metadata = ModelMetadata {
        metadata_uri: "QmStateTest".to_string(),
        royalty_basis_points: 1000,
        architecture: "GPT-3".to_string(),
        model_hash: MODEL_HASH,
        compute_requirements: ComputeRequirements {
            min_vram: 16000,
            min_compute_capability: 8.6,
            required_instructions: vec!["fp8".to_string()],
        },
    };

    // Build transaction
    let ix = Instruction {
        program_id: umazen_program::ID,
        accounts: umazen_program::accounts::MintModelNFT {
            dao: dao.pubkey(),
            model_nft: model_nft.pubkey(),
            system_program: system_program::id(),
            token_program: anchor_spl::token::ID,
            metadata_program: mpl_token_metadata::ID,
            rent: anchor_lang::solana_program::sysvar::rent::ID,
        }.to_account_metas(None),
        data: umazen_program::instruction::MintModelNFT { metadata }.data(),
    };

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer, &dao, &model_nft],
        context.last_blockhash,
    );

    // Execute and verify
    context.banks_client.process_transaction(tx).await.unwrap();
    
    // Validate NFT account state
    let nft_account = context.banks_client
        .get_account(model_nft.pubkey())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(nft_account.data.len(), 82); // SPL token mint size
    assert_eq!(nft_account.owner, anchor_spl::token::ID);

    // Validate metadata state
    let metadata_account = context.banks_client
        .get_account(mpl_token_metadata::state::PDA::find_metadata_account(&model_nft.pubkey()).0)
        .await
        .unwrap()
        .unwrap();
    assert!(metadata_account.data.starts_with(b"umazen_metadata_v1"));
}

#[tokio::test]
async fn test_invalid_state_transition() {
    let (mut context, payer) = setup_program().await;
    
    let model_nft = Keypair::new();
    let dao = Keypair::new();
    
    // Malformed metadata with empty architecture
    let invalid_metadata = ModelMetadata {
        metadata_uri: "QmInvalid".to_string(),
        royalty_basis_points: 1000,
        architecture: String::new(), // Invalid
        model_hash: MODEL_HASH,
        compute_requirements: ComputeRequirements::default(),
    };

    let ix = Instruction {
        program_id: umazen_program::ID,
        accounts: umazen_program::accounts::MintModelNFT {
            dao: dao.pubkey(),
            model_nft: model_nft.pubkey(),
            system_program: system_program::id(),
            token_program: anchor_spl::token::ID,
            metadata_program: mpl_token_metadata::ID,
            rent: anchor_lang::solana_program::sysvar::rent::ID,
        }.to_account_metas(None),
        data: umazen_program::instruction::MintModelNFT { metadata: invalid_metadata }.data(),
    };

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer, &dao, &model_nft],
        context.last_blockhash,
    );

    // Verify failure
    let err = context.banks_client.process_transaction(tx).await.unwrap_err();
    let err = err.unwrap();
    assert_eq!(
        err,
        TransactionError::InstructionError(
            0, 
            InstructionError::Custom(ValidationError::InvalidModelArchitecture as u32)
        )
    );
}

#[tokio::test]
async fn test_state_after_training() {
    let (mut context, payer) = setup_program().await;
    
    // Initialize model NFT
    let (model_nft, dao, metadata) = create_test_model(&mut context).await;

    // Start training
    let trainer = Keypair::new();
    let training_ix = umazen_program::instruction::StartTraining {
        model: model_nft.pubkey(),
        data_hash: [0xfe; 32],
        epochs: 5,
    };

    let tx = Transaction::new_signed_with_payer(
        &[Instruction {
            program_id: umazen_program::ID,
            accounts: umazen_program::accounts::StartTraining {
                dao: dao.pubkey(),
                model: model_nft.pubkey(),
                trainer: trainer.pubkey(),
                system_program: system_program::id(),
            }.to_account_metas(None),
            data: training_ix.data(),
        }],
        Some(&payer.pubkey()),
        &[&payer, &dao],
        context.last_blockhash,
    );

    context.banks_client.process_transaction(tx).await.unwrap();

    // Verify training state
    let training_account = context.banks_client
        .get_account(umazen_program::state::TrainingState::pda(&model_nft.pubkey()).0)
        .await
        .unwrap()
        .unwrap();
    
    let training_state = umazen_program::state::TrainingState::try_deserialize(
        &mut &training_account.data[..]
    ).unwrap();
    
    assert_eq!(training_state.epochs_completed, 0);
    assert_eq!(training_state.target_epochs, 5);
    assert_eq!(training_state.data_hash, [0xfe; 32]);
}

#[tokio::test]
async fn test_inference_state_mutation() {
    let (mut context, payer) = setup_program().await;
    let (model_nft, dao, _) = create_test_model(&mut context).await;

    // Perform inference
    let user = Keypair::new();
    let input = vec![0.8; 256];
    
    let infer_ix = umazen_program::instruction::RequestInference {
        model: model_nft.pubkey(),
        input: input.clone(),
    };

    let tx = Transaction::new_signed_with_payer(
        &[Instruction {
            program_id: umazen_program::ID,
            accounts: umazen_program::accounts::RequestInference {
                payer: user.pubkey(),
                model: model_nft.pubkey(),
                dao: dao.pubkey(),
                system_program: system_program::id(),
                inference_result: Pubkey::new_unique(),
            }.to_account_metas(None),
            data: infer_ix.data(),
        }],
        Some(&user.pubkey()),
        &[&user],
        context.last_blockhash,
    );

    context.banks_client.process_transaction(tx).await.unwrap();

    // Validate inference result state
    let result_account = context.banks_client
        .get_account(umazen_program::state::InferenceResult::pda(&model_nft.pubkey(), &input).0)
        .await
        .unwrap()
        .unwrap();
    
    let result_state = umazen_program::state::InferenceResult::try_deserialize(
        &mut &result_account.data[..]
    ).unwrap();
    
    assert!(result_state.timestamp > 0);
    assert_eq!(result_state.model, model_nft.pubkey());
}

async fn create_test_model(context: &mut ProgramTestContext) -> (Keypair, Keypair, ModelMetadata) {
    let model_nft = Keypair::new();
    let dao = Keypair::new();
    let metadata = ModelMetadata {
        metadata_uri: "QmTest".to_string(),
        royalty_basis_points: 500,
        architecture: "ViT-L/16".to_string(),
        model_hash: MODEL_HASH,
        compute_requirements: ComputeRequirements {
            min_vram: 24000,
            min_compute_capability: 8.9,
            required_instructions: vec!["moe".to_string()],
        },
    };

    let ix = umazen_program::instruction::MintModelNFT {
        metadata: metadata.clone(),
    };

    let tx = Transaction::new_signed_with_payer(
        &[Instruction {
            program_id: umazen_program::ID,
            accounts: umazen_program::accounts::MintModelNFT {
                dao: dao.pubkey(),
                model_nft: model_nft.pubkey(),
                system_program: system_program::id(),
                token_program: anchor_spl::token::ID,
                metadata_program: mpl_token_metadata::ID,
                rent: anchor_lang::solana_program::sysvar::rent::ID,
            }.to_account_metas(None),
            data: ix.data(),
        }],
        Some(&context.payer.pubkey()),
        &[&context.payer, &dao, &model_nft],
        context.last_blockhash,
    );

    context.banks_client.process_transaction(tx).await.unwrap();
    
    (model_nft, dao, metadata)
}
