import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..');

// Determine the Tauri binary path based on platform
const platform = process.platform;
const binaryName = platform === 'win32' ? 'lens.exe' : 'lens';
const binaryPath = path.join(repoRoot, 'apps', 'lens', 'target', 'release', binaryName);

export const config = {
  //
  // ====================
  // Runner Configuration
  // ====================
  runner: 'local',

  //
  // ==================
  // Specify Test Files
  // ==================
  specs: [
    './specs/**/*.js'
  ],

  //
  // ============
  // Capabilities
  // ============
  capabilities: [{
    maxInstances: 1,
    browserName: 'tauri',
    'tauri:options': {
      application: binaryPath,
      args: [],
    },
  }],

  //
  // ===================
  // Test Configurations
  // ===================
  logLevel: 'info',
  bail: 0,
  waitforTimeout: 10000,
  connectionRetryTimeout: 120000,
  connectionRetryCount: 3,

  //
  // ========
  // Services
  // ========
  services: [
    ['tauri', {
      application: binaryPath,
      captureBackendLogs: true,
    }]
  ],

  //
  // =====
  // Hooks
  // =====
  onPrepare: async () => {
    // Verify binary exists before running tests
    const fs = await import('node:fs');
    if (!fs.existsSync(binaryPath)) {
      throw new Error(`Tauri binary not found at ${binaryPath}. Run 'cargo build --release' first.`);
    }
    console.log(`✓ Using Tauri binary: ${binaryPath}`);
  },

  //
  // ======
  // Reporter
  // ======
  reporters: ['spec'],

  //
  // =====
  // Framework
  // =====
  framework: 'mocha',
  mochaOpts: {
    ui: 'bdd',
    timeout: 60000
  },
};
