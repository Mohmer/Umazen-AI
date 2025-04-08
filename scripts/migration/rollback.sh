#!/usr/bin/env bash
set -eo pipefail

# Configuration
readonly SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
readonly ROOT_DIR="$SCRIPT_DIR/.."
readonly PROGRAM_DIR="$ROOT_DIR/blockchain/programs"
readonly CACHE_DIR="$ROOT_DIR/.rollback-cache"
readonly LOG_FILE="$ROOT_DIR/rollback.log"
declare -A NETWORK_CLUSTERS=(
  ["mainnet"]="mainnet-beta"
  ["testnet"]="testnet"
  ["devnet"]="devnet"
)

# Initialize logging
exec > >(tee -a "$LOG_FILE") 2>&1

# Environment validation
validate_environment() {
  if ! command -v solana &> /dev/null; then
    echo "❌ Solana CLI not found. Install with: sh -c \"$(curl -sSfL https://release.solana.com/stable/install)\""
    exit 1
  fi
  
  if ! command -v anchor &> /dev/null; then
    echo "❌ Anchor CLI not found. Install with: cargo install --git https://github.com/coral-xyz/anchor anchor-cli --locked"
    exit 1
  fi
  
  mkdir -p "$CACHE_DIR"
}

# Security checks
check_authority() {
  local network=\$1
  local program_name=\$2
  
  if [[ "$network" == "mainnet" ]]; then
    if [[ -z "$MULTISIG_KEYPAIR" ]]; then
      echo "🔒 Mainnet rollback requires MULTISIG_KEYPAIR environment variable"
      exit 1
    fi
    solana-keygen pubkey "$MULTISIG_KEYPAIR" || {
      echo "❌ Invalid multisig keypair: $MULTISIG_KEYPAIR"
      exit 1
    }
  fi
}

# Core rollback function
rollback_program() {
  local network=\$1
  local program_name=\$2
  local cluster=${NETWORK_CLUSTERS[$network]}
  local program_id=$(jq -r .programs.${network}.${program_name} "$ROOT_DIR/Anchor.toml")
  local old_program_id=$(git show HEAD~1:Anchor.toml | jq -r .programs.${network}.${program_name})
  
  echo "🔄 Rolling back $program_name on $network (${cluster})"
  echo "📄 Current program ID: $program_id"
  echo "⏪ Target program ID: $old_program_id"

  # Build previous version
  git checkout HEAD~1 || {
    echo "❌ Failed to checkout previous commit"
    exit 1
  }
  
  anchor build --program-name $program_name || {
    echo "❌ Build failed for previous version"
    git checkout -
    exit 1
  }
  
  local build_dir="$PROGRAM_DIR/$program_name/target/deploy"
  local so_name="${program_name}.so"
  local keypair_name="${program_name}-keypair.json"

  # Deploy previous version
  if [[ "$network" == "mainnet" ]]; then
    echo "🔐 Executing mainnet multisig rollback..."
    solana program deploy \
      --buffer "$CACHE_DIR/${program_name}_buffer" \
      --fee-payer "$MULTISIG_KEYPAIR" \
      --keypair "$MULTISIG_KEYPAIR" \
      --cluster "$cluster" \
      --max-len 1073741824 \
      --override-auth \
      "$build_dir/$so_name" || {
        echo "❌ Deployment failed"
        git checkout -
        exit 1
      }
  else
    solana program deploy \
      --keypair "$HOME/.config/solana/id.json" \
      --cluster "$cluster" \
      --program-id "$build_dir/$keypair_name" \
      "$build_dir/$so_name" || {
        echo "❌ Deployment failed"
        git checkout -
        exit 1
      }
  fi

  # Verify deployment
  local deployed_id=$(solana program show --cluster $cluster $program_id | grep 'Program Id' | awk '{print \$3}')
  if [[ "$deployed_id" != "$old_program_id" ]]; then
    echo "❌ Program ID mismatch after rollback: $deployed_id != $old_program_id"
    git checkout -
    exit 1
  fi

  # Restore state if needed
  if [[ -f "$CACHE_DIR/${program_name}_state.snapshot" ]]; then
    echo "🔍 Restoring program state from snapshot..."
    solana program restore --cluster $cluster $old_program_id "$CACHE_DIR/${program_name}_state.snapshot" || {
      echo "⚠️ State restoration failed - manual intervention required"
    }
  fi

  git checkout -
  echo "✅ Successfully rolled back $program_name on $network"
}

# Main execution
main() {
  validate_environment
  
  local network=${1:-devnet}
  local program_name=${2:-all}
  
  check_authority "$network" "$program_name"
  
  if [[ "$program_name" == "all" ]]; then
    for program in model-nft training-market compute-oracle; do
      rollback_program "$network" "$program"
    done
  else
    rollback_program "$network" "$program_name"
  fi
}

main "$@"
