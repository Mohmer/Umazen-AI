#!/usr/bin/env bash
set -eo pipefail

# Configuration
readonly SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
readonly ROOT_DIR="${SCRIPT_DIR}/.."
readonly PROGRAMS_DIR="${ROOT_DIR}/blockchain/programs"
readonly NETWORK="${1:-mainnet-beta}"
readonly PROGRAM_NAME="${2:-model-nft}"
readonly KEYFILE="${3:-$HOME/.config/solana/mainnet-upgrade.json}"
readonly MULTISIG="${4:-}" # multisig PDA address
readonly BACKUP_DIR="${ROOT_DIR}/.backup/$(date +%s)"
readonly LOG_FILE="${ROOT_DIR}/.upgrade.log"
readonly ANCHOR_CMD="anchor build --verifiable"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Initialize upgrade environment
init_upgrade() {
  echo -e "${YELLOW}⚙️ Initializing upgrade environment...${NC}"
  mkdir -p "${BACKUP_DIR}"
  touch "${LOG_FILE}"
  export SOLANA_CLI_URL="https://api.${NETWORK}.solana.com"
  solana config set --url "${SOLANA_CLI_URL}" >> "${LOG_FILE}" 2>&1
  solana program show --program-id "${PROGRAM_ID}" > "${BACKUP_DIR}/old_program_info.json"
}

# Check upgrade dependencies
check_upgrade_deps() {
  local required=("solana" "anchor" "jq" "sha256sum")
  echo -e "${YELLOW}🔍 Checking upgrade dependencies...${NC}"
  
  for cmd in "${required[@]}"; do
    if ! command -v "${cmd}" >> "${LOG_FILE}" 2>&1; then
      echo -e "${RED}❌ Missing required dependency: ${cmd}${NC}"
      exit 1
    fi
  done
}

# Backup existing program state
backup_program() {
  echo -e "${YELLOW}📦 Backing up current program state...${NC}"
  local old_program_info=$(solana program show --program-id "${PROGRAM_ID}")
  
  # Save old program hash
  local old_hash=$(echo "${old_program_info}" | awk '/Buffer/ {getline; print \$1}')
  echo "${old_hash}" > "${BACKUP_DIR}/old_program.sha256"
  
  # Save old program buffer
  local old_buffer=$(echo "${old_program_info}" | grep "Buffer" | awk '{print \$2}')
  solana program dump "${old_buffer}" "${BACKUP_DIR}/old_program.so" >> "${LOG_FILE}" 2>&1
}

# Build new program version
build_program() {
  echo -e "${YELLOW}🏗️ Building new program version...${NC}"
  (
    cd "${PROGRAM_DIR}"
    ${ANCHOR_CMD} >> "${LOG_FILE}" 2>&1
    
    readonly NEW_SO_FILE="${PROGRAM_DIR}/target/deploy/${PROGRAM_NAME}.so"
    if [ ! -f "${NEW_SO_FILE}" ]; then
      echo -e "${RED}❌ Build failed: ${NEW_SO_FILE} not found${NC}"
      exit 1
    fi
    
    readonly NEW_HASH=$(sha256sum "${NEW_SO_FILE}" | awk '{print \$1}')
  )
}

# Deploy new program version
deploy_program() {
  echo -e "${YELLOW}🔄 Deploying program upgrade...${NC}"
  
  if [ -n "${MULTISIG}" ]; then
    # Multisig upgrade
    local upgrade_ix=$(solana program show --program-id "${PROGRAM_ID}" | grep "Upgradeable" | awk '{print \$3}')
    solana multisig create-upgrade "${MULTISIG}" "${PROGRAM_ID}" "${NEW_SO_FILE}" \
      --keypair "${KEYFILE}" >> "${LOG_FILE}" 2>&1
  else
    # Direct upgrade
    solana program deploy "${NEW_SO_FILE}" \
      --program-id "${PROGRAM_ID}" \
      --keypair "${KEYFILE}" \
      --upgrade-authority "${KEYFILE}" >> "${LOG_FILE}" 2>&1
  fi
}

# Verify program upgrade
verify_upgrade() {
  echo -e "${YELLOW}🔍 Verifying upgrade...${NC}"
  
  # Verify on-chain hash
  local chain_info=$(solana program show --program-id "${PROGRAM_ID}")
  local chain_hash=$(echo "${chain_info}" | awk '/Buffer/ {getline; print \$1}')
  
  if [ "${chain_hash}" != "${NEW_HASH}" ]; then
    echo -e "${RED}❌ Hash mismatch after upgrade!${NC}"
    echo -e "Local: ${NEW_HASH}"
    echo -e "Chain: ${chain_hash}"
    exit 1
  fi

  # Verify Anchor IDL
  (
    cd "${PROGRAM_DIR}"
    anchor verify "${PROGRAM_ID}" >> "${LOG_FILE}" 2>&1
  )
}

# Main upgrade process
main_upgrade() {
  readonly PROGRAM_DIR="${PROGRAMS_DIR}/${PROGRAM_NAME}"
  readonly PROGRAM_ID=$(grep "${PROGRAM_NAME} = \"" "${PROGRAM_DIR}/Anchor.toml" | cut -d '"' -f 2)
  
  check_upgrade_deps
  init_upgrade
  backup_program
  build_program
  deploy_program
  verify_upgrade

  echo -e "${GREEN}🎉 Upgrade successful! Backup saved to ${BACKUP_DIR}${NC}"
  echo -e "Old Hash: $(cat "${BACKUP_DIR}/old_program.sha256")"
  echo -e "New Hash: ${NEW_HASH}"
}

# Execute upgrade
main_upgrade
