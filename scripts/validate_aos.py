#!/usr/bin/env python3
"""
Validate .aos adapter archives

This script thoroughly validates the structure and integrity of .aos files,
checking format compliance, hash verification, and manifest requirements.
"""

import json
import struct
import sys
from pathlib import Path
import argparse

try:
    import blake3
    HAS_BLAKE3 = True
except ImportError:
    import hashlib
    HAS_BLAKE3 = False

def compute_hash(data: bytes) -> str:
    """Compute hash of data (BLAKE3 if available, else SHA256)"""
    if HAS_BLAKE3:
        return blake3.blake3(data).hexdigest()
    else:
        return hashlib.sha256(data).hexdigest()

def validate_manifest_fields(manifest: dict) -> list:
    """Validate required manifest fields and structure"""
    errors = []

    # Check format version
    if 'format_version' not in manifest:
        errors.append("Missing required field: format_version")
    elif manifest['format_version'] != 2:
        errors.append(f"Unsupported format version: {manifest['format_version']} (expected 2)")

    # Required fields for AOS 2.0
    required_fields = [
        'adapter_id',
        'name',
        'version',
        'rank',
        'alpha',
        'base_model',
        'target_modules',
        'created_at',
        'weights_hash'
    ]

    for field in required_fields:
        if field not in manifest:
            errors.append(f"Missing required field: {field}")

    # Validate adapter_id format (semantic naming)
    if 'adapter_id' in manifest:
        adapter_id = manifest['adapter_id']
        parts = adapter_id.split('/')
        if len(parts) != 4:
            errors.append(f"Invalid adapter_id format: {adapter_id} (expected tenant/domain/purpose/revision)")
        else:
            tenant, domain, purpose, revision = parts
            # Check for reserved names
            reserved_tenants = ['system', 'admin', 'root', 'test']
            reserved_domains = ['core', 'internal', 'deprecated']

            if tenant in reserved_tenants and tenant != 'default':
                errors.append(f"Reserved tenant name: {tenant}")
            if domain in reserved_domains:
                errors.append(f"Reserved domain name: {domain}")
            if not revision.startswith('r') and not revision.startswith('v'):
                errors.append(f"Invalid revision format: {revision} (should start with 'r' or 'v')")

    # Validate rank and alpha
    if 'rank' in manifest:
        rank = manifest['rank']
        if not isinstance(rank, int) or rank < 1 or rank > 256:
            errors.append(f"Invalid rank: {rank} (must be integer 1-256)")

    if 'alpha' in manifest:
        alpha = manifest['alpha']
        if not isinstance(alpha, (int, float)) or alpha <= 0:
            errors.append(f"Invalid alpha: {alpha} (must be positive number)")

    # Validate target_modules
    if 'target_modules' in manifest:
        modules = manifest['target_modules']
        if not isinstance(modules, list) or len(modules) == 0:
            errors.append("target_modules must be a non-empty list")
        else:
            valid_modules = ['q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj']
            for module in modules:
                if module not in valid_modules:
                    errors.append(f"Unknown target module: {module}")

    return errors

def validate_aos(aos_path: Path, verbose: bool = False) -> tuple:
    """
    Validate an .aos archive

    Returns: (is_valid: bool, errors: list[str])
    """
    if verbose:
        print(f"🔍 Validating {aos_path.name}...")

    errors = []
    file_size = aos_path.stat().st_size

    # Check file size limits
    max_size = 500 * 1024 * 1024  # 500 MB
    if file_size > max_size:
        errors.append(f"File too large: {file_size / 1024 / 1024:.2f} MB (max {max_size / 1024 / 1024} MB)")

    if file_size < 8:
        errors.append(f"File too small: {file_size} bytes (minimum 8 bytes for header)")
        return False, errors

    try:
        with open(aos_path, 'rb') as f:
            # Read and validate header
            header = f.read(8)
            if len(header) < 8:
                errors.append("Invalid header: too short")
                return False, errors

            manifest_offset, manifest_len = struct.unpack('<II', header)

            # Validate offsets
            if manifest_offset < 8:
                errors.append(f"Invalid manifest offset: {manifest_offset} (must be >= 8)")

            if manifest_offset + manifest_len > file_size:
                errors.append(f"Manifest extends beyond file: offset {manifest_offset} + len {manifest_len} > size {file_size}")
                return False, errors

            # Read weights
            weights_size = manifest_offset - 8
            if weights_size <= 0:
                errors.append(f"Invalid weights size: {weights_size}")
                return False, errors

            weights_data = f.read(weights_size)
            if len(weights_data) != weights_size:
                errors.append(f"Could not read all weights data: expected {weights_size}, got {len(weights_data)}")

            # Read and parse manifest
            f.seek(manifest_offset)
            manifest_json = f.read(manifest_len)
            if len(manifest_json) != manifest_len:
                errors.append(f"Could not read all manifest data: expected {manifest_len}, got {len(manifest_json)}")

            try:
                manifest = json.loads(manifest_json)
            except json.JSONDecodeError as e:
                errors.append(f"Invalid manifest JSON: {e}")
                return False, errors

            # Validate manifest structure
            manifest_errors = validate_manifest_fields(manifest)
            errors.extend(manifest_errors)

            # Verify weights hash
            if 'weights_hash' in manifest:
                computed_hash = compute_hash(weights_data)
                stored_hash = manifest['weights_hash']

                if computed_hash != stored_hash:
                    errors.append(f"Hash mismatch: computed {computed_hash[:16]}... != stored {stored_hash[:16]}...")
                elif verbose:
                    print(f"   ✓ Hash verified: {computed_hash[:16]}...")

            # Additional SafeTensors validation for weights
            if weights_size > 8:
                # Try to parse as SafeTensors header
                try:
                    st_header_size = struct.unpack('<Q', weights_data[:8])[0]
                    if st_header_size < weights_size - 8:
                        st_header_json = weights_data[8:8+st_header_size].decode('utf-8')
                        st_header = json.loads(st_header_json)
                        if verbose:
                            tensor_count = len([k for k in st_header.keys() if k != '__metadata__'])
                            print(f"   ✓ SafeTensors format: {tensor_count} tensors")
                except Exception as e:
                    # Not SafeTensors format, might be Q15 or other format
                    if verbose:
                        print(f"   ℹ️  Weights format: Custom/Q15 (not SafeTensors)")

            # Report results
            if verbose:
                print(f"\n📊 Archive Details:")
                print(f"   Format version: {manifest.get('format_version', 'unknown')}")
                print(f"   Adapter ID: {manifest.get('adapter_id', 'unknown')}")
                print(f"   Name: {manifest.get('name', 'unknown')}")
                print(f"   Rank: {manifest.get('rank', 'unknown')}")
                print(f"   Alpha: {manifest.get('alpha', 'unknown')}")
                print(f"   Base model: {manifest.get('base_model', 'unknown')}")
                print(f"   File size: {file_size / 1024 / 1024:.2f} MB")
                print(f"   Weights size: {weights_size / 1024 / 1024:.2f} MB")
                print(f"   Manifest size: {manifest_len / 1024:.2f} KB")

                if 'training_config' in manifest:
                    config = manifest['training_config']
                    print(f"\n📚 Training Config:")
                    print(f"   Learning rate: {config.get('learning_rate', 'unknown')}")
                    print(f"   Batch size: {config.get('batch_size', 'unknown')}")
                    print(f"   Epochs: {config.get('epochs', 'unknown')}")

    except Exception as e:
        errors.append(f"Unexpected error: {e}")

    # Final validation result
    is_valid = len(errors) == 0

    if verbose:
        if is_valid:
            print(f"\n✅ Valid .aos archive")
        else:
            print(f"\n❌ Validation failed with {len(errors)} error(s):")
            for error in errors:
                print(f"   - {error}")

    return is_valid, errors

def main():
    parser = argparse.ArgumentParser(
        description='Validate .aos adapter archives',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Validate single file
  %(prog)s adapters/code-assistant.aos

  # Validate with verbose output
  %(prog)s adapters/code-assistant.aos -v

  # Validate multiple files
  %(prog)s adapters/*.aos

  # Quiet mode (exit code only)
  %(prog)s adapters/code-assistant.aos -q
        """
    )

    parser.add_argument('aos_files', nargs='+', type=Path,
                        help='One or more .aos files to validate')
    parser.add_argument('--verbose', '-v', action='store_true',
                        help='Show detailed validation output')
    parser.add_argument('--quiet', '-q', action='store_true',
                        help='Quiet mode - only show errors')

    args = parser.parse_args()

    total_files = 0
    valid_files = 0
    all_errors = {}

    for aos_file in args.aos_files:
        if not aos_file.exists():
            if not args.quiet:
                print(f"Error: File not found: {aos_file}", file=sys.stderr)
            continue

        if not aos_file.is_file():
            if not args.quiet:
                print(f"Error: Not a file: {aos_file}", file=sys.stderr)
            continue

        total_files += 1
        is_valid, errors = validate_aos(aos_file, verbose=args.verbose and not args.quiet)

        if is_valid:
            valid_files += 1
            if not args.quiet and not args.verbose:
                print(f"✅ {aos_file.name}: Valid")
        else:
            all_errors[str(aos_file)] = errors
            if not args.quiet and not args.verbose:
                print(f"❌ {aos_file.name}: {len(errors)} error(s)")

    # Summary
    if total_files > 1 and not args.quiet:
        print(f"\n📊 Summary: {valid_files}/{total_files} files valid")

    # Show all errors if not in verbose mode (verbose shows them inline)
    if all_errors and not args.verbose and not args.quiet:
        print("\n❌ Validation Errors:")
        for file_path, errors in all_errors.items():
            print(f"\n  {Path(file_path).name}:")
            for error in errors[:5]:  # Show first 5 errors
                print(f"    - {error}")
            if len(errors) > 5:
                print(f"    ... and {len(errors) - 5} more")

    # Exit with error if any files invalid
    sys.exit(0 if valid_files == total_files else 1)

if __name__ == '__main__':
    main()