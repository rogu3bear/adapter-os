#!/usr/bin/env python3
import json
import sys
import os

def main():
    if len(sys.argv) != 4:
        print("Usage: update_registry.py <hash> <sdk_version> <compiler_version>")
        sys.exit(1)
    
    hash_value = sys.argv[1]
    sdk_version = sys.argv[2]
    compiler_version = sys.argv[3]
    
    # Read the current registry
    with open('kernels.json', 'r') as f:
        registry = json.load(f)
    
    # Update the hash for the unified kernel
    registry['kernels'][0]['blake3_hash'] = hash_value
    registry['kernels'][1]['blake3_hash'] = hash_value
    registry['kernels'][2]['blake3_hash'] = hash_value
    
    # Update build metadata
    registry['metal_sdk_version'] = sdk_version
    registry['compiler_version'] = compiler_version
    
    # Write back the updated registry
    with open('kernels.json', 'w') as f:
        json.dump(registry, f, indent=2)
    
    print(f'   Updated kernels.json with hash: {hash_value}')
    print(f'   Metal SDK: {sdk_version}')
    print(f'   Compiler: {compiler_version}')

if __name__ == '__main__':
    main()
