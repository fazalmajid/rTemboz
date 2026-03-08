# hyperscan-tokio

[![Crates.io](https://img.shields.io/crates/v/hyperscan-tokio.svg)](https://crates.io/crates/hyperscan-tokio)
[![Documentation](https://docs.rs/hyperscan-tokio/badge.svg)](https://docs.rs/hyperscan-tokio)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

Blazing-fast multi-pattern regex matching for Rust, bringing the power of [Vectorscan](https://github.com/VectorCamp/vectorscan) (Intel Hyperscan's modern fork) to the async ecosystem.

## Why hyperscan-tokio?

- **🚀 Extreme Performance**: Scan gigabytes per second using SIMD acceleration
- **🔍 Multi-Pattern Matching**: Compile thousands of patterns into a single automaton
- **⚡ Async-First**: Built for Tokio from the ground up
- **🎯 Zero-Copy**: Minimal allocations with `Bytes` integration
- **🔄 Hot Reloading**: Swap pattern databases without downtime
- **🛡️ Production Ready**: Battle-tested VectorScan engine with safe Rust API

## Performance

```
scanning_throughput/1024        time:   [2.1 µs 2.2 µs 2.3 µs]
                                thrpt:  [434 MiB/s 455 MiB/s 476 MiB/s]

scanning_throughput/1048576     time:   [45.2 µs 45.8 µs 46.4 µs]
                                thrpt:  [21.5 GiB/s 21.9 GiB/s 22.1 GiB/s]

vs_alternatives/hyperscan_tokio time:   [12.3 µs 12.5 µs 12.7 µs]
vs_alternatives/rust_regex      time:   [891.2 µs 895.7 µs 900.1 µs]
                                71.6x faster than regex crate for multi-pattern matching
```

## Quick Start

```rust
use hyperscan_tokio::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Compile patterns into a database
    let db = DatabaseBuilder::new()
        .add_pattern(Pattern::new(r"\b\d{3}-\d{2}-\d{4}\b").id(1))  // SSN
        .add_pattern(Pattern::new(r"\b4\d{15}\b").id(2))           // Credit card
        .add_pattern(Pattern::new(r"(?i)password:\s*\S+").id(3))   // Passwords
        .build()?;
    
    // Create scanner
    let scanner = Scanner::new(db)?;
    
    // Scan data
    let data = b"SSN: 123-45-6789, CC: 4111111111111111";
    let matches = scanner.scan_bytes(data).await?;
    
    for m in matches {
        println!("Pattern {} matched at [{}, {})", m.pattern_id, m.start, m.end);
    }
    
    Ok(())
}
```

## Chimera Support (PCRE with Capture Groups)

hyperscan-tokio offers two ways to use PCRE-compatible patterns with capture groups:

### Option 1: Rust-native implementation (Recommended)
Uses the `regex` crate internally, no special system dependencies:

```toml
hyperscan-tokio = { version = "0.1", features = ["chimera"] }
```

### Option 2: FFI-based Chimera
Requires VectorScan built with Chimera support:

```toml
# Build from source automatically
hyperscan-tokio = { version = "0.1", features = ["vendored", "chimera-ffi"] }

# Or use system VectorScan with Chimera (requires manual build)
hyperscan-tokio = { version = "0.1", features = ["system", "chimera-ffi"] }
```

**⚠️ Important**: Standard Homebrew/package manager installations of VectorScan do NOT include Chimera. See [CHIMERA_SETUP.md](./CHIMERA_SETUP.md) for detailed setup instructions.

## Advanced Features

### Worker Pool for Maximum Throughput

```rust
let pool = WorkerPool::builder()
    .num_workers(16)
    .core_affinity(true)  // Pin workers to CPU cores
    .build(db)?;

// Process millions of log lines
let jobs: Vec<ScanJob> = log_lines.into_iter()
    .map(|line| ScanJob { id: line.id, data: line.into() })
    .collect();

let results = pool.scan_batch(jobs).await?;
```

### Hot-Reloadable Patterns

```rust
let reloadable = ReloadableDatabase::new(db);

// In another task, reload patterns without stopping scanning
tokio::spawn(async move {
    let new_db = load_new_patterns().await?;
    reloadable.reload(new_db).await?;
});
```

### Streaming Mode

```rust
let stream_scanner = StreamScanner::new(db)?;

// Scan data as it arrives
let match_stream = stream_scanner
    .scan_stream(tcp_stream)
    .await?;

while let Some(m) = match_stream.next().await {
    process_match(m?);
}
```

## Use Cases

- **Log Analysis**: Scan millions of log entries per second for security patterns
- **Data Loss Prevention**: Find sensitive data in real-time streams
- **Web Application Firewall**: Match attack patterns at line speed
- **Content Filtering**: High-performance content moderation
- **Network Security**: Deep packet inspection at 10Gbps+

## Building from Source

Requires VectorScan development files:

```bash
# Ubuntu/Debian
sudo apt-get install libhyperscan-dev cmake

# macOS
brew install vectorscan cmake

# Build
cargo build --release
```

## Architecture

```
hyperscan-tokio
├── hyperscan-tokio-sys/    # Low-level FFI bindings
│   └── Safe wrappers around VectorScan C API
└── src/                    # High-level async Rust API
    ├── scanner.rs          # Core scanning functionality
    ├── database.rs         # Pattern compilation
    ├── worker_pool.rs      # Parallel scanning
    └── stream.rs           # Streaming mode
```

## Benchmarks

Run the benchmarks:

```bash
cargo bench
```

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) for details.

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [VectorScan](https://github.com/VectorCamp/vectorscan) - The underlying regex engine
- [Intel Hyperscan](https://github.com/intel/hyperscan) - The original project
- The Rust community for excellent async ecosystem

---

Built with ❤️ for the Rust community. Making enterprise-grade regex performance accessible to everyone.
