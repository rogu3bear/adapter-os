import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const playwrightRoot = path.resolve(__dirname, '..');
const visualSpecPath = path.resolve(playwrightRoot, 'ui/visual.spec.ts');
const snapshotsDir = path.resolve(playwrightRoot, 'ui/visual.spec.ts-snapshots');

const CANONICAL_OS = 'darwin';
const PROJECTS = ['chromium', 'webkit'];

function screenshotFilenameForProject(name, project) {
  const ext = path.extname(name) || '.png';
  const stem = ext ? name.slice(0, -ext.length) : name;
  return `${stem}-${project}-${CANONICAL_OS}${ext}`;
}

function parseEnableChatVisualsFlag(source) {
  const match = source.match(/const\s+ENABLE_CHAT_VISUALS\s*=\s*(true|false)\s*;/);
  if (!match) {
    return true;
  }
  return match[1] === 'true';
}

function collectScreenshotSets(source, enableChatVisuals) {
  const allReferenced = new Set();
  const activeReferenced = new Set();
  const testBlockRegex = /^  test\([^]*?^  \}\);\s*$/gm;

  for (const blockMatch of source.matchAll(testBlockRegex)) {
    const block = blockMatch[0];
    const blockDisabled = !enableChatVisuals && block.includes('test.skip(!ENABLE_CHAT_VISUALS');

    for (const screenshotMatch of block.matchAll(/toHaveScreenshot\(\s*['"`]([^'"`]+)['"`]/g)) {
      const screenshot = screenshotMatch[1];
      allReferenced.add(screenshot);
      if (!blockDisabled) {
        activeReferenced.add(screenshot);
      }
    }
  }

  return { allReferenced, activeReferenced };
}

function expandExpectedFiles(screenshotNames) {
  const files = new Set();
  for (const screenshot of screenshotNames) {
    for (const project of PROJECTS) {
      files.add(screenshotFilenameForProject(screenshot, project));
    }
  }
  return files;
}

function listSnapshotFiles(dirPath) {
  if (!fs.existsSync(dirPath)) {
    return [];
  }
  return fs
    .readdirSync(dirPath)
    .filter((entry) => entry.endsWith('.png'))
    .sort();
}

function reportFailure(title, items) {
  console.error(`\n[visual-contract] ${title}`);
  for (const item of items) {
    console.error(`  - ${item}`);
  }
}

const source = fs.readFileSync(visualSpecPath, 'utf8');
const enableChatVisuals = parseEnableChatVisualsFlag(source);
const { allReferenced, activeReferenced } = collectScreenshotSets(source, enableChatVisuals);

const allExpectedFiles = expandExpectedFiles(allReferenced);
const activeExpectedFiles = expandExpectedFiles(activeReferenced);
const existingFiles = listSnapshotFiles(snapshotsDir);
const existingSet = new Set(existingFiles);

const missingActive = [...activeExpectedFiles].filter((file) => !existingSet.has(file)).sort();
const orphanFiles = existingFiles.filter((file) => !allExpectedFiles.has(file));

if (missingActive.length > 0 || orphanFiles.length > 0) {
  if (missingActive.length > 0) {
    reportFailure('Missing active visual baselines:', missingActive);
  }
  if (orphanFiles.length > 0) {
    reportFailure('Orphan visual baselines (no matching toHaveScreenshot reference):', orphanFiles);
  }
  process.exit(1);
}

console.log(
  `[visual-contract] OK: ${activeReferenced.size} active screenshots, ${allReferenced.size} total references, ${existingFiles.length} baseline files (${CANONICAL_OS} canonical).`
);
