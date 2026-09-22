#!/usr/bin/env node
// i18n-key-checker.js - Verify all t() calls reference valid keys
import { readFileSync, readdirSync, statSync } from 'fs';
import { join, extname } from 'path';
import { fileURLToPath } from 'url';
import { dirname } from 'path';
import { getKeys } from './i18n.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Get all valid keys from i18n.js
const validKeys = new Set([...getKeys('en'), ...getKeys('ko')]);

// Find all JS files in the web/js directory
function findJsFiles(dir) {
  const files = [];
  const entries = readdirSync(dir);

  for (const entry of entries) {
    const fullPath = join(dir, entry);
    const stat = statSync(fullPath);

    if (stat.isDirectory()) {
      files.push(...findJsFiles(fullPath));
    } else if (extname(entry) === '.js' && !entry.includes('test') && !entry.includes('i18n-key-checker')) {
      files.push(fullPath);
    }
  }

  return files;
}

// Extract t() calls from a file
function extractTcalls(content) {
  const calls = [];
  // Match t('key') or t('key', { ... }) - must be standalone function call
  // Use word boundary and exclude import, etc.
  const regex = /(?:^|[^a-zA-Z_])t\(['"]([^'"]+)['"]/gm;
  let match;

  while ((match = regex.exec(content)) !== null) {
    calls.push({
      key: match[1],
      position: match.index
    });
  }

  return calls;
}

// Check all files
const jsFiles = findJsFiles(__dirname);
let errors = 0;
let warnings = 0;
let totalCalls = 0;

console.log('🔍 Checking i18n key references...\n');

for (const file of jsFiles) {
  const content = readFileSync(file, 'utf-8');
  const calls = extractTcalls(content);

  for (const call of calls) {
    totalCalls++;
    if (!validKeys.has(call.key)) {
      const relativePath = file.replace(__dirname, '.');
      console.error(`❌ ${relativePath}: Invalid key "${call.key}"`);
      errors++;
    }
  }
}

console.log(`\n📊 Summary:`);
console.log(`   Total t() calls: ${totalCalls}`);
console.log(`   Valid keys: ${validKeys.size}`);
console.log(`   Errors: ${errors}`);

if (errors > 0) {
  console.error(`\n❌ Found ${errors} invalid key reference(s)`);
  process.exit(1);
} else {
  console.log(`\n✅ All t() calls reference valid keys`);
  process.exit(0);
}
