// Readiness module behavioral tests
import { describe, it, expect, beforeEach, vi } from 'vitest';

// Use vi.hoisted to create mocks before vi.mock is hoisted
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  t: vi.fn((key) => key),
}));

// Mock the Tauri IPC boundary
vi.mock('./tauri.js', () => ({
  invoke: mocks.invoke,
}));

// Mock state module
vi.mock('./state.js', () => ({
  state: { cachedProviderReadiness: null, providerReadinessLoadState: null },
}));

// Mock i18n module
vi.mock('./i18n.js', () => ({
  t: mocks.t,
}));

// Import production module after mocking
import {
  LaneReadiness,
  NextAction,
  ProviderAccessState,
  normalizeAccessState,
  refreshProviderReadiness,
  renderReadinessSection,
} from './readiness.js';
import { state } from './state.js';

describe('Readiness constants', () => {
  it('defines all LaneReadiness variants', () => {
    expect(LaneReadiness.Unknown).toBe('unknown');
    expect(LaneReadiness.RuntimeNotInstalled).toBe('runtime_not_installed');
    expect(LaneReadiness.EndpointUnreachable).toBe('endpoint_unreachable');
    expect(LaneReadiness.AuthenticationRequired).toBe('authentication_required');
    expect(LaneReadiness.ProviderError).toBe('provider_error');
    expect(LaneReadiness.NoCompatibleModel).toBe('no_compatible_model');
    expect(LaneReadiness.ModelPresent).toBe('model_present');
    expect(LaneReadiness.ModelDownloading).toBe('model_downloading');
    expect(LaneReadiness.ModelLoading).toBe('model_loading');
    expect(LaneReadiness.ModelFailed).toBe('model_failed');
    expect(LaneReadiness.Ready).toBe('ready');
  });

  it('defines all NextAction variants', () => {
    expect(NextAction.None).toBe('none');
    expect(NextAction.Refresh).toBe('refresh');
    expect(NextAction.Download).toBe('download');
    expect(NextAction.Load).toBe('load');
    expect(NextAction.ConfigureModel).toBe('configure_model');
    expect(NextAction.ResolveAuth).toBe('resolve_auth');
  });

  it('defines all ProviderAccessState variants', () => {
    expect(ProviderAccessState.Available).toBe('available');
    expect(ProviderAccessState.Unknown).toBe('unknown');
    expect(ProviderAccessState.Unreachable).toBe('unreachable');
    expect(ProviderAccessState.AuthenticationRequired).toBe('authentication_required');
    expect(ProviderAccessState.ProviderError).toBe('provider_error');
  });

  it('normalizes string access states', () => {
    expect(normalizeAccessState('available')).toEqual({ kind: 'available' });
    expect(normalizeAccessState('unknown')).toEqual({ kind: 'unknown' });
    expect(normalizeAccessState('authentication_required')).toEqual({ kind: 'authentication_required' });
  });

  it('normalizes object access states with fields', () => {
    expect(normalizeAccessState({ unreachable: { reason: 'connection refused' } })).toEqual({
      kind: 'unreachable',
      reason: 'connection refused',
    });
    expect(normalizeAccessState({ provider_error: { reason: '500 Internal Server Error' } })).toEqual({
      kind: 'provider_error',
      reason: '500 Internal Server Error',
    });
  });

  it('returns unknown for malformed access states', () => {
    expect(normalizeAccessState(null)).toEqual({ kind: 'unknown' });
    expect(normalizeAccessState(undefined)).toEqual({ kind: 'unknown' });
    expect(normalizeAccessState({})).toEqual({ kind: 'unknown' });
    expect(normalizeAccessState({ a: 1, b: 2 })).toEqual({ kind: 'unknown' });
  });
});

describe('refreshProviderReadiness', () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    state.cachedProviderReadiness = null;
    state.providerReadinessLoadState = null;
  });

  it('caches readiness on success', async () => {
    const mockReadiness = {
      quick_lane: 'ready',
      quick_provider: 'ollama',
      deep_lane: 'unknown',
      deep_provider: 'ollama',
    };
    mocks.invoke.mockResolvedValue(mockReadiness);

    const result = await refreshProviderReadiness();

    expect(mocks.invoke).toHaveBeenCalledWith('get_provider_readiness');
    expect(result).toEqual(mockReadiness);
    expect(state.cachedProviderReadiness).toEqual(mockReadiness);
    expect(state.providerReadinessLoadState).toBe('LOADED');
  });

  it('sets error state on failure', async () => {
    mocks.invoke.mockRejectedValue(new Error('Backend unavailable'));

    const result = await refreshProviderReadiness();

    expect(result).toBeNull();
    expect(state.cachedProviderReadiness).toBeNull();
    expect(state.providerReadinessLoadState).toBe('ERROR');
  });
});

describe('renderReadinessSection', () => {
  beforeEach(() => {
    mocks.t.mockImplementation((key) => key);
  });

  it('renders checking state when readiness is null', () => {
    const html = renderReadinessSection(null);

    expect(html).toContain('Quick');
    expect(html).toContain('Deep');
    expect(html).toContain('status.checking');
  });

  it('renders Quick lane with ready state', () => {
    const readiness = {
      quick_lane: 'ready',
      quick_provider: 'ollama',
      quick_model: 'qwen2.5:7b',
      quick_effective_model: 'qwen2.5:7b',
      quick_provider_access: 'available',
      quick_model_inventory: { available: { models: ['qwen2.5:7b'] } },
      quick_next_action: 'none',
      quick_can_download: false,
      quick_can_load: false,
      quick_supports_progress: false,
      quick_download_target: null,
      deep_lane: 'unknown',
      deep_provider: 'ollama',
      deep_model: null,
      deep_provider_access: 'unknown',
      deep_model_inventory: 'not_attempted',
      deep_next_action: 'none',
      deep_can_download: false,
      deep_can_load: false,
      deep_supports_progress: false,
      download: null,
    };

    const html = renderReadinessSection(readiness);

    expect(html).toContain('Quick');
    expect(html).toContain('Ollama');
    expect(html).toContain('status.ready');
    expect(html).toContain('qwen2.5:7b');
  });

  it('renders authentication required state', () => {
    const readiness = {
      quick_lane: 'authentication_required',
      quick_provider: 'ollama',
      quick_model: null,
      quick_effective_model: null,
      quick_provider_access: 'authentication_required',
      quick_model_inventory: 'not_attempted',
      quick_next_action: 'resolve_auth',
      quick_can_download: false,
      quick_can_load: false,
      quick_supports_progress: false,
      quick_download_target: null,
      deep_lane: 'unknown',
      deep_provider: 'ollama',
      deep_model: null,
      deep_provider_access: 'unknown',
      deep_model_inventory: 'not_attempted',
      deep_next_action: 'none',
      deep_can_download: false,
      deep_can_load: false,
      deep_supports_progress: false,
      download: null,
    };

    const html = renderReadinessSection(readiness);

    expect(html).toContain('status.readiness.auth.required');
  });

  it('renders provider error state', () => {
    const readiness = {
      quick_lane: 'provider_error',
      quick_provider: 'ollama',
      quick_model: null,
      quick_effective_model: null,
      quick_provider_access: 'provider_error',
      quick_model_inventory: 'not_attempted',
      quick_next_action: 'refresh',
      quick_can_download: false,
      quick_can_load: false,
      quick_supports_progress: false,
      quick_download_target: null,
      deep_lane: 'unknown',
      deep_provider: 'ollama',
      deep_model: null,
      deep_provider_access: 'unknown',
      deep_model_inventory: 'not_attempted',
      deep_next_action: 'none',
      deep_can_download: false,
      deep_can_load: false,
      deep_supports_progress: false,
      download: null,
    };

    const html = renderReadinessSection(readiness);

    expect(html).toContain('status.readiness.error');
  });

  it('renders LM Studio provider label', () => {
    const readiness = {
      quick_lane: 'ready',
      quick_provider: 'lmstudio',
      quick_model: 'my-model',
      quick_effective_model: 'my-model',
      quick_provider_access: 'available',
      quick_model_inventory: { available: { models: ['my-model'] } },
      quick_next_action: 'none',
      quick_can_download: false,
      quick_can_load: false,
      quick_supports_progress: false,
      quick_download_target: null,
      deep_lane: 'unknown',
      deep_provider: 'lmstudio',
      deep_model: null,
      deep_provider_access: 'unknown',
      deep_model_inventory: 'not_attempted',
      deep_next_action: 'none',
      deep_can_download: false,
      deep_can_load: false,
      deep_supports_progress: false,
      download: null,
    };

    const html = renderReadinessSection(readiness);

    expect(html).toContain('LM Studio');
  });

  it('renders inventory lookup failed state', () => {
    const readiness = {
      quick_lane: 'unknown',
      quick_provider: 'ollama',
      quick_model: null,
      quick_effective_model: null,
      quick_provider_access: 'unreachable',
      quick_model_inventory: { lookup_failed: { reason: 'connection refused' } },
      quick_next_action: 'refresh',
      quick_can_download: false,
      quick_can_load: false,
      quick_supports_progress: false,
      quick_download_target: null,
      deep_lane: 'unknown',
      deep_provider: 'ollama',
      deep_model: null,
      deep_provider_access: 'unknown',
      deep_model_inventory: 'not_attempted',
      deep_next_action: 'none',
      deep_can_download: false,
      deep_can_load: false,
      deep_supports_progress: false,
      download: null,
    };

    const html = renderReadinessSection(readiness);

    expect(html).toContain('status.readiness.inventory.error');
  });
});
