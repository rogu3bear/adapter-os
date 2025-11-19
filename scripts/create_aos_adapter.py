#!/usr/bin/env python3
"""
Create .aos adapter archives from directory structure

This script packages adapter directories containing manifest.json and weights.safetensors
into the AOS 2.0 binary format for efficient loading by AdapterOS.
"""

import json
import struct
import argparse
from pathlib import Path
import sys

try:
    import blake3
    HAS_BLAKE3 = True
except ImportError:
    import hashlib
    HAS_BLAKE3 = False
    print("Warning: blake3 not installed, using SHA256 instead. Install with: pip install blake3")

def compute_hash(data: bytes) -> str:
    """Compute BLAKE3 hash of data, fallback to SHA256 if not available"""
    if HAS_BLAKE3:
        return blake3.blake3(data).hexdigest()
    else:
        return hashlib.sha256(data).hexdigest()

def load_safetensors(file_path: Path) -> bytes:
    """Load safetensors file as raw bytes"""
    with open(file_path, 'rb') as f:
        return f.read()

def update_manifest_for_aos(manifest: dict, weights_hash: str) -> dict:
    """Update manifest for AOS format version 2"""
    # Ensure format version is 2
    manifest['format_version'] = 2

    # Add weights hash
    manifest['weights_hash'] = weights_hash

    # Generate adapter_id if not present (semantic naming)
    if 'adapter_id' not in manifest:
        # Try to generate from name or use a default
        name = manifest.get('name', 'adapter')
        tenant = 'default'
        domain = 'general'
        purpose = name.lower().replace(' ', '-').replace('_', '-')
        revision = 'r001'
        manifest['adapter_id'] = f"{tenant}/{domain}/{purpose}/{revision}"

    # Add name if not present
    if 'name' not in manifest:
        manifest['name'] = manifest.get('adapter_id', 'Unnamed Adapter').split('/')[-1]

    # Ensure required fields
    required_fields = {
        'version': '1.0.0',
        'rank': 16,
        'alpha': 32.0,
        'base_model': 'qwen2.5-7b',
        'target_modules': ['q_proj', 'k_proj', 'v_proj', 'o_proj'],
        'created_at': manifest.get('created_at', '2025-01-18T12:00:00Z')
    }

    for field, default in required_fields.items():
        if field not in manifest:
            manifest[field] = default

    # Handle training_config if present
    if 'training_config' in manifest:
        config = manifest['training_config']
        # Ensure rank and alpha are consistent
        if 'rank' in config:
            manifest['rank'] = config['rank']
        if 'alpha' in config:
            manifest['alpha'] = config['alpha']

    return manifest

def create_aos_file(adapter_dir: Path, output_path: Path, verbose: bool = False):
    """Package adapter directory into .aos archive"""

    if verbose:
        print(f"📦 Packaging {adapter_dir.name}...")

    # Validate inputs
    manifest_path = adapter_dir / "manifest.json"
    weights_path = adapter_dir / "weights.safetensors"

    if not manifest_path.exists():
        raise FileNotFoundError(f"Missing manifest.json in {adapter_dir}")
    if not weights_path.exists():
        raise FileNotFoundError(f"Missing weights.safetensors in {adapter_dir}")

    # Read manifest
    with open(manifest_path, 'r') as f:
        manifest = json.load(f)

    # Load weights
    weights_data = load_safetensors(weights_path)

    # Compute BLAKE3 hash of weights
    weights_hash = compute_hash(weights_data)

    # Update manifest for AOS format
    manifest = update_manifest_for_aos(manifest, weights_hash)

    # Serialize manifest to JSON
    manifest_json = json.dumps(manifest, indent=2).encode('utf-8')

    # Calculate offsets
    header_size = 8
    weights_offset = header_size
    manifest_offset = weights_offset + len(weights_data)
    manifest_len = len(manifest_json)

    # Validate sizes fit in u32
    if manifest_offset > 0xFFFFFFFF:
        raise ValueError(f"Weights too large: {manifest_offset} bytes exceeds 4GB limit")
    if manifest_len > 0xFFFFFFFF:
        raise ValueError(f"Manifest too large: {manifest_len} bytes exceeds 4GB limit")

    # Create output directory if needed
    output_path.parent.mkdir(parents=True, exist_ok=True)

    # Write .aos file
    with open(output_path, 'wb') as f:
        # Write header (8 bytes)
        f.write(struct.pack('<II', manifest_offset, manifest_len))

        # Write weights
        f.write(weights_data)

        # Write manifest
        f.write(manifest_json)

    # Calculate file size
    file_size = output_path.stat().st_size

    if verbose:
        print(f"✅ Created {output_path.name}")
        print(f"   Size: {file_size / 1024 / 1024:.2f} MB")
        print(f"   Hash: {weights_hash[:16]}...")
        print(f"   ID: {manifest['adapter_id']}")
        print(f"   Rank: {manifest['rank']}")

    return weights_hash, manifest

def verify_aos_file(aos_path: Path, verbose: bool = False):
    """Quick verification of created .aos file"""
    if verbose:
        print(f"🔍 Verifying {aos_path.name}...")

    with open(aos_path, 'rb') as f:
        # Read header
        header = f.read(8)
        if len(header) < 8:
            raise ValueError("Invalid .aos file: header too short")

        manifest_offset, manifest_len = struct.unpack('<II', header)

        # Read weights
        weights_size = manifest_offset - 8
        weights_data = f.read(weights_size)

        # Read manifest
        manifest_json = f.read(manifest_len)
        manifest = json.loads(manifest_json)

        # Verify hash
        computed_hash = compute_hash(weights_data)
        stored_hash = manifest.get('weights_hash')

        if computed_hash != stored_hash:
            raise ValueError(f"Hash mismatch: computed {computed_hash[:16]}... != stored {stored_hash[:16]}...")

        if verbose:
            print(f"✅ Valid .aos file")
            print(f"   Format version: {manifest.get('format_version', 'unknown')}")
            print(f"   Adapter ID: {manifest.get('adapter_id', 'unknown')}")
            print(f"   Weights size: {weights_size / 1024 / 1024:.2f} MB")
            print(f"   Hash verified: {computed_hash[:16]}...")

    return True

def main():
    parser = argparse.ArgumentParser(
        description='Create .aos adapter archives from directory structure',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Package single adapter
  %(prog)s adapters/code_lang_v1/ -o adapters/code-assistant.aos

  # Package with verbose output
  %(prog)s adapters/README_adapter/ -o adapters/readme-writer.aos -v

  # Package and verify
  %(prog)s adapters/my_adapter/ -o my_adapter.aos --verify
        """
    )

    parser.add_argument('adapter_dir', type=Path,
                        help='Directory containing adapter files (manifest.json, weights.safetensors)')
    parser.add_argument('--output', '-o', type=Path,
                        help='Output .aos file path (default: adapters/<dirname>.aos)')
    parser.add_argument('--verbose', '-v', action='store_true',
                        help='Show detailed output')
    parser.add_argument('--verify', action='store_true',
                        help='Verify the created .aos file')

    args = parser.parse_args()

    # Validate input directory
    if not args.adapter_dir.exists():
        print(f"Error: Directory not found: {args.adapter_dir}", file=sys.stderr)
        sys.exit(1)

    if not args.adapter_dir.is_dir():
        print(f"Error: Not a directory: {args.adapter_dir}", file=sys.stderr)
        sys.exit(1)

    # Default output path
    if args.output is None:
        adapter_name = args.adapter_dir.name
        args.output = Path('adapters') / f"{adapter_name}.aos"

    try:
        # Create .aos archive
        weights_hash, manifest = create_aos_file(
            args.adapter_dir,
            args.output,
            verbose=args.verbose
        )

        # Verify if requested
        if args.verify:
            verify_aos_file(args.output, verbose=args.verbose)

        # Success message
        if not args.verbose:
            print(f"Created: {args.output}")

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == '__main__':
    main()