#!/usr/bin/env bash
set -eo pipefail

# Configuration
readonly SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
readonly KEYS_DIR="${SCRIPT_DIR}/../.keys"
readonly LOG_FILE="${SCRIPT_DIR}/keygen.log"
declare -A ENV_PATHS=(
  ["dev"]="${KEYS_DIR}/dev"
  ["test"]="${KEYS_DIR}/test" 
  ["prod"]="${KEYS_DIR}/prod"
)

# Initialize logging
exec > >(tee -a "$LOG_FILE") 2>&1

cleanup_temp_files() {
  find /tmp -name "umazen-key-*" -delete
}

validate_environment() {
  if ! command -v solana &> /dev/null; then
    echo "❌ Solana CLI required: brew install solana"
    exit 1
  fi
  
  if ! command -v ssh-keygen &> /dev/null; then
    echo "❌ SSH Keygen required"
    exit 1
  fi

  mkdir -p "$KEYS_DIR"
  chmod 700 "$KEYS_DIR"
}

generate_secure_keypair() {
  local env=\$1
  local key_type=\$2
  local key_path="${ENV_PATHS[$env]}/${key_type}"
  
  mkdir -p "$(dirname "$key_path")"
  umask 0177
  
  echo "🔐 Generating ${env} ${key_type} keypair..."
  solana-keygen new --no-bip39-passphrase --force --silent \
    --outfile "${key_path}.json" 2>&1 | tee -a "$LOG_FILE"
  
  local pubkey=$(solana-keygen pubkey "${key_path}.json")
  echo "${pubkey}" > "${key_path}.pub"
  
  # Generate SHA3-256 hash
  openssl dgst -sha3-256 "${key_path}.json" > "${key_path}.sha256"
  
  echo "✅ Generated ${key_type} key: ${pubkey}"
}

setup_multisig() {
  local env=\$1
  local signers=()
  
  read -p "Enter number of required signatures (M): " m
  read -p "Enter number of total signers (N): " n
  
  for ((i=1; i<=n; i++)); do
    read -p "Enter signer $i public key: " signer
    if [[ ! "$signer" =~ ^[1-9A-HJ-NP-Za-km-z]{32,44}$ ]]; then
      echo "❌ Invalid Solana public key: $signer"
      exit 1
    fi
    signers+=("$signer")
  done

  local multisig=$(solana-keygen new-multisig --threshold $m "${signers[@]}")
  echo "$multisig" > "${ENV_PATHS[$env]}/multisig.info"
  echo "🔏 Created ${m}/${n} Multisig: $multisig"
}

secure_backup() {
  local key_path=\$1
  local backup_target
  
  select backup_target in "USB" "Cloud" "Paper"; do
    case $backup_target in
      USB)
        if [[ -d "/Volumes/UMazenBackup" ]]; then
          cp "$key_path" "/Volumes/UMazenBackup/"
          echo "💾 Backed up to USB: /Volumes/UMazenBackup/"
        else
          echo "❌ USB drive not mounted"
          exit 1
        fi
        ;;
      Cloud)
        echo "⚠️ Use encrypted cloud storage manually!"
        exit 1
        ;;
      Paper)
        echo "🖨 Printing paper backup (QR + Base58)..."
        qrencode -t ASCII -o - < "$key_path"
        base58 "$key_path" | fold -w 50
        ;;
    esac
    break
  done
}

main() {
  trap cleanup_temp_files EXIT
  validate_environment
  
  local env
  PS3="Select environment: "
  select env in "dev" "test" "prod"; do
    [[ -n "$env" ]] && break
  done

  local key_type
  PS3="Select key type: "
  select key_type in "validator" "oracle" "multisig"; do
    [[ -n "$key_type" ]] && break
  done

  case $key_type in
    "multisig")
      setup_multisig "$env"
      ;;
    *)
      generate_secure_keypair "$env" "$key_type"
      read -p "Create secure backup? (y/N): " backup
      if [[ "$backup" == "y" ]]; then
        secure_backup "${ENV_PATHS[$env]}/${key_type}.json"
      fi
      ;;
  esac

  echo "🔑 Verification:"
  solana-keygen verify "${ENV_PATHS[$env]}/${key_type}.json"
}

main "$@"
