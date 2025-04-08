"""
Umazen Client Library - Python Interface for Decentralized AI Operations
"""

import os
import json
import asyncio
from pathlib import Path
from typing import Optional, Dict, List, Any, Union
from dataclasses import dataclass

import numpy as np
from anchorpy import Program, Context, Idl
from solana.rpc.async_api import AsyncClient
from solana.rpc.commitment import Confirmed
from solana.publickey import PublicKey
from solana.keypair import Keypair
from solana.transaction import Transaction
from cryptography.hazmat.primitives import serialization
from aiohttp import ClientSession

# Custom types
ModelParams = Union[np.ndarray, Dict[str, np.ndarray]]
TrainingConfig = Dict[str, Any]

@dataclass
class ModelMetadata:
    model_id: str
    owner: PublicKey
    hash: str
    size: int
    timestamp: int
    stake_amount: float
    accuracy: float

class UmazenClient:
    """
    Main client class for interacting with Umazen's AI infrastructure on Solana
    
    Features:
    - Secure connection to Solana clusters
    - Model training submission
    - Inference market operations
    - Model verification
    - Stake management
    - Cryptographic data validation
    """

    def __init__(
        self,
        network: str = "devnet",
        rpc_url: Optional[str] = None,
        keypair_path: Optional[str] = None,
        idl_path: Optional[str] = None
    ):
        """
        Initialize Umazen client
        
        :param network: Solana cluster (mainnet, devnet, testnet)
        :param rpc_url: Custom RPC endpoint URL
        :param keypair_path: Path to wallet keypair file
        :param idl_path: Path to Anchor IDL file
        """
        self.network = network
        self._rpc_client = None
        self._program = None
        self._keypair = self._load_keypair(keypair_path)
        self.idl = self._load_idl(idl_path)
        self.program_id = PublicKey(os.getenv("UMAZEN_PROGRAM_ID", "UMAZEN..."))
        self.session = ClientSession()

    async def connect(self):
        """Establish connection to Solana cluster"""
        rpc_url = self._get_rpc_url()
        self._rpc_client = AsyncClient(rpc_url, commitment=Confirmed)
        self._program = Program(
            idl=self.idl,
            program_id=self.program_id,
            provider=Context(
                connection=self._rpc_client,
                wallet=self._keypair
            )
        )

    async def close(self):
        """Clean up connections"""
        await self._rpc_client.close()
        await self.session.close()

    def _load_keypair(self, path: Optional[str]) -> Keypair:
        """Load wallet keypair from file or environment"""
        if path:
            with open(path, "r") as f:
                key_data = json.load(f)
            return Keypair.from_secret_key(bytes(key_data))
        
        if os.getenv("SOLANA_KEYPAIR"):
            return Keypair.from_secret_key(
                bytes(json.loads(os.getenv("SOLANA_KEYPAIR")))
            )
            
        raise ValueError("No valid keypair provided")

    def _load_idl(self, path: Optional[str]) -> Idl:
        """Load Anchor IDL from file or embedded data"""
        if path:
            with open(path, "r") as f:
                return Idl.from_json(f.read())
                
        # Fallback to embedded IDL
        return Idl.from_json(EMBEDDED_IDL)

    def _get_rpc_url(self) -> str:
        """Get appropriate RPC endpoint"""
        urls = {
            "mainnet": "https://api.mainnet-beta.solana.com",
            "devnet": "https://api.devnet.solana.com",
            "testnet": "https://api.testnet.solana.com"
        }
        return urls.get(self.network, self.network)

    async def submit_training_task(
        self,
        model_params: ModelParams,
        config: TrainingConfig,
        stake_amount: float = 1.0
    ) -> str:
        """
        Submit a new AI model training task to the network
        
        :param model_params: Initial model parameters
        :param config: Training configuration
        :param stake_amount: SOL amount to stake
        :return: Transaction signature
        """
        # Convert model parameters to on-chain format
        serialized_params = self._serialize_params(model_params)
        param_hash = self._generate_hash(serialized_params)

        # Prepare accounts
        model_account = self._derive_model_address(param_hash)
        ix = self._program.instruction.submit_training_task(
            param_hash,
            stake_amount,
            ctx=Context(
                accounts={
                    "model_account": model_account,
                    "authority": self._keypair.public_key,
                    "system_program": PublicKey("11111111111111111111111111111111")
                }
            )
        )

        # Build and send transaction
        tx = Transaction().add(ix)
        return await self._send_transaction(tx)

    async def get_model_metadata(self, model_id: str) -> ModelMetadata:
        """Retrieve metadata for a specific AI model"""
        model_account = PublicKey(model_id)
        account_info = await self._program.account["ModelAccount"].fetch(model_account)
        return ModelMetadata(
            model_id=str(model_account),
            owner=account_info.authority,
            hash=account_info.model_hash,
            size=account_info.model_size,
            timestamp=account_info.timestamp,
            stake_amount=account_info.stake_amount,
            accuracy=account_info.accuracy
        )

    async def perform_inference(
        self,
        model_id: str,
        input_data: np.ndarray,
        max_cost: float = 0.1
    ) -> np.ndarray:
        """
        Execute inference using a published AI model
        
        :param model_id: Address of model account
        :param input_data: Input data for inference
        :param max_cost: Maximum cost in SOL
        :return: Inference result
        """
        # Verify model availability
        model_account = PublicKey(model_id)
        if not await self._program.account["ModelAccount"].fetch_nullable(model_account):
            raise ValueError("Model not found")

        # Generate inference request hash
        input_hash = self._generate_hash(input_data.tobytes())

        # Create instruction
        ix = self._program.instruction.request_inference(
            input_hash,
            max_cost,
            ctx=Context(
                accounts={
                    "model_account": model_account,
                    "user": self._keypair.public_key
                }
            )
        )

        # Execute transaction
        tx = Transaction().add(ix)
        await self._send_transaction(tx)

        # Retrieve and verify result
        result = await self._fetch_inference_result(input_hash)
        return self._process_inference_result(result)

    async def _send_transaction(self, tx: Transaction) -> str:
        """Execute signed transaction with retries"""
        signers = [self._keypair]
        return await self._program.provider.send(
            tx,
            signers=signers,
            opts={"max_retries": 3}
        )

    def _derive_model_address(self, seed_hash: bytes) -> PublicKey:
        """Generate deterministic PDA for model account"""
        return PublicKey.find_program_address(
            [b"model", seed_hash],
            self.program_id
        )[0]

    @staticmethod
    def _serialize_params(params: ModelParams) -> bytes:
        """Serialize model parameters for on-chain storage"""
        if isinstance(params, np.ndarray):
            return params.tobytes()
        return b"".join([v.tobytes() for v in params.values()])

    @staticmethod
    def _generate_hash(data: bytes) -> str:
        """Generate cryptographic hash of model data"""
        # Implementation using SHA3-256
        from cryptography.hazmat.primitives import hashes
        digest = hashes.Hash(hashes.SHA3_256())
        digest.update(data)
        return digest.finalize().hex()

# Embedded IDL for fallback loading
EMBEDDED_IDL = {
    "version": "0.1.0",
    "name": "umazen",
    "instructions": [
        {
            "name": "submitTrainingTask",
            "accounts": [
                {"name": "modelAccount", "isMut": True, "isSigner": False},
                {"name": "authority", "isMut": True, "isSigner": True},
                {"name": "systemProgram", "isMut": False, "isSigner": False}
            ],
            "args": [
                {"name": "modelHash", "type": "string"},
                {"name": "stakeAmount", "type": "u64"}
            ]
        },
        # Additional instruction definitions...
    ],
    "accounts": [
        {
            "name": "ModelAccount",
            "type": {
                "kind": "struct",
                "fields": [
                    {"name": "authority", "type": "publicKey"},
                    {"name": "modelHash", "type": "string"},
                    {"name": "modelSize", "type": "u32"},
                    {"name": "timestamp", "type": "i64"},
                    {"name": "stakeAmount", "type": "u64"},
                    {"name": "accuracy", "type": "f32"}
                ]
            }
        }
    ]
}

# Example usage
async def main():
    client = UmazenClient(network="devnet")
    await client.connect()
    
    try:
        # Submit training task
        initial_params = np.random.rand(100, 100)
        tx_sig = await client.submit_training_task(
            model_params=initial_params,
            config={"epochs": 10, "batch_size": 32},
            stake_amount=1.5
        )
        print(f"Training submitted: {tx_sig}")

        # Retrieve model info
        metadata = await client.get_model_metadata(tx_sig)
        print(f"Model accuracy: {metadata.accuracy}")

    finally:
        await client.close()

if __name__ == "__main__":
    asyncio.run(main())
