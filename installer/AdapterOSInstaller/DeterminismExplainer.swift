//
//  DeterminismExplainer.swift
//  AdapterOSInstaller
//
//  Content and logic for determinism explanation
//

import Foundation

struct DeterminismExplainer {
    static let content = """
# AdapterOS: Deterministic AI Execution

AdapterOS runs bit-reproducible AI workloads. Every inference is cryptographically verified.

## What This Means

- Same input + same model = identical output, every time
- Full audit trail of every inference decision
- Cryptographic proof of computation integrity

## How It Works

1. **Deterministic Kernels**: All kernels compiled to deterministic Metal bytecode
2. **Seeded RNG**: Random number generation seeded from HKDF derivation
3. **Fixed Floating-Point**: Floating-point modes locked at kernel launch
4. **Event Hashing**: Event hashes form Merkle tree for verification

## Why It Matters

- **Audibility**: Every decision can be replayed and verified
- **Compliance**: Meet regulatory requirements for AI systems
- **Trust**: Cryptographic proof that outputs haven't been tampered with
- **Debugging**: Reproduce bugs deterministically for analysis

## Next Steps

### Start the Control Plane

```bash
./target/release/aos-cp --config configs/cp.toml
```

### Run Your First Inference

```bash
cargo run --bin aosctl serve --tenant default --plan qwen7b --socket /var/run/aos/default/aos.sock
```

### Learn More

- `docs/architecture.md` - System architecture overview
- `docs/control-plane.md` - Control plane operations
- `README.md` - Getting started guide

## Learn More

Visit the documentation at `docs/architecture.md` for detailed technical information.
"""
    
    static func saveToFile() -> Bool {
        let targetPath = "/usr/local/share/adapteros/docs/first_run.md"
        let targetDir = (targetPath as NSString).deletingLastPathComponent
        
        do {
            // Create directory if it doesn't exist
            try FileManager.default.createDirectory(atPath: targetDir, 
                                                   withIntermediateDirectories: true, 
                                                   attributes: nil)
            
            // Write content
            try content.write(toFile: targetPath, atomically: true, encoding: .utf8)
            return true
        } catch {
            print("Failed to save determinism explainer: \(error)")
            
            // Fallback to user's home directory if system directory fails
            let fallbackPath = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(".adapteros/first_run.md")
                .path
            
            do {
                let fallbackDir = (fallbackPath as NSString).deletingLastPathComponent
                try FileManager.default.createDirectory(atPath: fallbackDir,
                                                       withIntermediateDirectories: true,
                                                       attributes: nil)
                try content.write(toFile: fallbackPath, atomically: true, encoding: .utf8)
                return true
            } catch {
                print("Failed to save to fallback location: \(error)")
                return false
            }
        }
    }
}

