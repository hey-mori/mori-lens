// The packaged Control Center reaches Rust through window.__TAURI__, which
// Tauri defines only when app.withGlobalTauri is set. Every other test mocks
// ./tauri.js, so none of them would notice the global missing; v0.1.0-rc.3
// shipped that way and its Control Center never got past "Loading...".
import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

const appRoot = process.cwd();
const config = JSON.parse(readFileSync(join(appRoot, 'tauri.conf.json'), 'utf8'));

function webSources(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return webSources(path);
    if (!/\.(html|js)$/.test(entry.name) || entry.name.includes('.test.')) return [];
    return [path];
  });
}

describe('the Tauri global the frontend depends on', () => {
  const users = webSources(join(appRoot, config.build.frontendDist)).filter((file) =>
    readFileSync(file, 'utf8').includes('window.__TAURI__'),
  );

  it('is used by the frontend, so this test guards something', () => {
    expect(users.length).toBeGreaterThan(0);
  });

  it('is enabled in tauri.conf.json', () => {
    expect(config.app.withGlobalTauri).toBe(true);
  });
});
