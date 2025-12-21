#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const projectRoot = path.resolve(__dirname, '..', '..');
const openApiPath = path.join(projectRoot, 'target', 'codegen', 'openapi.json');
const outputPath = path.join(__dirname, '..', 'src', 'api', 'generated.ts');

console.log('🔧 AdapterOS OpenAPI Type Generator\n');

// Step 1: Generate OpenAPI spec
console.log('📋 Step 1: Generating OpenAPI specification...');
console.log(`   Command: cargo run -p adapteros-server-api --bin export-openapi -- ${openApiPath}`);

const exportResult = spawnSync(
  'cargo',
  [
    'run',
    '-p', 'adapteros-server-api',
    '--bin', 'export-openapi',
    '--',
    openApiPath
  ],
  {
    cwd: projectRoot,
    stdio: 'inherit',
    shell: false
  }
);

if (exportResult.status !== 0) {
  console.error('\n❌ Error: Failed to generate OpenAPI spec');
  process.exit(exportResult.status ?? 1);
}

// Step 2: Validate OpenAPI spec
console.log('\n✓ OpenAPI spec generated successfully');
console.log('\n🔍 Step 2: Validating OpenAPI specification...');

if (!existsSync(openApiPath)) {
  console.error(`\n❌ Error: OpenAPI spec not found at ${openApiPath}`);
  process.exit(1);
}

let spec;
try {
  const specContent = readFileSync(openApiPath, 'utf-8');
  spec = JSON.parse(specContent);
} catch (err) {
  console.error(`\n❌ Error: Failed to parse OpenAPI spec: ${err.message}`);
  process.exit(1);
}

// Validate spec has required schemas
const pathCount = spec.paths ? Object.keys(spec.paths).length : 0;
const schemaCount = spec.components?.schemas ? Object.keys(spec.components.schemas).length : 0;

if (pathCount === 0) {
  console.error('\n❌ Error: OpenAPI spec contains no paths');
  process.exit(1);
}

if (schemaCount === 0) {
  console.error('\n❌ Error: OpenAPI spec contains no schema components');
  process.exit(1);
}

console.log(`   Paths: ${pathCount}`);
console.log(`   Schemas: ${schemaCount}`);
console.log('✓ OpenAPI spec validation passed');

// Step 3: Ensure output directory exists
console.log('\n📁 Step 3: Ensuring output directory exists...');
const outputDir = path.dirname(outputPath);
if (!existsSync(outputDir)) {
  mkdirSync(outputDir, { recursive: true });
  console.log(`   Created directory: ${outputDir}`);
} else {
  console.log(`   Directory exists: ${outputDir}`);
}

// Step 4: Generate TypeScript types
console.log('\n🔨 Step 4: Generating TypeScript types...');
console.log(`   Input:  ${openApiPath}`);
console.log(`   Output: ${outputPath}`);

const openapiTypescriptResult = spawnSync(
  'npx',
  [
    'openapi-typescript',
    openApiPath,
    '-o', outputPath,
    '--export-type',
    '--alphabetize'
  ],
  {
    cwd: projectRoot,
    stdio: 'inherit',
    shell: false
  }
);

if (openapiTypescriptResult.status !== 0) {
  console.error('\n❌ Error: Failed to generate TypeScript types');
  process.exit(openapiTypescriptResult.status ?? 1);
}

// Final validation
if (!existsSync(outputPath)) {
  console.error(`\n❌ Error: Generated types file not found at ${outputPath}`);
  process.exit(1);
}

console.log('\n✅ TypeScript types generated successfully!');
console.log(`\n📄 Output: ${outputPath}`);
console.log('\n🎉 Type generation complete!\n');
