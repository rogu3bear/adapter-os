#!/usr/bin/env python3
"""
AdapterOS Upload Examples

Complete working examples for uploading .aos adapters to AdapterOS.
Run with: python upload_examples.py
"""

import requests
import json
import time
import os
from pathlib import Path
from dataclasses import dataclass
from typing import Optional, Callable
import struct


@dataclass
class UploadResponse:
    """Parsed upload response"""
    adapter_id: str
    tenant_id: str
    hash_b3: str
    file_path: str
    file_size: int
    lifecycle_state: str
    created_at: str


class SimpleUploader:
    """Minimal upload example"""

    def __init__(self, base_url: str, token: str):
        self.base_url = base_url
        self.token = token

    def upload(self, file_path: str, name: str) -> UploadResponse:
        """Upload .aos file"""
        with open(file_path, 'rb') as f:
            response = requests.post(
                f'{self.base_url}/v1/adapters/upload-aos',
                headers={'Authorization': f'Bearer {self.token}'},
                files={
                    'file': f,
                    'name': (None, name),
                },
            )

        response.raise_for_status()
        data = response.json()

        return UploadResponse(
            adapter_id=data['adapter_id'],
            tenant_id=data['tenant_id'],
            hash_b3=data['hash_b3'],
            file_path=data['file_path'],
            file_size=data['file_size'],
            lifecycle_state=data['lifecycle_state'],
            created_at=data['created_at'],
        )


class ProductionUploader:
    """Production-grade uploader with retry, validation, and error handling"""

    def __init__(
        self,
        base_url: str,
        token: str,
        max_retries: int = 3,
        timeout: float = 60.0,
    ):
        self.base_url = base_url
        self.token = token
        self.max_retries = max_retries
        self.timeout = timeout
        self.session = self._create_session()

    def _create_session(self) -> requests.Session:
        """Create optimized session with connection pooling and retry logic"""
        session = requests.Session()

        # Configure connection pooling
        from requests.adapters import HTTPAdapter
        from urllib3.util.retry import Retry

        retry_strategy = Retry(
            total=self.max_retries,
            backoff_factor=1.0,
            status_forcelist=[500, 502, 503, 504],
            allowed_methods=["HEAD", "GET", "OPTIONS", "POST"]
        )
        adapter = HTTPAdapter(max_retries=retry_strategy)
        session.mount("http://", adapter)
        session.mount("https://", adapter)

        return session

    def validate_file(self, file_path: str) -> None:
        """Validate file before upload"""
        path = Path(file_path)

        # Check existence
        if not path.exists():
            raise FileNotFoundError(f"File not found: {file_path}")

        # Check extension
        if path.suffix != '.aos':
            raise ValueError(f"File must have .aos extension, got {path.suffix}")

        # Check size
        file_size = path.stat().st_size
        max_size = 1024 * 1024 * 1024  # 1GB
        if file_size > max_size:
            raise ValueError(
                f"File too large: {file_size / (1024*1024):.1f}MB "
                f"(max: {max_size / (1024*1024):.0f}MB)"
            )

        # Validate .aos structure
        try:
            self._validate_aos_structure(file_path)
        except Exception as e:
            raise ValueError(f"Invalid .aos file: {e}")

    def _validate_aos_structure(self, file_path: str) -> None:
        """Validate .aos file format"""
        with open(file_path, 'rb') as f:
            # Read header
            header = f.read(8)
            if len(header) < 8:
                raise ValueError("File too small for .aos header")

            offset, length = struct.unpack('<II', header)

            # Validate header values
            if offset < 8 or length == 0:
                raise ValueError(
                    f"Invalid .aos header: offset={offset}, length={length}"
                )

            # Check manifest fits within file
            f.seek(0, 2)  # Seek to end
            file_size = f.tell()

            if offset + length > file_size:
                raise ValueError(
                    f"Manifest extends beyond file: "
                    f"offset={offset}, length={length}, file_size={file_size}"
                )

            # Validate manifest is JSON
            f.seek(offset)
            manifest_bytes = f.read(length)

            try:
                manifest = json.loads(manifest_bytes)
                if not isinstance(manifest, dict):
                    raise ValueError("Manifest must be JSON object")
            except json.JSONDecodeError as e:
                raise ValueError(f"Manifest is not valid JSON: {e}")

    def upload(
        self,
        file_path: str,
        name: str,
        description: Optional[str] = None,
        tier: str = "ephemeral",
        category: str = "general",
        scope: str = "general",
        rank: int = 1,
        alpha: float = 1.0,
        on_progress: Optional[Callable[[int, int], None]] = None,
    ) -> UploadResponse:
        """
        Upload adapter with validation and retry logic

        Args:
            file_path: Path to .aos file
            name: Display name
            description: Optional description
            tier: ephemeral, warm, or persistent
            category: general, code, text, vision, or audio
            scope: general, public, private, or tenant
            rank: LoRA rank (1-512)
            alpha: LoRA scaling (0.0-100.0)
            on_progress: Optional progress callback (bytes_done, total)

        Returns:
            UploadResponse with adapter metadata

        Raises:
            FileNotFoundError: File doesn't exist
            ValueError: Validation failed
            requests.HTTPError: Upload failed
        """
        # Validate input
        self.validate_file(file_path)

        if not 1 <= rank <= 512:
            raise ValueError(f"Rank must be 1-512, got {rank}")

        if not 0.0 <= alpha <= 100.0:
            raise ValueError(f"Alpha must be 0.0-100.0, got {alpha}")

        # Retry loop
        last_error = None
        for attempt in range(1, self.max_retries + 1):
            try:
                return self._do_upload(
                    file_path, name, description, tier, category, scope,
                    rank, alpha, on_progress
                )
            except requests.exceptions.Timeout:
                last_error = f"Timeout on attempt {attempt}"
                if attempt < self.max_retries:
                    delay = 2 ** attempt
                    print(f"Timeout, retrying in {delay}s...")
                    time.sleep(delay)
            except requests.exceptions.ConnectionError:
                last_error = f"Connection error on attempt {attempt}"
                if attempt < self.max_retries:
                    delay = 2 ** attempt
                    print(f"Connection error, retrying in {delay}s...")
                    time.sleep(delay)

        raise RuntimeError(f"Upload failed: {last_error}")

    def _do_upload(
        self,
        file_path: str,
        name: str,
        description: Optional[str],
        tier: str,
        category: str,
        scope: str,
        rank: int,
        alpha: float,
        on_progress: Optional[Callable[[int, int], None]],
    ) -> UploadResponse:
        """Perform actual upload"""
        file_size = Path(file_path).stat().st_size
        self.timeout = max(60, file_size / (1024 * 1024))  # 1s per MB minimum

        with open(file_path, 'rb') as f:
            files = {
                'file': f,
                'name': (None, name),
                'tier': (None, tier),
                'category': (None, category),
                'scope': (None, scope),
                'rank': (None, str(rank)),
                'alpha': (None, str(alpha)),
            }

            if description:
                files['description'] = (None, description)

            # Track progress
            if on_progress:
                bytes_read = 0

                def wrapped_read(size=-1):
                    nonlocal bytes_read
                    chunk = f.read(size)
                    bytes_read += len(chunk)
                    on_progress(bytes_read, file_size)
                    return chunk

                f.read = wrapped_read

            response = self.session.post(
                f'{self.base_url}/v1/adapters/upload-aos',
                headers={'Authorization': f'Bearer {self.token}'},
                files=files,
                timeout=self.timeout,
            )

        # Handle errors
        if response.status_code == 400:
            error = response.json()
            raise ValueError(
                f"Validation error ({error.get('error_code')}): "
                f"{error.get('message')}"
            )
        elif response.status_code == 403:
            raise PermissionError(
                "Insufficient permissions (need Admin or Operator role)"
            )
        elif response.status_code == 409:
            raise ValueError("Adapter ID conflict (UUID collision, retry)")
        elif response.status_code == 413:
            raise ValueError("File too large for endpoint")
        elif response.status_code == 507:
            raise RuntimeError("Server disk space exhausted")

        response.raise_for_status()

        # Parse response
        data = response.json()
        return UploadResponse(
            adapter_id=data['adapter_id'],
            tenant_id=data['tenant_id'],
            hash_b3=data['hash_b3'],
            file_path=data['file_path'],
            file_size=data['file_size'],
            lifecycle_state=data['lifecycle_state'],
            created_at=data['created_at'],
        )


def create_test_aos_file(path: str) -> None:
    """Create minimal valid .aos file for testing"""
    manifest = json.dumps({
        "version": "1.0.0",
        "model_type": "lora",
        "base_model": "llama",
        "rank": 16,
        "alpha": 8.0,
    }).encode('utf-8')

    with open(path, 'wb') as f:
        # Write header
        f.write(struct.pack('<I', 8))  # Manifest offset
        f.write(struct.pack('<I', len(manifest)))  # Manifest length
        # Write manifest
        f.write(manifest)


def example_simple_upload():
    """Example 1: Minimal upload"""
    print("=" * 60)
    print("Example 1: Simple Upload")
    print("=" * 60)

    token = os.getenv('JWT_TOKEN', 'test-token')
    api_url = os.getenv('API_URL', 'http://localhost:8080')

    uploader = SimpleUploader(api_url, token)

    # Create test file
    test_file = '/tmp/test_adapter.aos'
    create_test_aos_file(test_file)

    try:
        result = uploader.upload(test_file, 'Simple Test Adapter')
        print(f"✓ Upload successful!")
        print(f"  Adapter ID: {result.adapter_id}")
        print(f"  Hash: {result.hash_b3}")
        print(f"  State: {result.lifecycle_state}")
    except Exception as e:
        print(f"✗ Upload failed: {e}")
    finally:
        os.remove(test_file)


def example_production_upload():
    """Example 2: Production upload with validation"""
    print("\n" + "=" * 60)
    print("Example 2: Production Upload")
    print("=" * 60)

    token = os.getenv('JWT_TOKEN', 'test-token')
    api_url = os.getenv('API_URL', 'http://localhost:8080')

    uploader = ProductionUploader(api_url, token, max_retries=3)

    # Create test file
    test_file = '/tmp/production_adapter.aos'
    create_test_aos_file(test_file)

    try:
        # Upload with progress tracking
        def show_progress(done, total):
            pct = (done / total) * 100
            print(f"  Progress: {pct:.1f}%", end='\r')

        result = uploader.upload(
            test_file,
            name='Production Test Adapter',
            description='Tested adapter for production',
            tier='persistent',
            category='code',
            rank=16,
            alpha=8.0,
            on_progress=show_progress,
        )

        print(f"\n✓ Upload successful!")
        print(f"  Adapter ID: {result.adapter_id}")
        print(f"  File size: {result.file_size} bytes")
        print(f"  Hash: {result.hash_b3}")
        print(f"  State: {result.lifecycle_state}")

    except ValueError as e:
        print(f"✗ Validation error: {e}")
    except PermissionError as e:
        print(f"✗ Permission error: {e}")
    except Exception as e:
        print(f"✗ Upload failed: {e}")
    finally:
        os.remove(test_file)


def example_batch_upload():
    """Example 3: Batch upload with rate limiting"""
    print("\n" + "=" * 60)
    print("Example 3: Batch Upload")
    print("=" * 60)

    token = os.getenv('JWT_TOKEN', 'test-token')
    api_url = os.getenv('API_URL', 'http://localhost:8080')

    uploader = ProductionUploader(api_url, token)

    # Create multiple test files
    test_files = []
    for i in range(3):
        test_file = f'/tmp/batch_adapter_{i}.aos'
        create_test_aos_file(test_file)
        test_files.append((test_file, f'Batch Adapter {i}'))

    try:
        results = []
        for file_path, name in test_files:
            print(f"Uploading {name}...")
            result = uploader.upload(file_path, name, tier='ephemeral')
            results.append(result)
            print(f"  ✓ {result.adapter_id}")
            time.sleep(0.5)  # Rate limit: ~2 uploads/sec

        print(f"\n✓ Batch upload complete ({len(results)} adapters)")
        for result in results:
            print(f"  - {result.adapter_id}: {result.lifecycle_state}")

    except Exception as e:
        print(f"✗ Batch upload failed: {e}")
    finally:
        for file_path, _ in test_files:
            if os.path.exists(file_path):
                os.remove(file_path)


def example_error_handling():
    """Example 4: Error handling patterns"""
    print("\n" + "=" * 60)
    print("Example 4: Error Handling")
    print("=" * 60)

    token = os.getenv('JWT_TOKEN', 'test-token')
    api_url = os.getenv('API_URL', 'http://localhost:8080')

    uploader = ProductionUploader(api_url, token)

    # Example 4a: File validation errors
    print("\n4a. Testing validation errors:")
    try:
        uploader.upload('/nonexistent/file.aos', 'Test')
    except FileNotFoundError as e:
        print(f"  ✓ Caught: {e}")

    try:
        test_file = '/tmp/wrong_ext.txt'
        open(test_file, 'w').write('test')
        uploader.upload(test_file, 'Test')
    except ValueError as e:
        print(f"  ✓ Caught: {e}")
    finally:
        os.remove(test_file)

    try:
        uploader.upload('valid.aos', 'Test', rank=1000)  # Out of bounds
    except ValueError as e:
        print(f"  ✓ Caught: {e}")

    # Example 4b: Network errors (simulated)
    print("\n4b. Network retry strategy:")
    print("  Timeout → Wait 2s → Retry")
    print("  Connection error → Wait 4s → Retry")
    print("  After 3 attempts → Fail with descriptive error")


if __name__ == '__main__':
    print("\nAdapterOS Upload Examples")
    print("Set JWT_TOKEN and API_URL environment variables\n")

    example_simple_upload()
    example_production_upload()
    example_batch_upload()
    example_error_handling()

    print("\n" + "=" * 60)
    print("Examples complete!")
    print("=" * 60)
