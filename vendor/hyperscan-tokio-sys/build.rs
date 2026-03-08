use std::env;
use std::path::PathBuf;
use std::process::Command;

fn check_chimera_availability() -> bool {
    // Check common locations for chimera headers
    let possible_paths = vec![
        "/usr/include/chimera/chimera.h",
        "/usr/local/include/chimera/chimera.h",
        "/opt/homebrew/include/chimera/chimera.h",
        "/opt/homebrew/opt/vectorscan/include/chimera/chimera.h",
    ];
    
    // Also check HYPERSCAN_ROOT/VECTORSCAN_ROOT
    if let Ok(root) = env::var("HYPERSCAN_ROOT").or_else(|_| env::var("VECTORSCAN_ROOT")) {
        if std::path::Path::new(&format!("{}/include/chimera/chimera.h", root)).exists() {
            return true;
        }
    }
    
    for path in possible_paths {
        if std::path::Path::new(path).exists() {
            return true;
        }
    }
    
    false
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    
    // Tell rustc about our custom cfg flags
    println!("cargo:rustc-check-cfg=cfg(has_chimera)");
    println!("cargo:rustc-check-cfg=cfg(arch_x86_64)");
    println!("cargo:rustc-check-cfg=cfg(arch_aarch64)");
    println!("cargo:rustc-check-cfg=cfg(has_avx2)");
    println!("cargo:rustc-check-cfg=cfg(has_avx512)");
    println!("cargo:rustc-check-cfg=cfg(has_neon)");
    println!("cargo:rustc-check-cfg=cfg(vectorscan_system)");
    
    // Detect CPU features at build time
    detect_cpu_features();
    
    // Determine linking strategy based on features
    if cfg!(feature = "system") {
        // User must have VectorScan/Hyperscan installed
        link_system_library();
    } else if cfg!(feature = "vendored") {
        // Download and build from source
        build_vectorscan();
    } else if cfg!(feature = "hyperscan") {
        // Use original Intel Hyperscan
        link_hyperscan();
    } else {
        // Default to system
        link_system_library();
    }
    
    // Check if Chimera feature is enabled
    if cfg!(feature = "chimera") {
        // Check if chimera headers are actually available
        let has_chimera = check_chimera_availability();
        if has_chimera {
            println!("cargo:rustc-cfg=has_chimera");
            println!("cargo:warning=Chimera support enabled - using system Chimera");
        } else {
            println!("cargo:warning=Chimera feature requested but chimera headers not found!");
            println!("cargo:warning=To use Chimera, you need to:");
            println!("cargo:warning=1. Build VectorScan from source with BUILD_CHIMERA=ON");
            println!("cargo:warning=2. Or use the 'vendored' feature to build from source");
            println!("cargo:warning=3. Or disable the chimera feature");
            if !cfg!(feature = "vendored") {
                panic!("Chimera headers not found. Use --features vendored,chimera to build from source");
            }
        }
    }
    
    generate_bindings();
}

fn link_system_library() {
    // Check for HYPERSCAN_ROOT or VECTORSCAN_ROOT environment variable
    if let Ok(root) = env::var("HYPERSCAN_ROOT").or_else(|_| env::var("VECTORSCAN_ROOT")) {
        println!("cargo:rustc-link-search={}/lib", root);
        println!("cargo:rustc-link-lib=hs");
        
        // If chimera feature is enabled, link against chimera and pcre
        if cfg!(feature = "chimera") {
            println!("cargo:rustc-link-lib=chimera");
            println!("cargo:rustc-link-lib=pcre");
        }
        
        println!("cargo:rustc-cfg=vectorscan_system");
        return;
    }
    
    // Try pkg-config first
    if pkg_config::probe_library("libhs").is_ok() {
        println!("cargo:rustc-cfg=vectorscan_system");
        
        // Check if Chimera is available when feature is not explicitly enabled
        if !cfg!(feature = "chimera") {
            if std::path::Path::new("/opt/homebrew/include/chimera/chimera.h").exists() ||
               std::path::Path::new("/usr/local/include/chimera/chimera.h").exists() ||
               std::path::Path::new("/usr/include/chimera/chimera.h").exists() {
                println!("cargo:warning=Chimera headers detected but chimera feature not enabled");
                println!("cargo:warning=To use PCRE patterns, add 'chimera' feature to your Cargo.toml");
            }
        }
        
        // If chimera feature is enabled, also link pcre
        if cfg!(feature = "chimera") {
            println!("cargo:rustc-link-lib=pcre");
        }
        
        return;
    }
    
    // Fallback to standard paths
    println!("cargo:rustc-link-lib=hs");
    
    if cfg!(feature = "chimera") {
        println!("cargo:rustc-link-lib=chimera");
        println!("cargo:rustc-link-lib=pcre");
    }
    
    // Add common library paths
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-search=/usr/local/lib");
        println!("cargo:rustc-link-search=/opt/homebrew/lib");
    } else if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-search=/usr/lib");
        println!("cargo:rustc-link-search=/usr/local/lib");
    }
}

fn link_hyperscan() {
    if cfg!(not(target_arch = "x86_64")) {
        panic!("Intel Hyperscan only supports x86_64 architecture");
    }
    
    // Similar to link_system_library but specifically for Hyperscan
    if pkg_config::probe_library("libhs").is_ok() {
        return;
    }
    
    println!("cargo:rustc-link-lib=hs");
}

fn detect_cpu_features() {
    // Detect architecture
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    
    match arch.as_str() {
        "x86_64" => {
            println!("cargo:rustc-cfg=arch_x86_64");
            
            // Check for specific features via compiler
            if cfg!(target_feature = "avx2") {
                println!("cargo:rustc-cfg=has_avx2");
            }
            if cfg!(target_feature = "avx512f") {
                println!("cargo:rustc-cfg=has_avx512");
            }
        }
        "aarch64" => {
            println!("cargo:rustc-cfg=arch_aarch64");
            if cfg!(target_feature = "neon") {
                println!("cargo:rustc-cfg=has_neon");
            }
        }
        _ => {}
    }
    
    // Print helpful build information
    println!("cargo:warning=Building for architecture: {}", arch);
}

fn build_vectorscan() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let build_dir = out_dir.join("build");
    
    // Clone or use vendored VectorScan
    let vectorscan_src = if let Ok(src) = env::var("VECTORSCAN_SRC") {
        PathBuf::from(src)
    } else {
        download_vectorscan()
    };
    
    // Check if we need to download PCRE for Chimera
    if cfg!(feature = "chimera") {
        ensure_pcre(&vectorscan_src);
    }
    
    // Configure with CMake
    let mut config = cmake::Config::new(&vectorscan_src);
    config
        .out_dir(&build_dir)
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_STATIC_LIBS", "ON")
        .define("FAT_RUNTIME", "ON"); // Multi-arch support
    
    // Enable Chimera if feature is set
    if cfg!(feature = "chimera") {
        config.define("BUILD_CHIMERA", "ON");
        println!("cargo:warning=Building VectorScan with Chimera support");
    }
    
    // Build
    let dst = config.build();
    
    // Link
    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-lib=static=hs");
    println!("cargo:rustc-link-lib=static=hs_runtime");
    
    if cfg!(feature = "chimera") {
        println!("cargo:rustc-link-lib=static=chimera");
        // PCRE is usually built as part of VectorScan when BUILD_CHIMERA=ON
        println!("cargo:rustc-link-lib=static=pcre");
    }
    
    // C++ stdlib
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=c++");
    } else {
        println!("cargo:rustc-link-lib=stdc++");
    }
}

fn download_vectorscan() -> PathBuf {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let vectorscan_dir = out_dir.join("vectorscan-src");
    
    // Skip if already downloaded
    if vectorscan_dir.exists() {
        return vectorscan_dir;
    }
    
    // Use a specific version for reproducibility
    const VECTORSCAN_VERSION: &str = "v5.4.11";
    
    println!("cargo:warning=Downloading VectorScan {} source...", VECTORSCAN_VERSION);
    
    // Method 1: Try git clone (most reliable)
    println!("cargo:warning=Cloning VectorScan {} source...", VECTORSCAN_VERSION);
    let status = Command::new("git")
        .args(&[
            "clone",
            "--depth", "1",
            "--branch", VECTORSCAN_VERSION,
            "https://github.com/VectorCamp/vectorscan.git",
            vectorscan_dir.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to execute git clone");
    
    if !status.success() {
        // Fallback: Try to download tarball with better error handling
        println!("cargo:warning=Git clone failed, trying direct download...");
        let url = format!(
            "https://github.com/VectorCamp/vectorscan/archive/refs/tags/{}.tar.gz",
            VECTORSCAN_VERSION
        );
        
        let tarball_path = out_dir.join("vectorscan.tar.gz");
        
        // Download using curl with better options
        let status = Command::new("curl")
            .args(&[
                "-L",              // Follow redirects
                "-f",              // Fail on HTTP errors
                "-s",              // Silent mode
                "-S",              // Show errors
                "-o", tarball_path.to_str().unwrap(),
                &url,
            ])
            .status()
            .expect("Failed to execute curl");
        
        if !status.success() {
            eprintln!("ERROR: Failed to download VectorScan source.");
            eprintln!("Please try one of these alternatives:");
            eprintln!("1. Install git: brew install git");
            eprintln!("2. Use system library: cargo build --no-default-features --features system");
            eprintln!("3. Download manually from: https://github.com/VectorCamp/vectorscan/releases");
            panic!("Failed to download VectorScan source");
        }
        
        // Extract tarball
        std::fs::create_dir_all(&vectorscan_dir).unwrap();
        let status = Command::new("tar")
            .args(&[
                "-xzf",
                tarball_path.to_str().unwrap(),
                "-C",
                vectorscan_dir.to_str().unwrap(),
                "--strip-components=1",
            ])
            .status()
            .expect("Failed to execute tar");
        
        if !status.success() {
            panic!("Failed to extract VectorScan source");
        }
        
        // Clean up tarball
        std::fs::remove_file(tarball_path).ok();
    }
    
    vectorscan_dir
}

/// Download and extract PCRE for Chimera support
fn ensure_pcre(vectorscan_src: &PathBuf) {
    let pcre_dir = vectorscan_src.join("pcre");
    
    // Skip if already exists
    if pcre_dir.exists() {
        println!("cargo:warning=PCRE directory already exists");
        return;
    }
    
    const PCRE_VERSION: &str = "8.45";
    let pcre_url = format!("https://ftp.pcre.org/pub/pcre/pcre-{}.tar.gz", PCRE_VERSION);
    
    println!("cargo:warning=Downloading PCRE {} for Chimera support...", PCRE_VERSION);
    
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let tarball_path = out_dir.join("pcre.tar.gz");
    
    // Download PCRE
    let status = Command::new("curl")
        .args(&[
            "-L",
            "-f",
            "-s",
            "-S",
            "-o", tarball_path.to_str().unwrap(),
            &pcre_url,
        ])
        .status()
        .expect("Failed to execute curl");
    
    if !status.success() {
        eprintln!("ERROR: Failed to download PCRE.");
        eprintln!("You can manually download PCRE from:");
        eprintln!("  wget {}", pcre_url);
        eprintln!("  tar xf pcre-{}.tar.gz -C {} --strip-components=1", PCRE_VERSION, pcre_dir.display());
        panic!("Failed to download PCRE for Chimera support");
    }
    
    // Create pcre directory
    std::fs::create_dir_all(&pcre_dir).unwrap();
    
    // Extract PCRE
    let status = Command::new("tar")
        .args(&[
            "-xzf",
            tarball_path.to_str().unwrap(),
            "-C",
            pcre_dir.to_str().unwrap(),
            "--strip-components=1",
        ])
        .status()
        .expect("Failed to execute tar");
    
    if !status.success() {
        panic!("Failed to extract PCRE source");
    }
    
    // Clean up
    std::fs::remove_file(tarball_path).ok();
    
    println!("cargo:warning=PCRE {} extracted successfully", PCRE_VERSION);
}

fn generate_bindings() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    
    // Create a conditional wrapper.h based on features AND availability
    let has_chimera = cfg!(feature = "chimera") && check_chimera_availability();
    
    let wrapper_content = if has_chimera {
        r#"#include <hs/hs.h>
#include <hs/hs_compile.h>
#include <hs/hs_runtime.h>
#include <chimera/chimera.h>
#include <chimera/ch_compile.h>
#include <chimera/ch_runtime.h>
"#
    } else {
        r#"#include <hs/hs.h>
#include <hs/hs_compile.h>
#include <hs/hs_runtime.h>
"#
    };
    
    let wrapper_path = out_path.join("wrapper.h");
    std::fs::write(&wrapper_path, wrapper_content).expect("Failed to write wrapper.h");
    
    let mut builder = bindgen::Builder::default()
        .header(wrapper_path.to_str().unwrap())
        .clang_arg("-I/usr/local/include")
        .clang_arg("-I/usr/include");
    
    // Add Homebrew paths for macOS
    if cfg!(target_os = "macos") {
        // M1 Macs use /opt/homebrew
        builder = builder
            .clang_arg("-I/opt/homebrew/include")
            .clang_arg("-I/opt/homebrew/opt/vectorscan/include");
        // Intel Macs use /usr/local
        builder = builder
            .clang_arg("-I/usr/local/opt/vectorscan/include");
    }
    
    // Add include path from HYPERSCAN_ROOT if set
    if let Ok(root) = env::var("HYPERSCAN_ROOT").or_else(|_| env::var("VECTORSCAN_ROOT")) {
        builder = builder.clang_arg(format!("-I{}/include", root));
    }
    
    builder = builder
        // Core Hyperscan types
        .allowlist_type("hs_.*")
        .allowlist_function("hs_.*")
        .allowlist_var("HS_.*")
        // Callbacks
        .allowlist_type("match_event_handler")
        // Ensure size_t is proper
        .size_t_is_usize(true);
    
    // Only include Chimera bindings if feature is enabled AND available
    if has_chimera {
        builder = builder
            // Chimera types and functions
            .allowlist_type("ch_.*")
            .allowlist_function("ch_.*")
            .allowlist_var("CH_.*")
            .allowlist_type("ch_capture_t");
    }
    
    let bindings = builder
        .generate()
        .expect("Unable to generate bindings");
    
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
