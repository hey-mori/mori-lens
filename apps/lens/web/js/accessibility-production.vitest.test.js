// Production-markup accessibility tests for P20C readiness states.
// Uses real renderReadinessSection() output, not synthetic ARIA markup.
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createControlCenterFixture, cleanupFixture } from './test-infrastructure.js';
import axe from 'axe-core';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  t: vi.fn((key) => key),
}));

vi.mock('./tauri.js', () => ({
  invoke: mocks.invoke,
}));

vi.mock('./i18n.js', () => ({
  t: mocks.t,
}));

import { renderReadinessSection } from './readiness.js';

// Exact wire-format readiness fixtures matching Rust serde contract
const STATES = {
  ready: {
    quick_lane: 'ready',
    quick_provider: 'ollama',
    quick_model: 'qwen2.5:7b',
    quick_effective_model: 'qwen2.5:7b',
    quick_provider_access: 'available',
    quick_model_inventory: { available: { models: ['qwen2.5:7b'] } },
    quick_next_action: 'none',
    quick_can_download: false,
    quick_can_load: false,
    quick_supports_progress: true,
    quick_download_target: null,
    download: null,
    deep_lane: 'ready',
    deep_provider: 'ollama',
    deep_model: 'qwen2.5:14b',
    deep_configuration_valid: true,
  },

  endpoint_unreachable: {
    quick_lane: 'endpoint_unreachable',
    quick_provider: 'ollama',
    quick_model: null,
    quick_effective_model: null,
    quick_provider_access: 'unreachable',
    quick_model_inventory: null,
    quick_next_action: 'refresh',
    quick_can_download: false,
    quick_can_load: false,
    quick_supports_progress: false,
    quick_download_target: null,
    download: null,
    deep_lane: 'ready',
    deep_provider: 'ollama',
    deep_model: 'qwen2.5:14b',
    deep_configuration_valid: true,
  },

  authentication_required: {
    quick_lane: 'authentication_required',
    quick_provider: 'lmstudio',
    quick_model: 'qwen2.5:7b',
    quick_effective_model: 'qwen2.5:7b',
    quick_provider_access: 'authentication_required',
    quick_model_inventory: { available: { models: ['qwen2.5:7b'] } },
    quick_next_action: 'resolve_auth',
    quick_can_download: false,
    quick_can_load: false,
    quick_supports_progress: false,
    quick_download_target: null,
    download: null,
    deep_lane: 'ready',
    deep_provider: 'ollama',
    deep_model: 'qwen2.5:14b',
    deep_configuration_valid: true,
  },

  provider_error: {
    quick_lane: 'provider_error',
    quick_provider: 'ollama',
    quick_model: null,
    quick_effective_model: null,
    quick_provider_access: 'provider_error',
    quick_model_inventory: null,
    quick_next_action: 'refresh',
    quick_can_download: false,
    quick_can_load: false,
    quick_supports_progress: false,
    quick_download_target: null,
    download: null,
    deep_lane: 'ready',
    deep_provider: 'ollama',
    deep_model: 'qwen2.5:14b',
    deep_configuration_valid: true,
  },

  download_available: {
    quick_lane: 'no_compatible_model',
    quick_provider: 'ollama',
    quick_model: null,
    quick_effective_model: null,
    quick_provider_access: 'available',
    quick_model_inventory: { available: { models: [] } },
    quick_next_action: 'download',
    quick_can_download: true,
    quick_can_load: false,
    quick_supports_progress: true,
    quick_download_target: null,
    download: null,
    deep_lane: 'ready',
    deep_provider: 'ollama',
    deep_model: 'qwen2.5:14b',
    deep_configuration_valid: true,
  },

  load_available: {
    quick_lane: 'model_present',
    quick_provider: 'lmstudio',
    quick_model: 'qwen2.5:7b',
    quick_effective_model: 'qwen2.5:7b',
    quick_provider_access: 'available',
    quick_model_inventory: { available: { models: ['qwen2.5:7b'] } },
    quick_next_action: 'load',
    quick_can_download: false,
    quick_can_load: true,
    quick_supports_progress: false,
    quick_download_target: null,
    download: null,
    deep_lane: 'ready',
    deep_provider: 'ollama',
    deep_model: 'qwen2.5:14b',
    deep_configuration_valid: true,
  },

  downloading: {
    quick_lane: 'model_downloading',
    quick_provider: 'ollama',
    quick_model: null,
    quick_effective_model: null,
    quick_provider_access: 'available',
    quick_model_inventory: { available: { models: [] } },
    quick_next_action: 'none',
    quick_can_download: false,
    quick_can_load: false,
    quick_supports_progress: true,
    quick_download_target: null,
    download: {
      active: true,
      provider: 'ollama',
      model: null,
      status: { downloading: { bytes_downloaded: 524288000, total_bytes: 1073741824, bytes_per_second: 52428800, estimated_completion_seconds: 10 } },
    },
    deep_lane: 'ready',
    deep_provider: 'ollama',
    deep_model: 'qwen2.5:14b',
    deep_configuration_valid: true,
  },

  paused: {
    quick_lane: 'model_downloading',
    quick_provider: 'ollama',
    quick_model: null,
    quick_effective_model: null,
    quick_provider_access: 'available',
    quick_model_inventory: { available: { models: [] } },
    quick_next_action: 'none',
    quick_can_download: false,
    quick_can_load: false,
    quick_supports_progress: true,
    quick_download_target: null,
    download: {
      active: true,
      provider: 'ollama',
      model: null,
      status: { paused: { bytes_downloaded: 524288000, total_bytes: 1073741824, bytes_per_second: 0, estimated_completion_seconds: null } },
    },
    deep_lane: 'ready',
    deep_provider: 'ollama',
    deep_model: 'qwen2.5:14b',
    deep_configuration_valid: true,
  },

  download_failed: {
    quick_lane: 'model_failed',
    quick_provider: 'ollama',
    quick_model: null,
    quick_effective_model: null,
    quick_provider_access: 'available',
    quick_model_inventory: { available: { models: [] } },
    quick_next_action: 'refresh',
    quick_can_download: false,
    quick_can_load: false,
    quick_supports_progress: true,
    quick_download_target: null,
    download: {
      active: false,
      provider: 'ollama',
      model: null,
      status: { failed: 'download_failed' },
    },
    deep_lane: 'ready',
    deep_provider: 'ollama',
    deep_model: 'qwen2.5:14b',
    deep_configuration_valid: true,
  },

  unsupported_deep: {
    quick_lane: 'ready',
    quick_provider: 'ollama',
    quick_model: 'qwen2.5:7b',
    quick_effective_model: 'qwen2.5:7b',
    quick_provider_access: 'available',
    quick_model_inventory: { available: { models: ['qwen2.5:7b'] } },
    quick_next_action: 'none',
    quick_can_download: false,
    quick_can_load: false,
    quick_supports_progress: true,
    quick_download_target: null,
    download: null,
    deep_lane: 'unknown',
    deep_provider: null,
    deep_model: null,
    deep_configuration_valid: false,
  },
};

function mountReadinessHtml(readiness) {
  const container = document.getElementById('status-readiness-region');
  container.innerHTML = renderReadinessSection(readiness);
  return container;
}

describe('P20C production-markup accessibility', () => {
  let fixture;

  beforeEach(() => {
    fixture = createControlCenterFixture();
    const statusSection = document.getElementById('status-section');
    statusSection.innerHTML = `
      <div id="status-content">
        <div id="status-readiness-region"></div>
      </div>
    `;
  });

  afterEach(() => {
    cleanupFixture(fixture);
  });

  it('Ready state passes axe', async () => {
    const region = mountReadinessHtml(STATES.ready);
    const results = await axe.run(region);
    expect(results.violations).toEqual([]);
  });

  it('EndpointUnreachable state passes axe', async () => {
    const region = mountReadinessHtml(STATES.endpoint_unreachable);
    const results = await axe.run(region);
    expect(results.violations).toEqual([]);
  });

  it('AuthenticationRequired state passes axe', async () => {
    const region = mountReadinessHtml(STATES.authentication_required);
    const results = await axe.run(region);
    expect(results.violations).toEqual([]);
  });

  it('ProviderError state passes axe', async () => {
    const region = mountReadinessHtml(STATES.provider_error);
    const results = await axe.run(region);
    expect(results.violations).toEqual([]);
  });

  it('Download action state passes axe', async () => {
    const region = mountReadinessHtml(STATES.download_available);
    const results = await axe.run(region);
    expect(results.violations).toEqual([]);
  });

  it('Load action state passes axe', async () => {
    const region = mountReadinessHtml(STATES.load_available);
    const results = await axe.run(region);
    expect(results.violations).toEqual([]);
  });

  it('Downloading progress state passes axe', async () => {
    const region = mountReadinessHtml(STATES.downloading);
    const results = await axe.run(region);
    expect(results.violations).toEqual([]);
  });

  it('Paused download state passes axe', async () => {
    const region = mountReadinessHtml(STATES.paused);
    const results = await axe.run(region);
    expect(results.violations).toEqual([]);
  });

  it('Failed download state passes axe', async () => {
    const region = mountReadinessHtml(STATES.download_failed);
    const results = await axe.run(region);
    expect(results.violations).toEqual([]);
  });

  it('Unsupported Deep configuration state passes axe', async () => {
    const region = mountReadinessHtml(STATES.unsupported_deep);
    const results = await axe.run(region);
    expect(results.violations).toEqual([]);
  });

  it('null readiness (checking) state passes axe', async () => {
    const region = mountReadinessHtml(null);
    const results = await axe.run(region);
    expect(results.violations).toEqual([]);
  });
});
