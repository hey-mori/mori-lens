// Operation controller behavioral tests
// Proves exact polling cardinality, IPC command usage, and lifecycle ownership
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createControlCenterFixture, cleanupFixture } from './test-infrastructure.js';

// Use vi.hoisted to create mocks before vi.mock is hoisted
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  state: { cachedProviderReadiness: null },
}));

// Mock the Tauri IPC boundary
vi.mock('./tauri.js', () => ({
  invoke: mocks.invoke,
}));

// Mock state module
vi.mock('./state.js', () => ({
  state: mocks.state,
}));

// Import production module after mocking
import {
  startOperationPolling,
  observeExisting,
  stopPolling,
  dispose,
  isCurrentlyPolling,
  getLastSnapshot,
} from './operation-controller.js';

describe('Operation controller', () => {
  let fixture;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    fixture = createControlCenterFixture();

    // Default: no active operation
    mocks.invoke.mockResolvedValue(null);
  });

  afterEach(() => {
    dispose();
    cleanupFixture(fixture);
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  describe('startOperationPolling', () => {
    it('starts polling immediately when called', () => {
      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      expect(isCurrentlyPolling()).toBe(true);
    });

    it('calls get_model_download_status on first poll', async () => {
      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      // Advance timer to trigger first poll
      await vi.advanceTimersByTimeAsync(2000);

      expect(mocks.invoke).toHaveBeenCalledWith('get_model_download_status');
    });

    it('polls every 2 seconds for active operations', async () => {
      mocks.invoke.mockResolvedValue({
        active: true,
        status: { progress: 50 },
      });

      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      // First poll
      await vi.advanceTimersByTimeAsync(2000);
      expect(mocks.invoke).toHaveBeenCalledTimes(1);

      // Second poll
      await vi.advanceTimersByTimeAsync(2000);
      expect(mocks.invoke).toHaveBeenCalledTimes(2);

      // Third poll
      await vi.advanceTimersByTimeAsync(2000);
      expect(mocks.invoke).toHaveBeenCalledTimes(3);
    });

    it('stops polling when operation reaches terminal state', async () => {
      let callCount = 0;
      mocks.invoke.mockImplementation(async () => {
        callCount++;
        if (callCount === 1) {
          return { active: true, status: { progress: 50 } };
        }
        // Second call: terminal state
        return { active: false, status: null };
      });

      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      // First poll: active
      await vi.advanceTimersByTimeAsync(2000);
      expect(isCurrentlyPolling()).toBe(true);
      expect(updateCallback).toHaveBeenCalledWith(
        { active: true, status: { progress: 50 } }
      );

      // Second poll: terminal
      await vi.advanceTimersByTimeAsync(2000);
      expect(isCurrentlyPolling()).toBe(false);
      expect(updateCallback).toHaveBeenCalledWith(null, true); // null snapshot, terminal flag
    });

    it('does not call get_provider_readiness during polling', async () => {
      mocks.invoke.mockResolvedValue({
        active: true,
        status: { progress: 50 },
      });

      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      // Poll multiple times
      await vi.advanceTimersByTimeAsync(6000);

      // Verify only get_model_download_status was called
      const commands = mocks.invoke.mock.calls.map(([cmd]) => cmd);
      expect(commands).toEqual([
        'get_model_download_status',
        'get_model_download_status',
        'get_model_download_status',
      ]);
      expect(commands).not.toContain('get_provider_readiness');
    });
  });

  describe('observeExisting', () => {
    it('does not start polling when no cached operation', () => {
      const updateCallback = vi.fn();
      observeExisting(updateCallback);

      expect(isCurrentlyPolling()).toBe(false);
    });

    it('starts polling when cached operation is active', () => {
      // Mock cached readiness with active download
      mocks.state.cachedProviderReadiness = {
        download: {
          active: true,
          status: { progress: 30 },
        },
      };

      const updateCallback = vi.fn();
      observeExisting(updateCallback);

      expect(isCurrentlyPolling()).toBe(true);
    });

    it('does not start polling when cached operation is not active', () => {
      mocks.state.cachedProviderReadiness = {
        download: {
          active: false,
          status: null,
        },
      };

      const updateCallback = vi.fn();
      observeExisting(updateCallback);

      expect(isCurrentlyPolling()).toBe(false);
    });
  });

  describe('stopPolling', () => {
    it('stops polling immediately', () => {
      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);
      expect(isCurrentlyPolling()).toBe(true);

      stopPolling();
      expect(isCurrentlyPolling()).toBe(false);
    });

    it('clears pending timer', async () => {
      mocks.invoke.mockResolvedValue({
        active: true,
        status: { progress: 50 },
      });

      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      // Stop before timer fires
      stopPolling();

      // Advance timer
      await vi.advanceTimersByTimeAsync(2000);

      // Should not have been called
      expect(mocks.invoke).not.toHaveBeenCalled();
    });
  });

  describe('dispose', () => {
    it('stops polling and clears callback', () => {
      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      dispose();

      expect(isCurrentlyPolling()).toBe(false);
      expect(getLastSnapshot()).toBeNull();
    });
  });

  describe('paused state', () => {
    it('polls at reduced frequency when paused', async () => {
      mocks.invoke.mockResolvedValue({
        active: true,
        status: { paused: true },
      });

      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      // First poll at 2s (active interval)
      await vi.advanceTimersByTimeAsync(2000);
      expect(mocks.invoke).toHaveBeenCalledTimes(1);

      // Second poll at 15s (paused interval)
      await vi.advanceTimersByTimeAsync(15000);
      expect(mocks.invoke).toHaveBeenCalledTimes(2);

      // Third poll at 15s (paused interval)
      await vi.advanceTimersByTimeAsync(15000);
      expect(mocks.invoke).toHaveBeenCalledTimes(3);
    });
  });

  describe('error handling', () => {
    it('stops polling on IPC error', async () => {
      mocks.invoke.mockRejectedValue(new Error('IPC failed'));

      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      // Trigger poll
      await vi.advanceTimersByTimeAsync(2000);

      expect(isCurrentlyPolling()).toBe(false);
    });

    it('handles null response by stopping polling', async () => {
      mocks.invoke.mockResolvedValue(null);

      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      // Trigger poll
      await vi.advanceTimersByTimeAsync(2000);

      expect(isCurrentlyPolling()).toBe(false);
    });
  });

  describe('update callback', () => {
    it('calls update callback with snapshot on each poll', async () => {
      const snapshot = {
        active: true,
        status: { progress: 50, bytes_downloaded: 1024 },
      };
      mocks.invoke.mockResolvedValue(snapshot);

      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      await vi.advanceTimersByTimeAsync(2000);

      expect(updateCallback).toHaveBeenCalledWith(snapshot);
    });

    it('calls update callback with terminal flag on completion', async () => {
      let callCount = 0;
      mocks.invoke.mockImplementation(async () => {
        callCount++;
        if (callCount === 1) {
          return { active: true, status: { progress: 50 } };
        }
        return { active: false, status: null };
      });

      const updateCallback = vi.fn();
      startOperationPolling(updateCallback);

      // First poll: active
      await vi.advanceTimersByTimeAsync(2000);
      expect(updateCallback).toHaveBeenCalledWith(
        { active: true, status: { progress: 50 } }
      );

      // Second poll: terminal
      await vi.advanceTimersByTimeAsync(2000);
      expect(updateCallback).toHaveBeenCalledWith(null, true);
    });
  });
});
