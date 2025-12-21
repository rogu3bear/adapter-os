#!/usr/bin/env python3
"""
Manifest signing script for AdapterOS CI/CD
Signs kernel manifests with Ed25519 for deterministic verification
"""

import argparse
import base64
import json
import sys
from pathlib import Path
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ed25519
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.backends import default_backend


def load_private_key(key_data: str) -> ed25519.Ed25519PrivateKey:
    """Load Ed25519 private key from base64-encoded PEM"""
    try:
        # Decode base64 if needed
        if not key_data.startswith('-----BEGIN'):
            key_data = base64.b64decode(key_data).decode('utf-8')
        
        private_key = serialization.load_pem_private_key(
            key_data.encode('utf-8'),
            password=None,
            backend=default_backend()
        )
        
        if not isinstance(private_key, ed25519.Ed25519PrivateKey):
            raise ValueError("Key is not Ed25519")
            
        return private_key
    except Exception as e:
        print(f"❌ Failed to load private key: {e}", file=sys.stderr)
        sys.exit(1)


def sign_manifest(manifest_path: Path, output_path: Path, private_key: ed25519.Ed25519PrivateKey) -> None:
    """Sign manifest and write .sig file"""
    try:
        # Load manifest
        with open(manifest_path, 'r') as f:
            manifest_data = f.read()
        
        # Parse JSON to ensure it's valid
        manifest_json = json.loads(manifest_data)
        
        # Sign the canonical JSON (sorted keys for determinism)
        canonical_json = json.dumps(manifest_json, sort_keys=True, separators=(',', ':'))
        signature = private_key.sign(canonical_json.encode('utf-8'))
        
        # Get public key for verification
        public_key = private_key.public_key()
        public_bytes = public_key.public_bytes(
            encoding=serialization.Encoding.Raw,
            format=serialization.PublicFormat.Raw
        )
        
        # Create signature metadata
        sig_metadata = {
            "signature": base64.b64encode(signature).decode('ascii'),
            "public_key": base64.b64encode(public_bytes).decode('ascii'),
            "algorithm": "Ed25519",
            "canonical_json": canonical_json
        }
        
        # Write signature file
        with open(output_path, 'w') as f:
            json.dump(sig_metadata, f, indent=2)
        
        print(f"✅ Manifest signed: {output_path}")
        print(f"   Signature: {base64.b64encode(signature).decode('ascii')[:16]}...")
        
    except Exception as e:
        print(f"❌ Failed to sign manifest: {e}", file=sys.stderr)
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(description='Sign AdapterOS kernel manifest')
    parser.add_argument('--manifest', required=True, help='Path to manifest JSON')
    parser.add_argument('--out', required=True, help='Output path for signed manifest')
    parser.add_argument('--key', required=True, help='Ed25519 private key (base64 or PEM)')
    
    args = parser.parse_args()
    
    manifest_path = Path(args.manifest)
    output_path = Path(args.out)
    
    if not manifest_path.exists():
        print(f"❌ Manifest not found: {manifest_path}", file=sys.stderr)
        sys.exit(1)
    
    # Load private key
    private_key = load_private_key(args.key)
    
    # Sign manifest
    sign_manifest(manifest_path, output_path, private_key)


if __name__ == '__main__':
    main()
