#!/usr/bin/env bash
set -eo pipefail

# Configuration
readonly SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
readonly ROOT_DIR="${SCRIPT_DIR}/.."
readonly PROGRAMS_DIR="${ROOT_DIR}/blockchain/programs"
readonly DEPLOYED_PROGRAMS=(
  "model-nft"
  "training"
  "inference-market"
)
readonly NETWORK=${1:-"mainnet-beta"}
readonly LOG_FILE="${ROOT_DIR}/.verify.log"
readonly ANCHOR_TEST_CMD="anchor verify --provider-cluster ${NETWORK}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Initialize verification environment
init_verification() {
  echo -e "${YELLOW}⚙️ Initializing verification environment...${NC}"
  mkdir -p "${ROOT_DIR}/.anchor"
  rm -f "${LOG_FILE}"
  touch "${LOG_FILE}"
  export SOLANA_CLI_URL="https://api.${NETWORK}.solana.com"
  export PATH="${HOME}/.cargo/bin:${PATH}"
  solana config set --url "${SOLANA_CLI_URL}" >> "${LOG_FILE}" 2>&1
}

# Check verification dependencies
check_verification_deps() {
  local required=("solana" "anchor" "git" "sha256sum")
  echo -e "${YELLOW}🔍 Checking verification dependencies...${NC}"
  
  for cmd in "${required[@]}"; do
    if ! command -v "${cmd}" >> "${LOG_FILE}" 2>&1; then
      echo -e "${RED}❌ Missing required dependency: ${cmd}${NC}"
      exit 1
    fi
  done
}

# Build program and get build hash
build_program() {
  local program_dir="\$1"
  echo -e "${BLUE}🏗️ Building ${program_dir}...${NC}"
  
  (
    cd "${program_dir}"
    anchor build --arch sbf --verifiable >> "${LOG_FILE}" 2>&1
    
    local so_file="${program_dir}/target/deploy/$(basename ${program_dir}).so"
    if [ ! -f "${so_file}" ]; then
      echo -e "${RED}❌ Build failed: ${so_file} not found${NC}"
      exit 1
    fi
    
    BUILD_HASH=$(solana program dump -u l "${so_file}" | sha256sum | awk '{print \$1}')
    echo "${BUILD_HASH}"
  )
}

# Verify single program
verify_program() {
  local program_name="\$1"
  local program_dir="${PROGRAMS_DIR}/${program_name}"
  echo -e "${YELLOW}🔎 Verifying ${program_name}...${NC}"

  # Get program ID from Anchor.toml
  local program_id=$(grep "${program_name} = \"" "${program_dir}/Anchor.toml" | cut -d '"' -f 2)
  if [ -z "${program_id}" ]; then
    echo -e "${RED}❌ Program ID not found in Anchor.toml${NC}"
    exit 1
  fi

  # Build and get local hash
  local local_hash=$(build_program "${program_dir}")
  if [ $? -ne 0 ]; then
    exit 1
  fi

  # Get on-chain hash
  echo -e "${BLUE}⛓ Querying chain for program ${program_id}...${NC}"
  local chain_info=$(solana program show "${program_id}")
  if ! echo "${chain_info}" | grep -q "Program Id"; then
    echo -e "${RED}❌ Program not deployed: ${program_id}${NC}"
    exit 1
  fi

  local chain_hash=$(echo "${chain_info}" | awk '/Buffer/ {getline; print \$1}')
  if [ -z "${chain_hash}" ]; then
    echo -e "${RED}❌ Failed to get chain hash${NC}"
    exit 1
  fi

  # Compare hashes
  if [ "${local_hash}" != "${chain_hash}" ]; then
    echo -e "${RED}❌ Hash mismatch for ${program_name}${NC}"
    echo -e "Local:  ${local_hash}"
    echo -e "Chain:  ${chain_hash}"
    exit 1
  fi

  # Verify with Anchor
  echo -e "${BLUE}🔐 Anchor verification...${NC}"
  (
    cd "${program_dir}"
    ${ANCHOR_TEST_CMD} "${program_id}" >> "${LOG_FILE}" 2>&1
  )
  
  echo -e "${GREEN}✅ Verified ${program_name} (${program_id})${NC}"
  echo -e "Hash: ${local_hash}"
}

# Main verification process
main_verification() {
  init_verification
  check_verification_deps

  echo -e "${YELLOW}🔗 Network: ${SOLANA_CLI_URL}${NC}"
  echo -e "${YELLOW}📋 Programs to verify: ${DEPLOYED_PROGRAMS[*]}${NC}"

  for program in "${DEPLOYED_PROGRAMS[@]}"; do
    verify_program "${program}" || exit 1
  done

  echo -e "${GREEN}🎉 All programs verified successfully!${NC}"
}

# Execute verification
main_verification
