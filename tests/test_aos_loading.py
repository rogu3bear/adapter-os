#!/usr/bin/env python3
"""
Test loading of .aos adapter files

This test script verifies that all created .aos files can be loaded
and have the expected properties.
"""

import struct
import json
from pathlib import Path
import sys

def load_aos_manifest(aos_path: Path) -> dict:
    """Load and return the manifest from an .aos file"""
    with open(aos_path, 'rb') as f:
        # Read header
        manifest_offset, manifest_len = struct.unpack('<II', f.read(8))

        # Seek to manifest
        f.seek(manifest_offset)

        # Read and parse manifest
        manifest_json = f.read(manifest_len)
        return json.loads(manifest_json)

def test_adapter_loading():
    """Test loading all three adapter files"""
    print("🧪 Testing .aos adapter loading...")

    adapters = [
        {
            'file': 'adapters/code-assistant.aos',
            'expected_rank': 16,
            'expected_name_contains': 'adapter',
            'min_size_mb': 1.0
        },
        {
            'file': 'adapters/readme-writer.aos',
            'expected_rank': 8,
            'expected_name_contains': 'adapter',
            'min_size_mb': 0.1
        },
        {
            'file': 'adapters/creative-writer.aos',
            'expected_rank': 12,
            'expected_name_contains': 'adapter',
            'min_size_mb': 1.0
        }
    ]

    all_passed = True
    results = []

    for adapter_info in adapters:
        aos_path = Path(adapter_info['file'])
        test_name = aos_path.name

        try:
            # Check file exists
            if not aos_path.exists():
                results.append((test_name, False, "File not found"))
                all_passed = False
                continue

            # Check file size
            size_mb = aos_path.stat().st_size / 1024 / 1024
            if size_mb < adapter_info['min_size_mb']:
                results.append((test_name, False, f"File too small: {size_mb:.2f} MB"))
                all_passed = False
                continue

            # Load manifest
            manifest = load_aos_manifest(aos_path)

            # Verify format version
            if manifest.get('format_version') != 2:
                results.append((test_name, False, f"Wrong format version: {manifest.get('format_version')}"))
                all_passed = False
                continue

            # Verify rank
            if manifest.get('rank') != adapter_info['expected_rank']:
                results.append((test_name, False, f"Wrong rank: {manifest.get('rank')} (expected {adapter_info['expected_rank']}"))
                all_passed = False
                continue

            # Verify name/id contains expected string
            adapter_id = manifest.get('adapter_id', '')
            name = manifest.get('name', '')
            if adapter_info['expected_name_contains'] not in adapter_id.lower() and \
               adapter_info['expected_name_contains'] not in name.lower():
                results.append((test_name, False, f"Name/ID doesn't contain '{adapter_info['expected_name_contains']}'"))
                all_passed = False
                continue

            # Verify required fields
            required_fields = ['weights_hash', 'base_model', 'target_modules', 'alpha']
            missing_fields = [f for f in required_fields if f not in manifest]
            if missing_fields:
                results.append((test_name, False, f"Missing fields: {', '.join(missing_fields)}"))
                all_passed = False
                continue

            # All checks passed
            results.append((test_name, True, f"Rank={manifest['rank']}, Size={size_mb:.2f}MB"))

        except Exception as e:
            results.append((test_name, False, f"Error: {e}"))
            all_passed = False

    # Print results
    print("\n📊 Test Results:")
    for name, passed, message in results:
        status = "✅" if passed else "❌"
        print(f"  {status} {name}: {message}")

    # Summary
    passed_count = sum(1 for _, passed, _ in results if passed)
    total_count = len(results)
    print(f"\n{'✅' if all_passed else '❌'} Summary: {passed_count}/{total_count} tests passed")

    return all_passed

def test_catalog_integrity():
    """Test that catalog.json correctly references the adapters"""
    print("\n🧪 Testing catalog integrity...")

    catalog_path = Path('adapters/catalog.json')
    if not catalog_path.exists():
        print("❌ catalog.json not found")
        return False

    try:
        with open(catalog_path, 'r') as f:
            catalog = json.load(f)

        errors = []

        # Check adapter count
        if catalog.get('total_adapters') != 3:
            errors.append(f"Wrong adapter count: {catalog.get('total_adapters')} (expected 3)")

        # Check each referenced file exists
        for adapter in catalog.get('adapters', []):
            file_path = Path('adapters') / adapter.get('file', '')
            if not file_path.exists():
                errors.append(f"Referenced file not found: {adapter.get('file')}")

        # Print results
        if errors:
            print("❌ Catalog validation failed:")
            for error in errors:
                print(f"   - {error}")
            return False
        else:
            print("✅ Catalog validation passed")
            print(f"   - {catalog.get('total_adapters')} adapters documented")
            print(f"   - Total size: {catalog.get('total_size_mb')} MB")
            print(f"   - Format: {catalog.get('format')}")
            return True

    except Exception as e:
        print(f"❌ Error reading catalog: {e}")
        return False

def main():
    """Run all tests"""
    print("=" * 50)
    print("AdapterOS .aos File Test Suite")
    print("=" * 50)

    # Run tests
    loading_passed = test_adapter_loading()
    catalog_passed = test_catalog_integrity()

    # Final summary
    print("\n" + "=" * 50)
    if loading_passed and catalog_passed:
        print("✅ ALL TESTS PASSED")
        print("\n📦 Successfully created and validated:")
        print("   - 3 .aos adapter files")
        print("   - Total size: ~3.7 MB")
        print("   - All files loadable and valid")
        print("   - Catalog documentation complete")
        sys.exit(0)
    else:
        print("❌ SOME TESTS FAILED")
        sys.exit(1)

if __name__ == '__main__':
    main()