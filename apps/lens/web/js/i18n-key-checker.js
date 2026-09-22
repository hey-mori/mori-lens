#!/usr/bin/env node
// i18n-key-checker.js - Comprehensive i18n integrity checker
// Verifies: duplicate declarations, key parity, HTML refs, JS t() refs
import { readFileSync } from 'fs';
import { join } from 'path';
import { fileURLToPath } from 'url';
import { dirname } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

let errors = 0;

function fail(msg) {
  console.error('  FAIL: ' + msg);
  errors++;
}

function pass(msg) {
  console.log('  PASS: ' + msg);
}

// ─── 1. Duplicate key detection (source-level) ──────────────────────
console.log('\n1. Duplicate key detection (source-level)');

const i18nSrc = readFileSync(join(__dirname, 'i18n.js'), 'utf-8');
const enStart = i18nSrc.indexOf('en:');
const koStart = i18nSrc.indexOf('ko:');

function extractKeysFromBlock(block) {
  const keys = [];
  const regex = /^\s*'([^']+)':/gm;
  let m;
  while ((m = regex.exec(block)) !== null) keys.push(m[1]);
  return keys;
}

const enBlock = i18nSrc.substring(enStart, koStart);
const koBlock = i18nSrc.substring(koStart);
const enKeysAll = extractKeysFromBlock(enBlock);
const koKeysAll = extractKeysFromBlock(koBlock);

const enUnique = [...new Set(enKeysAll)];
const koUnique = [...new Set(koKeysAll)];
const enDups = enKeysAll.filter((k, i) => enKeysAll.indexOf(k) !== i);
const koDups = koKeysAll.filter((k, i) => koKeysAll.indexOf(k) !== i);

console.log(`   English declarations: ${enKeysAll.length}`);
console.log(`   English unique keys: ${enUnique.length}`);
console.log(`   English duplicates: ${enDups.length}`);
if (enDups.length > 0) fail('Duplicate English keys: ' + [...new Set(enDups)].join(', '));
else pass('No duplicate English keys');

console.log(`   Korean declarations: ${koKeysAll.length}`);
console.log(`   Korean unique keys: ${koUnique.length}`);
console.log(`   Korean duplicates: ${koDups.length}`);
if (koDups.length > 0) fail('Duplicate Korean keys: ' + [...new Set(koDups)].join(', '));
else pass('No duplicate Korean keys');

// ─── 2. en/ko key parity ───────────────────────────────────────────
console.log('\n2. en/ko unique key parity');

const enSet = new Set(enUnique);
const koSet = new Set(koUnique);
const missingInKo = enUnique.filter(k => !koSet.has(k));
const missingInEn = koUnique.filter(k => !enSet.has(k));

console.log(`   Missing in ko: ${missingInKo.length === 0 ? 'none' : missingInKo.join(', ')}`);
console.log(`   Missing in en: ${missingInEn.length === 0 ? 'none' : missingInEn.join(', ')}`);

if (missingInKo.length === 0 && missingInEn.length === 0) pass('en/ko key sets are identical');
else {
  if (missingInKo.length > 0) fail('Keys in en but not ko: ' + missingInKo.join(', '));
  if (missingInEn.length > 0) fail('Keys in ko but not en: ' + missingInEn.join(', '));
}

// ─── 3. HTML data-i18n reference validation ─────────────────────────
console.log('\n3. HTML data-i18n reference validation');

const validKeys = enSet; // identical to koSet after parity check

function checkHtmlRefs() {
  let htmlPath = join(__dirname, '..', 'index.html');
  let htmlSrc;
  try {
    htmlSrc = readFileSync(htmlPath, 'utf-8');
  } catch {
    console.log('   SKIP: index.html not found');
    return;
  }

  // data-i18n="key"
  const dataI18nRegex = /data-i18n="([^"]+)"/g;
  let m;
  let htmlRefs = 0;
  let htmlErrors = 0;
  while ((m = dataI18nRegex.exec(htmlSrc)) !== null) {
    htmlRefs++;
    if (!validKeys.has(m[1])) {
      fail('HTML data-i18n: unknown key "' + m[1] + '"');
      htmlErrors++;
    }
  }

  // data-i18n-aria="key"
  const dataI18nAriaRegex = /data-i18n-aria="([^"]+)"/g;
  while ((m = dataI18nAriaRegex.exec(htmlSrc)) !== null) {
    htmlRefs++;
    if (!validKeys.has(m[1])) {
      fail('HTML data-i18n-aria: unknown key "' + m[1] + '"');
      htmlErrors++;
    }
  }

  // data-i18n-title="key"
  const dataI18nTitleRegex = /data-i18n-title="([^"]+)"/g;
  while ((m = dataI18nTitleRegex.exec(htmlSrc)) !== null) {
    htmlRefs++;
    if (!validKeys.has(m[1])) {
      fail('HTML data-i18n-title: unknown key "' + m[1] + '"');
      htmlErrors++;
    }
  }

  console.log(`   HTML key references checked: ${htmlRefs}`);
  if (htmlErrors === 0) pass('All HTML data-i18n refs valid');
}

checkHtmlRefs();

// ─── 4. JS t() literal key reference validation ─────────────────────
console.log('\n4. JS literal t() reference validation');

const { readdirSync, statSync } = await import('fs');
const { extname } = await import('path');

function findJsFiles(dir) {
  const files = [];
  for (const entry of readdirSync(dir)) {
    const fullPath = join(dir, entry);
    const stat = statSync(fullPath);
    if (stat.isDirectory()) files.push(...findJsFiles(fullPath));
    else if (extname(entry) === '.js' && !entry.includes('test') && !entry.includes('i18n-key-checker'))
      files.push(fullPath);
  }
  return files;
}

const jsFiles = findJsFiles(__dirname);
let totalCalls = 0;
let jsErrors = 0;

for (const file of jsFiles) {
  const content = readFileSync(file, 'utf-8');
  const regex = /(?:^|[^a-zA-Z_])t\(['"]([^'"]+)['"]/gm;
  let m;
  while ((m = regex.exec(content)) !== null) {
    totalCalls++;
    if (!validKeys.has(m[1])) {
      const rel = file.replace(__dirname, '.');
      fail(`JS t() in ${rel}: unknown key "${m[1]}"`);
      jsErrors++;
    }
  }
}

console.log(`   JS literal t() refs checked: ${totalCalls}`);
if (jsErrors === 0) pass('All JS t() refs valid');

// ─── Summary ────────────────────────────────────────────────────────
console.log('\n=== Summary ===');
console.log(`   English declarations = ${enKeysAll.length}`);
console.log(`   English unique keys = ${enUnique.length}`);
console.log(`   English duplicates = ${enDups.length}`);
console.log(`   Korean declarations = ${koKeysAll.length}`);
console.log(`   Korean unique keys = ${koUnique.length}`);
console.log(`   Korean duplicates = ${koDups.length}`);
console.log(`   en/ko parity = ${missingInKo.length === 0 && missingInEn.length === 0 ? 'PASS' : 'FAIL'}`);
console.log(`   duplicate keys = ${enDups.length + koDups.length}`);
console.log(`   missing references = ${errors}`);

if (errors > 0) {
  console.error('\n❌ Integrity check FAILED with ' + errors + ' error(s)');
  process.exit(1);
} else {
  console.log('\n✅ All integrity checks passed');
  process.exit(0);
}
