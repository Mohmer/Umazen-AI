#!/usr/bin/env bash
set -eo pipefail

# Configuration
readonly SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
readonly ROOT_DIR="${SCRIPT_DIR}/.."
readonly PROGRAM_DIR="${ROOT_DIR}/blockchain/programs/umazen"
readonly DEPLOY_DIR="${ROOT_DIR}/.deploy"
readonly NETWORK=${1:-"localhost"}
readonly ENV_FILE="${ROOT_DIR}/.env.${NETWORK}"
readonly LOG_FILE="${DEPLOY_DIR}/deploy.log"
readonly ANCHOR_PROVIDER_URL="https://api.${NETWORK}.solana.com"
readonly ANCHOR_WALLET="${HOME}/.config/solana/id.json"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Initialize
init() {
  echo -e "${YELLOW}⚙️  Initializing deployment environment...${NC}"
  mkdir -p "${DEPLOY_DIR}"
  rm -f "${LOG_FILE}"
  touch "${LOG_FILE}"
  source "${ENV_FILE}" 2>/dev/null || true
  export ANCHOR_PROVIDER_URL
  export ANCHOR_WALLET
  export PATH="${HOME}/.cargo/bin:${PATH}"
}

# Check dependencies
check_dependencies() {
  local deps=("solana" "anchor" "rustc" "cargo" "npm" "node")
  echo -e "${YELLOW}🔍 Checking dependencies...${NC}"
  
  for dep in "${deps[@]}"; do
    if ! command -v "${dep}" >/dev/null 2>&1; then
      echo -e "${RED}❌ Error: ${dep} not found${NC}"
      exit 1
    fi
    echo -e "✅ ${dep} $( ${dep} --version 2>&1 )"
  done
}

# Build program
build_program() {
  echo -e "${YELLOW}🏗️  Building Solana program...${NC}"
  (
    cd "${PROGRAM_DIR}"
    anchor build --arch sbf --verifiable >>"${LOG_FILE}" 2>&1
    local build_hash=$(solana program dump -u l target/deploy/umazen.so | sha256sum)
    echo -e "✅ Build hash: ${build_hash:0:16}"
  ) || {
    echo -e "${RED}❌ Program build failed${NC}"
    exit 1
  }
}

# Deploy program
deploy_program() {
  echo -e "${YELLOW}🚀 Deploying to ${NETWORK}...${NC}"
  (
    cd "${PROGRAM_DIR}"
    local program_id=$(solana address -k target/deploy/umazen-keypair.json)
    
    echo -e "📄 Program ID: ${program_id}"
    echo -e "💳 Deployer: $(solana-keygen pubkey ${ANCHOR_WALLET})"
    
    anchor deploy --provider.cluster "${NETWORK}" \
      --provider.wallet "${ANCHOR_WALLET}" \
      --program-id "${program_id}" >>"${LOG_FILE}" 2>&1
    
    solana program show "${program_id}" | grep -q "Loaded" || {
      echo -e "${RED}❌ Deployment verification failed${NC}"
      exit 1
    }
    
    echo -e "✅ Program deployed successfully"
  ) || exit 1
}

# Run tests
run_tests() {
  echo -e "${YELLOW}🧪 Running tests...${NC}"
  (
    cd "${PROGRAM_DIR}"
    anchor test --skip-local-validator --skip-deploy --skip-build \
      --provider.cluster "${NETWORK}" \
      --provider.wallet "${ANCHOR_WALLET}" >>"${LOG_FILE}" 2>&1
    
    echo -e "✅ All tests passed"
  ) || {
    echo -e "${RED}❌ Tests failed${NC}"
    exit 1
  }
}

# Deploy frontend
deploy_frontend() {
  echo -e "${YELLOW}🖥️  Deploying frontend...${NC}"
  (
    cd "${ROOT_DIR}/frontend"
    npm ci --silent >>"${LOG_FILE}" 2>&1
    npm run build --silent >>"${LOG_FILE}" 2>&1
    aws s3 sync build/ s3://umazen-${NETWORK} --delete >>"${LOG_FILE}" 2>&1
    echo -e "✅ Frontend deployed to: https://${NETWORK}.umazen.io"
  ) || {
    echo -e "${RED}❌ Frontend deployment failed${NC}"
    exit 1
  }
}

# Start local validator
start_local_validator() {
  echo -e "${YELLOW}🏁 Starting local validator...${NC}"
  (
    solana-test-validator \
      --reset \
      --quiet \
      --bpf-program target/deploy/umazen-keypair.json target/deploy/umazen.so \
      --url "http://localhost:8899" >"${LOG_FILE}" 2>&1 &
    
    sleep 10
    solana config set --url "http://localhost:8899"
    solana airdrop 1000 $(solana-keygen pubkey) >>"${LOG_FILE}" 2>&1
    
    echo -e "✅ Local cluster ready: http://localhost:8899"
  ) || {
    echo -e "${RED}❌ Failed to start local validator${NC}"
    exit 1
  }
}

# Main deployment flow
main() {
  init
  check_dependencies
  
  case "${NETWORK}" in
    "localhost")
      build_program
      start_local_validator
      deploy_program
      run_tests
      ;;
    "devnet"|"testnet")
      build_program
      deploy_program
      run_tests
      deploy_frontend
      ;;
    "mainnet")
      read -p "⚠️ Confirm mainnet deployment (y/n)? " -n 1 -r
      echo
      if [[ $REPLY =~ ^[Yy]$ ]]; then
        build_program
        deploy_program
        deploy_frontend
      fi
      ;;
    *)
      echo -e "${RED}❌ Unknown network: ${NETWORK}${NC}"
      exit 1
      ;;
  esac

  echo -e "${GREEN}🎉 Deployment completed successfully!${NC}"
}

# Execute main
main
