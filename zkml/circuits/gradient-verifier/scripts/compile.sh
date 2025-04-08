#!/usr/bin/env bash
set -eo pipefail

# Configuration
readonly SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
readonly ROOT_DIR="${SCRIPT_DIR}/.."
readonly BUILD_DIR="${ROOT_DIR}/.build"
readonly LOGS_DIR="${BUILD_DIR}/logs"
readonly PROGRAMS_DIR="${ROOT_DIR}/blockchain/programs"
readonly CIRCUITS_DIR="${ROOT_DIR}/zk/circuits"
readonly SDK_DIR="${ROOT_DIR}/js"
readonly ANCHOR_VERSION="0.29.0"
readonly SOLANA_CLI_VERSION="1.16.18"
readonly RUSTC_VERSION="1.72.0"

# Initialize environment
source "${SCRIPT_DIR}/_colors.sh"
source "${SCRIPT_DIR}/_utils.sh"

function main() {
  parse_args "$@"
  init_directories
  check_dependencies
  clean_artifacts
  compile_core
  compile_circuits
  compile_sdk
  verify_builds
  generate_artifacts
}

function parse_args() {
  while [[ $# -gt 0 ]]; do
    case \$1 in
      -n|--network)
        NETWORK="\$2"
        shift; shift ;;
      -j|--jobs)
        PARALLEL_JOBS="\$2"
        shift; shift ;;
      --skip-clean)
        SKIP_CLEAN=true ;;
      --skip-verify)
        SKIP_VERIFY=true ;;
      *)
        error "Unknown option: \$1"
        exit 1 ;;
    esac
  done

  export NETWORK=${NETWORK:-testnet}
  export BUILD_FEATURES="--features ${NETWORK}"
  export PARALLEL_JOBS=${PARALLEL_JOBS:-$(nproc)}
  export RUSTFLAGS="\
    -C link-arg=-zstack-size=32768 \
    -C link-arg=-Cretain-symbols=all \
    -C link-arg=--no-rosegment \
    -C link-arg=--no-entry \
    -C opt-level=3 \
    -C target-cpu=native"
}

function init_directories() {
  mkdir -p "${BUILD_DIR}" "${LOGS_DIR}"
  > "${LOGS_DIR}/compilation.log"
}

function check_dependencies() {
  header "Verifying build environment"
  
  check_tool "rustc" "${RUSTC_VERSION}" "--version | awk '{print \\$2}'"
  check_tool "solana" "${SOLANA_CLI_VERSION}" "--version | awk '{print \\$2}'"
  check_tool "anchor" "${ANCHOR_VERSION}" "--version | awk '{print \\$2}'"
  check_tool "node" "18.0.0" "--version | sed 's/[^0-9.]//g'"
  check_tool "circom" "2.1.6" "--version | awk '{print \\$3}'"
  check_tool "cargo-geiger" "0.11.5" "--version | awk '{print \\$2}'"
}

function clean_artifacts() {
  if [ "${SKIP_CLEAN}" = true ]; then
    warn "Skipping clean step"
    return
  fi

  header "Cleaning previous artifacts"
  
  cargo clean --manifest-path "${ROOT_DIR}/Cargo.toml" \
    >> "${LOGS_DIR}/compilation.log" 2>&1
  
  rm -rf \
    "${BUILD_DIR}/programs" \
    "${BUILD_DIR}/circuits" \
    "${BUILD_DIR}/sdk" \
    "${SDK_DIR}/dist" \
    "${SDK_DIR}/types"
}

function compile_core() {
  header "Compiling Solana programs"

  (
    cd "${PROGRAMS_DIR}"
    find . -maxdepth 1 -type d -name '*-*' -print0 | while IFS= read -r -d '' dir; do
      program_name=$(basename "${dir}")
      compile_program "${program_name}"
    done
  )
}

function compile_program() {
  local program_name=\$1
  local program_dir="${PROGRAMS_DIR}/${program_name}"
  local build_log="${LOGS_DIR}/${program_name}.log"
  
  info "Building ${program_name}"
  
  export CARGO_TARGET_DIR="${BUILD_DIR}/programs/${program_name}"
  
  (
    cd "${program_dir}"
    anchor build ${BUILD_FEATURES} \
      --arch bpf \
      --verifiable \
      -- \
      -j "${PARALLEL_JOBS}" \
      >> "${build_log}" 2>&1
  ) || error "Failed to build ${program_name}"
  
  analyze_program_build "${program_name}" "${build_log}"
}

function analyze_program_build() {
  local program_name=\$1
  local build_log=\$2
  
  # Security checks
  if grep -q 'unsafe[[:space:]]*{' "${build_log}"; then
    warn "Unsafe code detected in ${program_name}"
  fi
  
  # Size checks
  local sofile="${CARGO_TARGET_DIR}/bpfel-unknown-unknown/release/${program_name}.so"
  local size=$(stat -c%s "${sofile}")
  if (( size > 180000 )); then
    error "Program ${program_name} exceeds size limit (${size} bytes)"
    exit 1
  fi
}

function compile_circuits() {
  header "Compiling ZK Circuits"

  (
    cd "${CIRCUITS_DIR}"
    find . -name "*.circom" -print0 | while IFS= read -r -d '' file; do
      circuit_name=$(basename "${file}" .circom)
      compile_circuit "${circuit_name}"
    done
  )
}

function compile_circuit() {
  local name=\$1
  local circuit_dir="${CIRCUITS_DIR}/${name}"
  local build_dir="${BUILD_DIR}/circuits/${name}"
  local log_file="${LOGS_DIR}/${name}_circuit.log"
  
  info "Building ${name} circuit"
  
  mkdir -p "${build_dir}"
  
  circom "${circuit_dir}/${name}.circom" \
    --r1cs --wasm --sym --c \
    -o "${build_dir}" \
    --prime secq256k1 \
    --O3 \
    --inspect \
    >> "${log_file}" 2>&1 || error "Circuit ${name} compilation failed"
  
  optimize_circuit "${name}" "${build_dir}"
}

function optimize_circuit() {
  local name=\$1
  local build_dir=\$2
  
  info "Optimizing ${name} circuit"
  
  snarkjs r1cs export json "${build_dir}/${name}.r1cs" \
    "${build_dir}/${name}.json" \
    >> "${LOGS_DIR}/circuit_optimization.log" 2>&1
  
  node "${build_dir}/${name}_js/generate_witness.js" \
    "${build_dir}/${name}_js/${name}.wasm" \
    "${CIRCUITS_DIR}/${name}/input.json" \
    "${build_dir}/witness.wtns" \
    >> "${LOGS_DIR}/circuit_optimization.log" 2>&1
}

function compile_sdk() {
  header "Building TypeScript SDK"
  
  (
    cd "${SDK_DIR}"
    rm -rf node_modules
    npm ci --silent
    tsc -p tsconfig.json
    npm run build:types
  ) >> "${LOGS_DIR}/sdk.log" 2>&1 || error "SDK compilation failed"
}

function verify_builds() {
  if [ "${SKIP_VERIFY}" = true ]; then
    warn "Skipping verification step"
    return
  fi

  header "Verifying build artifacts"
  
  verify_programs
  verify_circuits
  verify_sdk
}

function verify_programs() {
  local ref_hash=$(git rev-parse HEAD:blockchain)
  
  find "${BUILD_DIR}/programs" -name "*.so" -print0 | while IFS= read -r -d '' file; do
    local program_hash=$(sha256sum "${file}" | awk '{print \$1}')
    local expected_hash=$(jq -r .programs.${program_name} "${ROOT_DIR}/hashes.json")
    
    if [ "${program_hash}" != "${expected_hash}" ]; then
      error "Hash mismatch for ${file}"
      exit 1
    fi
  done
}

function generate_artifacts() {
  header "Generating deployment artifacts"
  
  generate_idls
  package_release
  create_checksums
}

function generate_idls() {
  info "Generating program IDLs"
  
  find "${PROGRAMS_DIR}" -name "target" -type d -prune -o -name "*-idl.json" -print0 | while IFS= read -r -d '' file; do
    cp "${file}" "${BUILD_DIR}/idls"
  done
}

main "$@"
success "Build completed successfully"
