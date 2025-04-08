#!/usr/bin/env bash
set -eo pipefail

# Configuration
readonly SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
readonly CIRCUITS_DIR="${SCRIPT_DIR}/../zk/circuits"
readonly TEST_DIR="${SCRIPT_DIR}/../.test"
readonly LOGS_DIR="${TEST_DIR}/logs"
readonly REPORTS_DIR="${TEST_DIR}/reports"
readonly ARTIFACTS_DIR="${TEST_DIR}/artifacts"
readonly PROFILE_DATA="${TEST_DIR}/coverage.profdata"

# Test Parameters
declare -A TIMEOUTS=(
  [compile]=120
  [witness]=60
  [prove]=300
  [verify]=30
  [negative]=90
)

export RUST_MIN_STACK=8388608 # 8MB stack for recursion
export CIRCUIT_OPT_LEVEL="--O3 --prime secq256k1"
export PROVING_SCHEME="groth16"
export PTAU_HASH="6548b0cce3ae03442d94a694bdf5165b" # phase1.ptau verification hash

function main() {
  parse_args "$@"
  init_environment
  check_dependencies
  run_test_suite
  generate_reports
}

function parse_args() {
  while [[ $# -gt 0 ]]; do
    case \$1 in
      -c|--circuit)
        CIRCUIT_NAME="\$2"
        shift; shift ;;
      -j|--jobs)
        PARALLEL_JOBS="\$2"
        shift; shift ;;
      --skip-clean)
        SKIP_CLEAN=true ;;
      --generate-vectors)
        GENERATE_VECTORS=true ;;
      *)
        error "Invalid option: \$1"
        exit 1 ;;
    esac
  done

  CIRCUIT_NAME=${CIRCUIT_NAME:-zkml}
  PARALLEL_JOBS=${PARALLEL_JOBS:-$(nproc)}
}

function init_environment() {
  mkdir -p "${TEST_DIR}" "${LOGS_DIR}" "${REPORTS_DIR}" "${ARTIFACTS_DIR}"
  > "${LOGS_DIR}/test.log"
  export CIRCUIT_PATH="${CIRCUITS_DIR}/${CIRCUIT_NAME}"
}

function check_dependencies() {
  header "Checking Test Dependencies"
  
  check_tool "circom" "2.1.6" "--version | awk '{print \\$3}'"
  check_tool "snarkjs" "0.7.0" "--version | awk '{print \\$2}'"
  check_tool "node" "18.0.0" "--version | sed 's/[^0-9.]//g'"
  check_tool "timeout" "9.0" "--version | head -1 | awk '{print \\$4}'"
}

function run_test_suite() {
  local start_time=$(date +%s)
  
  clean_artifacts
  compile_circuit
  generate_trusted_setup
  run_positive_tests
  run_negative_tests
  run_edge_cases
  run_performance_tests
  run_security_checks
  
  local duration=$(($(date +%s) - start_time))
  info "Test suite completed in ${duration}s"
}

function compile_circuit() {
  header "Compiling ${CIRCUIT_NAME} Circuit"
  
  local build_log="${LOGS_DIR}/compile.log"
  
  timeout ${TIMEOUTS[compile]} circom "${CIRCUIT_PATH}/${CIRCUIT_NAME}.circom" \
    --r1cs --wasm --sym --c \
    -o "${ARTIFACTS_DIR}" \
    ${CIRCUIT_OPT_LEVEL} \
    > "${build_log}" 2>&1 || test_failed "Compilation failed"
  
  analyze_compilation "${build_log}"
}

function analyze_compilation() {
  local log_file=\$1
  
  # Check constraints count
  local constraints=$(grep "Final Number of constraints" "${log_file}" | awk '{print $NF}')
  if [[ "$constraints" -gt 5000000 ]]; then
    warn "High constraint count: ${constraints}"
  fi
  
  # Check for warnings
  if grep -q "warning" "${log_file}"; then
    warn "Compiler warnings detected"
    grep --color=auto "warning" "${log_file}"
  fi
}

function generate_trusted_setup() {
  header "Generating Trusted Setup"
  
  local phase1="${ARTIFACTS_DIR}/phase1.ptau"
  local circuit_r1cs="${ARTIFACTS_DIR}/${CIRCUIT_NAME}.r1cs"
  
  # Download phase1 if not exists
  if [[ ! -f "${phase1}" ]]; then
    curl -sSfL https://hermez.s3-eu-west-1.amazonaws.com/powersOfTau28_hez_final_22.ptau \
      -o "${phase1}" || test_failed "Phase1 download failed"
  fi
  
  # Verify phase1 integrity
  local downloaded_hash=$(md5sum "${phase1}" | awk '{print \$1}')
  [[ "$downloaded_hash" == "$PTAU_HASH" ]] || test_failed "Phase1 file corrupted"

  # Generate zkey
  snarkjs ${PROVING_SCHEME} setup "${circuit_r1cs}" "${phase1}" \
    "${ARTIFACTS_DIR}/${CIRCUIT_NAME}_0000.zkey" >> "${LOGS_DIR}/setup.log" 2>&1
  
  # Contribute to ceremony
  echo "test entropy" | snarkjs zkey contribute \
    "${ARTIFACTS_DIR}/${CIRCUIT_NAME}_0000.zkey" \
    "${ARTIFACTS_DIR}/${CIRCUIT_NAME}.zkey" >> "${LOGS_DIR}/contribute.log" 2>&1
  
  # Export verification key
  snarkjs zkey export verificationkey "${ARTIFACTS_DIR}/${CIRCUIT_NAME}.zkey" \
    "${ARTIFACTS_DIR}/verification_key.json" >> "${LOGS_DIR}/export.log" 2>&1
}

function run_positive_tests() {
  header "Running Positive Test Cases"
  
  local test_cases=($(find "${CIRCUIT_PATH}/tests/positive" -name "input_*.json"))
  local test_count=${#test_cases[@]}
  local passed=0
  
  parallel -j "${PARALLEL_JOBS}" --bar --joblog "${LOGS_DIR}/parallel.log" \
    "process_test_case ::: ${test_cases[*]}"
  
  # Parse results
  passed=$(grep -c "TEST PASSED" "${LOGS_DIR}/positive.log")
  report_metric "positive" "${passed}" "${test_count}"
}

function process_test_case() {
  local input_file=\$1
  local case_name=$(basename "${input_file}" .json)
  local output_dir="${ARTIFACTS_DIR}/${case_name}"
  local result_file="${REPORTS_DIR}/${case_name}.txt"
  
  mkdir -p "${output_dir}"
  
  (
    generate_witness "${input_file}" "${output_dir}"
    generate_proof "${output_dir}"
    verify_proof "${output_dir}"
    echo "TEST PASSED: ${case_name}" >> "${LOGS_DIR}/positive.log"
  ) || echo "TEST FAILED: ${case_name}" >> "${LOGS_DIR}/positive.log"
  
  # Save artifacts for failed tests
  if [[ $? -ne 0 ]]; then
    tar czf "${REPORTS_DIR}/${case_name}_debug.tar.gz" "${output_dir}"
  fi
}

function generate_witness() {
  local input_file=\$1
  local output_dir=\$2
  
  timeout ${TIMEOUTS[witness]} node \
    "${ARTIFACTS_DIR}/${CIRCUIT_NAME}_js/generate_witness.js" \
    "${ARTIFACTS_DIR}/${CIRCUIT_NAME}_js/${CIRCUIT_NAME}.wasm" \
    "${input_file}" \
    "${output_dir}/witness.wtns" >> "${LOGS_DIR}/witness.log" 2>&1
}

function generate_proof() {
  local output_dir=\$1
  
  timeout ${TIMEOUTS[prove]} snarkjs ${PROVING_SCHEME} prove \
    "${ARTIFACTS_DIR}/${CIRCUIT_NAME}.zkey" \
    "${output_dir}/witness.wtns" \
    "${output_dir}/proof.json" \
    "${output_dir}/public.json" >> "${LOGS_DIR}/prove.log" 2>&1
}

function verify_proof() {
  local output_dir=\$1
  
  timeout ${TIMEOUTS[verify]} snarkjs ${PROVING_SCHEME} verify \
    "${ARTIFACTS_DIR}/verification_key.json" \
    "${output_dir}/public.json" \
    "${output_dir}/proof.json" >> "${LOGS_DIR}/verify.log" 2>&1
}

function run_negative_tests() {
  header "Running Negative Test Cases"
  
  local test_cases=($(find "${CIRCUIT_PATH}/tests/negative" -name "input_*.json"))
  local test_count=${#test_cases[@]}
  local passed=0
  
  for case in "${test_cases[@]}"; do
    local case_name=$(basename "${case}" .json)
    local output_dir="${ARTIFACTS_DIR}/${case_name}"
    
    mkdir -p "${output_dir}"
    
    (generate_witness "${case}" "${output_dir}" 2>/dev/null) && {
      echo "TEST FAILED: ${case_name}" >> "${LOGS_DIR}/negative.log"
    } || {
      echo "TEST PASSED: ${case_name}" >> "${LOGS_DIR}/negative.log"
      ((passed++))
    }
  done
  
  report_metric "negative" "${passed}" "${test_count}"
}

function run_edge_cases() {
  header "Testing Edge Cases"
  
  test_zero_values
  test_overflow_conditions
  test_boundary_limits
}

function test_zero_values() {
  local input="${CIRCUIT_PATH}/tests/edge/zero_values.json"
  local output_dir="${ARTIFACTS_DIR}/zero_values"
  
  ( process_test_case "${input}" ) || {
    warn "Zero value test failed"
    return 1
  }
}

function test_overflow_conditions() {
  local input="${CIRCUIT_PATH}/tests/edge/overflow.json"
  local output_dir="${ARTIFACTS_DIR}/overflow"
  
  ( generate_witness "${input}" "${output_dir}" ) && {
    error "Overflow test failed - accepted invalid input"
    return 1
  }
}

function test_boundary_limits() {
  local input="${CIRCUIT_PATH}/tests/edge/boundary.json"
  local output_dir="${ARTIFACTS_DIR}/boundary"
  
  ( process_test_case "${input}" ) || {
    warn "Boundary limit test failed"
    return 1
  }
}

function run_performance_tests() {
  header "Running Performance Benchmarks"
  
  benchmark_witness_generation
  benchmark_proof_generation
  benchmark_proof_verification
  measure_memory_usage
}

function benchmark_witness_generation() {
  local input="${CIRCUIT_PATH}/tests/benchmark/input.json"
  local output_dir="${ARTIFACTS_DIR}/bench_witness"
  
  time_cmd "witness_gen" \
    generate_witness "${input}" "${output_dir}"
}

function benchmark_proof_generation() {
  local output_dir="${ARTIFACTS_DIR}/bench_prove"
  
  time_cmd "proof_gen" \
    generate_proof "${output_dir}"
}

function benchmark_proof_verification() {
  local output_dir="${ARTIFACTS_DIR}/bench_verify"
  
  time_cmd "proof_verify" \
    verify_proof "${output_dir}"
}

function time_cmd() {
  local metric=\$1
  shift
  
  local start=$(date +%s.%N)
  "$@"
  local end=$(date +%s.%N)
  
  local runtime=$(echo "$end - $start" | bc)
  echo "${metric}: ${runtime}s" >> "${REPORTS_DIR}/performance.txt"
}

function measure_memory_usage() {
  local output_dir="${ARTIFACTS_DIR}/memory_test"
  
  /usr/bin/time -v -o "${REPORTS_DIR}/memory.log" \
    node "${ARTIFACTS_DIR}/${CIRCUIT_NAME}_js/generate_witness.js" \
    "${ARTIFACTS_DIR}/${CIRCUIT_NAME}_js/${CIRCUIT_NAME}.wasm" \
    "${CIRCUIT_PATH}/tests/benchmark/input.json" \
    "${output_dir}/witness.wtns"
}

function run_security_checks() {
  header "Performing Security Audits"
  
  check_constant_branching
  check_unconstrained_signals
  check_side_channels
}

function check_constant_branching() {
  if grep -qr "component main[[:space:]]*=" "${CIRCUIT_PATH}" \
    | grep -E "if[[:space:]]*\(1"; then
    error "Constant branching detected"
    return 1
  fi
}

function check_unconstrained_signals() {
  local r1cs_info="${REPORTS_DIR}/r1cs_analysis.txt"
  
  snarkjs r1cs info "${ARTIFACTS_DIR}/${CIRCUIT_NAME}.r1cs" > "${r1cs_info}"
  
  local n_pub=$(awk '/Public Inputs:/ {print \$3}' "${r1cs_info}")
  local n_priv=$(awk '/Private Inputs:/ {print \$3}' "${r1cs_info}")
  
  if [[ "$n_priv" -gt $((n_pub * 2)) ]]; then
    warn "High private/public signal ratio: ${n_priv}/${n_pub}"
  fi
}

function check_side_channels() {
  if strings "${ARTIFACTS_DIR}/${CIRCUIT_NAME}_js/${CIRCUIT_NAME}.wasm" | grep -q "secret"; then
    error "Potential secret data in WASM"
    return 1
  fi
}

function generate_reports() {
  header "Generating Test Reports"
  
  generate_coverage_report
  generate_summary_report
  package_artifacts
}

function generate_coverage_report() {
  if [[ "$GENERATE_VECTORS" == "true" ]]; then
    info "Generating test vectors"
    python3 "${SCRIPT_DIR}/gen_vectors.py" \
      --circuit "${CIRCUIT_NAME}" \
      --output "${TEST_DIR}/vectors"
  fi
  
  # Coverage analysis using llvm-cov
  if command -v llvm-cov >/dev/null; then
    llvm-cov report \
      --instr-profile="${PROFILE_DATA}" \
      --object="${ARTIFACTS_DIR}/${CIRCUIT_NAME}.wasm" \
      > "${REPORTS_DIR}/coverage.txt"
  fi
}

function generate_summary_report() {
  local total_tests=$(($(wc -l < "${LOGS_DIR}/positive.log") + $(wc -l < "${LOGS_DIR}/negative.log")))
  local passed_tests=$(($(grep -c "PASSED" "${LOGS_DIR}/positive.log") + $(grep -c "PASSED" "${LOGS_DIR}/negative.log")))
  
  cat > "${REPORTS_DIR}/summary.md" <<EOF
# Test Summary Report

**Circuit**: ${CIRCUIT_NAME}
**Date**: $(date +%Y-%m-%d)

## Metrics
- **Total Tests**: ${total_tests}
- **Passed**: ${passed_tests}
- **Failed**: $((total_tests - passed_tests))

## Performance
$(cat "${REPORTS_DIR}/performance.txt")

## Security Findings
$(grep -hE "WARN|ERROR" "${LOGS_DIR}"/*.log | sed 's/^/- /')

EOF
}

function package_artifacts() {
  tar czf "${TEST_DIR}/${CIRCUIT_NAME}_test_artifacts.tar.gz" \
    "${ARTIFACTS_DIR}" "${REPORTS_DIR}" \
    --exclude "*.wasm" --exclude "*.zkey"
}

function clean_artifacts() {
  if [[ "$SKIP_CLEAN" == "true" ]]; then
    warn "Skipping artifact cleanup"
    return
  fi
  
  rm -rf "${ARTIFACTS_DIR:?}"/*
  find "${LOGS_DIR}" -type f -name "*.log" -exec truncate -s 0 {} \;
}

function test_failed() {
  error "\$1"
  exit 2
}

# Utility functions
source "${SCRIPT_DIR}/_test_utils.sh"

main "$@"
