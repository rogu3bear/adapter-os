#!/usr/bin/env node

/**
 * AdapterOS Upload Examples
 *
 * Complete working examples for uploading .aos adapters to AdapterOS.
 * Run with: node upload_examples.js
 *
 * Requirements:
 *   npm install form-data node-fetch
 */

const fs = require('fs');
const path = require('path');
const FormData = require('form-data');
const fetch = require('node-fetch');

/**
 * Simple uploader - minimal functionality
 */
class SimpleUploader {
    constructor(baseUrl, token) {
        this.baseUrl = baseUrl;
        this.token = token;
    }

    async upload(filePath, name) {
        const form = new FormData();
        form.append('file', fs.createReadStream(filePath), {
            filename: path.basename(filePath),
        });
        form.append('name', name);

        const response = await fetch(
            `${this.baseUrl}/v1/adapters/upload-aos`,
            {
                method: 'POST',
                headers: {
                    'Authorization': `Bearer ${this.token}`,
                    ...form.getHeaders(),
                },
                body: form,
            }
        );

        if (!response.ok) {
            const error = await response.json();
            throw new Error(
                `Upload failed (${response.status}): ` +
                `${error.error_code} - ${error.message}`
            );
        }

        return response.json();
    }
}

/**
 * Production uploader - with retry, validation, and progress
 */
class ProductionUploader {
    constructor(baseUrl, token, options = {}) {
        this.baseUrl = baseUrl;
        this.token = token;
        this.maxRetries = options.maxRetries || 3;
        this.timeout = options.timeout || 60000; // 60 seconds
        this.retryDelay = options.retryDelay || 1000; // 1 second
    }

    /**
     * Validate file before upload
     */
    validateFile(filePath) {
        // Check existence
        if (!fs.existsSync(filePath)) {
            throw new Error(`File not found: ${filePath}`);
        }

        // Check extension
        if (!filePath.endsWith('.aos')) {
            throw new Error(`File must have .aos extension, got: ${filePath}`);
        }

        // Check size
        const stats = fs.statSync(filePath);
        const maxSize = 1024 * 1024 * 1024; // 1GB
        if (stats.size > maxSize) {
            throw new Error(
                `File too large: ${(stats.size / (1024 * 1024)).toFixed(1)}MB ` +
                `(max: ${maxSize / (1024 * 1024)}MB)`
            );
        }

        // Validate .aos structure
        this.validateAosStructure(filePath);
    }

    /**
     * Validate .aos file format
     */
    validateAosStructure(filePath) {
        const buffer = Buffer.alloc(8);
        const fd = fs.openSync(filePath, 'r');

        try {
            // Read header
            fs.readSync(fd, buffer, 0, 8, 0);

            // Parse header (little-endian u32 values)
            const offset = buffer.readUInt32LE(0);
            const length = buffer.readUInt32LE(4);

            // Validate header
            if (offset < 8 || length === 0) {
                throw new Error(
                    `Invalid .aos header: offset=${offset}, length=${length}`
                );
            }

            // Check manifest fits in file
            const stats = fs.fstatSync(fd);
            if (offset + length > stats.size) {
                throw new Error(
                    `Manifest extends beyond file: ` +
                    `offset=${offset}, length=${length}, size=${stats.size}`
                );
            }

            // Validate manifest is JSON
            const manifestBuffer = Buffer.alloc(length);
            fs.readSync(fd, manifestBuffer, 0, length, offset);
            const manifest = JSON.parse(manifestBuffer.toString('utf8'));

            if (typeof manifest !== 'object' || Array.isArray(manifest)) {
                throw new Error('Manifest must be JSON object');
            }
        } finally {
            fs.closeSync(fd);
        }
    }

    /**
     * Upload adapter with validation and retry
     */
    async upload(filePath, name, options = {}) {
        // Validate input
        this.validateFile(filePath);

        const {
            description,
            tier = 'ephemeral',
            category = 'general',
            scope = 'general',
            rank = 1,
            alpha = 1.0,
            onProgress,
        } = options;

        // Validate parameters
        if (rank < 1 || rank > 512) {
            throw new Error(`Rank must be 1-512, got ${rank}`);
        }
        if (alpha < 0.0 || alpha > 100.0) {
            throw new Error(`Alpha must be 0.0-100.0, got ${alpha}`);
        }

        // Retry loop
        let lastError;
        for (let attempt = 1; attempt <= this.maxRetries; attempt++) {
            try {
                return await this.doUpload(
                    filePath,
                    name,
                    description,
                    tier,
                    category,
                    scope,
                    rank,
                    alpha,
                    onProgress
                );
            } catch (error) {
                lastError = error;

                if (attempt < this.maxRetries) {
                    const isRetryable =
                        error.code === 'ETIMEDOUT' ||
                        error.code === 'ECONNREFUSED' ||
                        error.code === 'ENOTFOUND';

                    if (isRetryable) {
                        const delay = Math.pow(2, attempt) * this.retryDelay;
                        console.log(
                            `Attempt ${attempt} failed, retrying in ${delay}ms...`
                        );
                        await this.sleep(delay);
                        continue;
                    }
                }

                throw error;
            }
        }

        throw new Error(`Upload failed after ${this.maxRetries} attempts: ${lastError}`);
    }

    /**
     * Perform actual upload
     */
    async doUpload(
        filePath,
        name,
        description,
        tier,
        category,
        scope,
        rank,
        alpha,
        onProgress
    ) {
        const stats = fs.statSync(filePath);
        const fileSize = stats.size;

        // Adjust timeout based on file size (1 second per MB minimum)
        const adaptiveTimeout = Math.max(
            this.timeout,
            (fileSize / (1024 * 1024)) * 1000
        );

        const form = new FormData();

        // Create read stream with progress tracking
        const fileStream = fs.createReadStream(filePath);
        let uploadedBytes = 0;

        fileStream.on('data', (chunk) => {
            uploadedBytes += chunk.length;
            if (onProgress) {
                onProgress(uploadedBytes, fileSize);
            }
        });

        form.append('file', fileStream, {
            filename: path.basename(filePath),
        });
        form.append('name', name);
        form.append('tier', tier);
        form.append('category', category);
        form.append('scope', scope);
        form.append('rank', String(rank));
        form.append('alpha', String(alpha));

        if (description) {
            form.append('description', description);
        }

        const controller = new AbortController();
        const timeout = setTimeout(
            () => controller.abort(),
            adaptiveTimeout
        );

        try {
            const response = await fetch(
                `${this.baseUrl}/v1/adapters/upload-aos`,
                {
                    method: 'POST',
                    headers: {
                        'Authorization': `Bearer ${this.token}`,
                        ...form.getHeaders(),
                    },
                    body: form,
                    signal: controller.signal,
                }
            );

            // Handle HTTP errors
            if (response.status === 400) {
                const error = await response.json();
                throw new ValueError(
                    `Validation error (${error.error_code}): ${error.message}`
                );
            } else if (response.status === 403) {
                throw new PermissionError(
                    'Insufficient permissions (need Admin or Operator role)'
                );
            } else if (response.status === 409) {
                throw new Error('Adapter ID conflict (UUID collision, retry)');
            } else if (response.status === 413) {
                throw new Error('File too large for endpoint');
            } else if (response.status === 507) {
                throw new Error('Server disk space exhausted');
            }

            if (!response.ok) {
                const error = await response.json();
                throw new Error(
                    `Upload failed (${response.status}): ` +
                    `${error.error_code} - ${error.message}`
                );
            }

            return response.json();
        } finally {
            clearTimeout(timeout);
        }
    }

    sleep(ms) {
        return new Promise((resolve) => setTimeout(resolve, ms));
    }
}

/**
 * Create minimal valid .aos file for testing
 */
function createTestAosFile(filePath) {
    const manifest = {
        version: '1.0.0',
        model_type: 'lora',
        base_model: 'llama',
        rank: 16,
        alpha: 8.0,
    };

    const manifestJson = JSON.stringify(manifest);
    const manifestBuffer = Buffer.from(manifestJson, 'utf8');

    const buffer = Buffer.alloc(8 + manifestBuffer.length);

    // Write header (little-endian)
    buffer.writeUInt32LE(8, 0); // Manifest offset
    buffer.writeUInt32LE(manifestBuffer.length, 4); // Manifest length

    // Write manifest
    manifestBuffer.copy(buffer, 8);

    fs.writeFileSync(filePath, buffer);
}

/**
 * Example 1: Simple upload
 */
async function exampleSimpleUpload() {
    console.log('='.repeat(60));
    console.log('Example 1: Simple Upload');
    console.log('='.repeat(60));

    const token = process.env.JWT_TOKEN || 'test-token';
    const apiUrl = process.env.API_URL || 'http://localhost:8080';

    const uploader = new SimpleUploader(apiUrl, token);

    // Create test file
    const testFile = '/tmp/test_adapter.aos';
    createTestAosFile(testFile);

    try {
        const result = await uploader.upload(testFile, 'Simple Test Adapter');
        console.log('✓ Upload successful!');
        console.log(`  Adapter ID: ${result.adapter_id}`);
        console.log(`  Hash: ${result.hash_b3}`);
        console.log(`  State: ${result.lifecycle_state}`);
    } catch (error) {
        console.error(`✗ Upload failed: ${error.message}`);
    } finally {
        if (fs.existsSync(testFile)) {
            fs.unlinkSync(testFile);
        }
    }
}

/**
 * Example 2: Production upload with validation
 */
async function exampleProductionUpload() {
    console.log('\n' + '='.repeat(60));
    console.log('Example 2: Production Upload');
    console.log('='.repeat(60));

    const token = process.env.JWT_TOKEN || 'test-token';
    const apiUrl = process.env.API_URL || 'http://localhost:8080';

    const uploader = new ProductionUploader(apiUrl, token, {
        maxRetries: 3,
        timeout: 60000,
    });

    // Create test file
    const testFile = '/tmp/production_adapter.aos';
    createTestAosFile(testFile);

    try {
        const result = await uploader.upload(
            testFile,
            'Production Test Adapter',
            {
                description: 'Tested adapter for production',
                tier: 'persistent',
                category: 'code',
                rank: 16,
                alpha: 8.0,
                onProgress: (done, total) => {
                    const pct = ((done / total) * 100).toFixed(1);
                    process.stdout.write(`  Progress: ${pct}%\r`);
                },
            }
        );

        console.log('\n✓ Upload successful!');
        console.log(`  Adapter ID: ${result.adapter_id}`);
        console.log(`  File size: ${result.file_size} bytes`);
        console.log(`  Hash: ${result.hash_b3}`);
        console.log(`  State: ${result.lifecycle_state}`);
    } catch (error) {
        console.error(`✗ Upload failed: ${error.message}`);
    } finally {
        if (fs.existsSync(testFile)) {
            fs.unlinkSync(testFile);
        }
    }
}

/**
 * Example 3: Batch upload with rate limiting
 */
async function exampleBatchUpload() {
    console.log('\n' + '='.repeat(60));
    console.log('Example 3: Batch Upload');
    console.log('='.repeat(60));

    const token = process.env.JWT_TOKEN || 'test-token';
    const apiUrl = process.env.API_URL || 'http://localhost:8080';

    const uploader = new ProductionUploader(apiUrl, token);

    // Create multiple test files
    const testFiles = [];
    for (let i = 0; i < 3; i++) {
        const testFile = `/tmp/batch_adapter_${i}.aos`;
        createTestAosFile(testFile);
        testFiles.push(testFile);
    }

    try {
        const results = [];
        for (let i = 0; i < testFiles.length; i++) {
            console.log(`Uploading batch_adapter_${i}.aos...`);
            const result = await uploader.upload(
                testFiles[i],
                `Batch Adapter ${i}`,
                { tier: 'ephemeral' }
            );
            results.push(result);
            console.log(`  ✓ ${result.adapter_id}`);

            // Rate limiting: ~2 uploads per second
            if (i < testFiles.length - 1) {
                await new Promise((resolve) => setTimeout(resolve, 500));
            }
        }

        console.log(
            `\n✓ Batch upload complete (${results.length} adapters)`
        );
        results.forEach((result) => {
            console.log(
                `  - ${result.adapter_id}: ${result.lifecycle_state}`
            );
        });
    } catch (error) {
        console.error(`✗ Batch upload failed: ${error.message}`);
    } finally {
        for (const testFile of testFiles) {
            if (fs.existsSync(testFile)) {
                fs.unlinkSync(testFile);
            }
        }
    }
}

/**
 * Example 4: Error handling patterns
 */
async function exampleErrorHandling() {
    console.log('\n' + '='.repeat(60));
    console.log('Example 4: Error Handling');
    console.log('='.repeat(60));

    const token = process.env.JWT_TOKEN || 'test-token';
    const apiUrl = process.env.API_URL || 'http://localhost:8080';

    const uploader = new ProductionUploader(apiUrl, token);

    // Example 4a: File validation errors
    console.log('\n4a. Testing validation errors:');

    try {
        uploader.validateFile('/nonexistent/file.aos');
    } catch (error) {
        console.log(`  ✓ Caught: ${error.message}`);
    }

    try {
        const testFile = '/tmp/wrong_ext.txt';
        fs.writeFileSync(testFile, 'test');
        uploader.validateFile(testFile);
    } catch (error) {
        console.log(`  ✓ Caught: ${error.message}`);
    }

    try {
        await uploader.upload('valid.aos', 'Test', { rank: 1000 });
    } catch (error) {
        console.log(`  ✓ Caught: ${error.message}`);
    }

    // Example 4b: Network errors (conceptual)
    console.log('\n4b. Network retry strategy:');
    console.log('  Timeout → Wait 2s → Retry');
    console.log('  Connection error → Wait 4s → Retry');
    console.log('  After 3 attempts → Fail with descriptive error');
}

/**
 * Main entry point
 */
async function main() {
    console.log('\nAdapterOS Upload Examples');
    console.log(
        'Set JWT_TOKEN and API_URL environment variables\n'
    );

    try {
        await exampleSimpleUpload();
        await exampleProductionUpload();
        await exampleBatchUpload();
        await exampleErrorHandling();

        console.log('\n' + '='.repeat(60));
        console.log('Examples complete!');
        console.log('='.repeat(60));
    } catch (error) {
        console.error('Fatal error:', error);
        process.exit(1);
    }
}

// Run examples if this is the main module
if (require.main === module) {
    main().catch(console.error);
}

module.exports = { SimpleUploader, ProductionUploader, createTestAosFile };
