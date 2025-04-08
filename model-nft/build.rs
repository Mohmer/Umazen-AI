//! Umazen Build Script - Cross-platform compilation with Solana BPF constraints

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Environment Validation
    check_required_tools(&["solana", "llvm-config", "rustc", "cargo"])?;
    verify_solana_version("1.16.6")?;
    check_llvm_version(15)?;

    // 2. BPF Target Configuration
    let target = "bpfel-unknown-unknown";
    init_bpf_target(target)?;

    // 3. Feature-based Code Generation
    generate_feature_flags();
    generate_zkp_constraints()?;

    // 4. Dynamic Library Linking
    setup_cuda_linking()?;
    setup_zk_accelerators()?;

    // 5. Security Hardening
    inject_stack_protection();
    enable_overflow_checks();

    // 6. Performance Optimization
    configure_lto();
    setup_pgo_profiling()?;

    // 7. Metadata Generation
    generate_build_info()?;

    Ok(())
}

// --- Implementation Details ---

/// Verify presence of required CLI tools
fn check_required_tools(tools: &[&str]) -> Result<(), String> {
    for tool in tools {
        let status = Command::new(tool)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| format!("{} not found in PATH", tool))?;

        if !status.success() {
            return Err(format!("{} failed version check", tool));
        }
    }
    Ok(())
}

/// Validate Solana CLI version
fn verify_solana_version(required: &str) -> Result<(), String> {
    let output = Command::new("solana")
        .arg("--version")
        .output()
        .map_err(|e| format!("solana CLI error: {}", e))?;

    let version_str = String::from_utf8_lossy(&output.stdout);
    let installed = version_str.split_whitespace().nth(1).unwrap_or("");

    if installed != required {
        return Err(format!(
            "Solana version mismatch: required {}, found {}",
            required, installed
        ));
    }
    Ok(())
}

/// Ensure LLVM toolchain compatibility
fn check_llvm_version(min_version: u32) -> Result<(), String> {
    let output = Command::new("llvm-config")
        .arg("--version")
        .output()
        .map_err(|e| format!("llvm-config error: {}", e))?;

    let version_str = String::from_utf8_lossy(&output.stdout);
    let installed_version = version_str
        .split('.')
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);

    if installed_version < min_version {
        return Err(format!(
            "LLVM version >= {} required, found {}",
            min_version, installed_version
        ));
    }
    Ok(())
}

/// Initialize BPF target configuration
fn init_bpf_target(target: &str) -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = env::var("OUT_DIR")?;
    let target_dir = Path::new(&out_dir).join("bpf-target");
    
    fs::create_dir_all(&target_dir)?;
    
    // Generate target specification file
    let target_spec = format!(
        r#"{{
            "llvm-target": "bpf",
            "data-layout": "e-m:e-p:64:64-i64:64-n32:64-S128",
            "arch": "bpf",
            "os": "solana",
            "executables": true,
            "linker": "llvm-link",
            "linker-flavor": "ld.lld",
            "panic-strategy": "abort",
            "disable-redzone": true,
            "features": "+solana"
        }}"#
    );
    
    fs::write(target_dir.join("bpf.json"), target_spec)?;
    
    // Set environment variables
    println!("cargo:rustc-target={}", target);
    println!("cargo:rustc-env=TARGET={}", target);
    println!("cargo:rustc-link-arg=--target={}", target);
    
    Ok(())
}

/// Generate ZKP constraint files
fn generate_zkp_constraints() -> Result<(), Box<dyn std::error::Error>> {
    let zk_dir = Path::new("zkml").join("constraints");
    if zk_dir.exists() {
        let output = Command::new("circom")
            .arg("--r1cs")
            .arg("--wasm")
            .arg("--sym")
            .arg(zk_dir.join("main.circom"))
            .output()?;
        
        if !output.status.success() {
            return Err(format!(
                "Circom compilation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ).into());
        }
    }
    Ok(())
}

/// Configure CUDA linking if feature enabled
fn setup_cuda_linking() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("CARGO_FEATURE_CUDA").is_ok() {
        let cuda_path = Path::new("/usr/local/cuda")
            .canonicalize()
            .or_else(|_| find_cuda_library())?;

        println!("cargo:rustc-link-search={}/lib64", cuda_path.display());
        println!("cargo:rustc-link-lib=cudart");
        println!("cargo:rustc-link-lib=cublas");
    }
    Ok(())
}

/// Detect CUDA installation paths
fn find_cuda_library() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let possible_paths = &[
        "/usr/local/cuda-12.2",
        "/opt/cuda",
        "C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.2",
    ];

    for path in possible_paths {
        let p = Path::new(path);
        if p.join("lib64").exists() {
            return Ok(p.to_path_buf());
        }
    }
    Err("CUDA library not found".into())
}

/// Security hardening configurations
fn inject_stack_protection() {
    println!("cargo:rustc-cdylib-link-arg=-Wl,-z,stack-size=4194304"); // 4MB stack
    println!("cargo:rustc-link-arg=-fstack-protector-strong");
}

fn enable_overflow_checks() {
    println!("cargo:rustc-rustflags=-Coverflow-checks=yes");
}

/// LTO configuration
fn configure_lto() {
    if env::var("PROFILE").unwrap() == "release" {
        println!("cargo:rustc-rustflags=-Clto=fat");
        println!("cargo:rustc-rustflags=-Cembed-bitcode=yes");
    }
}

/// Setup PGO instrumentation
fn setup_pgo_profiling() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("CARGO_FEATURE_PGO").is_ok() {
        let pgo_dir = Path::new("target").join("pgo");
        fs::create_dir_all(&pgo_dir)?;

        println!("cargo:rustc-env=LLVM_PROFILE_FILE={}/cargo-test-%p-%m.profraw", pgo_dir.display());
        println!("cargo:rustc-rustflags=-Cprofile-generate={}", pgo_dir.display());
    }
    Ok(())
}

/// Generate build metadata
fn generate_build_info() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()?;
    
    let git_hash = if output.status.success() {
        String::from_utf8(output.stdout)?.trim().to_string()
    } else {
        "unknown".to_string()
    };

    let build_info = format!(
        r#"// Auto-generated build info
        pub const BUILD_TIMESTAMP: &str = "{}";
        pub const GIT_COMMIT_HASH: &str = "{}";
        pub const RUSTC_VERSION: &str = "{}";
        "#,
        chrono::Utc::now().to_rfc3339(),
        git_hash,
        rustc_version()
    );

    let out_path = PathBuf::from(env::var("OUT_DIR")?).join("build_info.rs");
    fs::write(out_path, build_info)?;

    Ok(())
}

/// Get rustc version
fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}
