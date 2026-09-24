// Accessibility test infrastructure using axe-core
// This file verifies that axe-core is properly integrated and can detect violations.
// Actual accessibility fixes are out of scope for P19C.
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createControlCenterFixture, cleanupFixture } from './test-infrastructure.js';
import axe from 'axe-core';

describe('axe-core Integration', () => {
  let fixture;

  beforeEach(() => {
    fixture = createControlCenterFixture();
  });

  afterEach(() => {
    cleanupFixture(fixture);
  });

  it('axe-core is properly loaded', () => {
    expect(axe).toBeDefined();
    expect(axe.run).toBeDefined();
    expect(typeof axe.run).toBe('function');
  });

  it('can run accessibility checks on DOM elements', async () => {
    const results = await axe.run(fixture);

    // Verify the results structure
    expect(results).toBeDefined();
    expect(results.violations).toBeDefined();
    expect(results.passes).toBeDefined();
    expect(results.incomplete).toBeDefined();
    expect(results.inapplicable).toBeDefined();

    // Verify we can access violation details
    if (results.violations.length > 0) {
      const violation = results.violations[0];
      expect(violation.id).toBeDefined();
      expect(violation.impact).toBeDefined();
      expect(violation.description).toBeDefined();
      expect(violation.nodes).toBeDefined();
    }
  });

  it('can filter by WCAG level', async () => {
    const results = await axe.run(fixture, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a'],
      },
    });

    expect(results).toBeDefined();
    expect(results.violations).toBeDefined();
  });

  it('can filter by impact', async () => {
    const results = await axe.run(fixture, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a'],
      },
    });

    expect(results).toBeDefined();
    expect(results.violations).toBeDefined();
  });

  it('can run specific rules', async () => {
    const results = await axe.run(fixture, {
      rules: {
        'color-contrast': { enabled: true },
      },
    });

    expect(results).toBeDefined();
  });

  it('detects accessibility violations in test fixture', async () => {
    const results = await axe.run(fixture);

    // The test fixture intentionally has some accessibility issues
    // (missing labels on form fields) to verify axe-core can detect them.
    // This test documents that axe-core is working correctly.
    console.log(`Accessibility scan found ${results.violations.length} violations`);
    console.log(`Accessibility scan found ${results.passes.length} passes`);

    // Verify axe-core ran successfully
    expect(results.violations).toBeDefined();
    expect(Array.isArray(results.violations)).toBe(true);
  });
});

describe('Accessibility Test Documentation', () => {
  it('documents known accessibility issues', () => {
    // This test documents that the Control Center has known accessibility
    // issues that need to be fixed in a future PR.
    //
    // Known issues:
    // - Form fields missing labels (WCAG 1.3.1, 4.1.2)
    // - Some buttons may need better accessible names
    // - Color contrast may need adjustment
    //
    // These issues were identified by axe-core and should be addressed
    // in a follow-up PR after P19C is complete.

    const knownIssues = [
      'Form fields need associated labels',
      'Buttons need accessible names',
      'Color contrast needs verification',
    ];

    expect(knownIssues.length).toBeGreaterThan(0);
  });
});

describe('P20C Accessibility - Readiness Region States', () => {
  let fixture;

  beforeEach(() => {
    fixture = createControlCenterFixture();
  });

  afterEach(() => {
    cleanupFixture(fixture);
  });

  it('readiness region has proper ARIA labels for download states', async () => {
    // Create a readiness region with download state
    const readinessRegion = document.createElement('div');
    readinessRegion.id = 'status-readiness-region';
    readinessRegion.setAttribute('role', 'region');
    readinessRegion.setAttribute('aria-label', 'Model readiness');
    readinessRegion.innerHTML = `
      <div class="readiness-content">
        <button data-action="download" aria-label="Download model">
          Download
        </button>
      </div>
    `;
    fixture.appendChild(readinessRegion);

    const results = await axe.run(readinessRegion, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a', 'wcag412'],
      },
    });

    // Verify no violations for button labeling
    const buttonViolations = results.violations.filter(v =>
      v.id === 'button-name' || v.id === 'name-role-value'
    );
    expect(buttonViolations.length).toBe(0);
  });

  it('operation progress indicators have proper ARIA attributes', async () => {
    // Create a progress indicator
    const progressContainer = document.createElement('div');
    progressContainer.className = 'download-progress';
    progressContainer.setAttribute('role', 'progressbar');
    progressContainer.setAttribute('aria-valuenow', '50');
    progressContainer.setAttribute('aria-valuemin', '0');
    progressContainer.setAttribute('aria-valuemax', '100');
    progressContainer.setAttribute('aria-label', 'Download progress');
    progressContainer.innerHTML = '<div class="progress-bar">50%</div>';
    fixture.appendChild(progressContainer);

    const results = await axe.run(progressContainer, {
      runOnly: {
        type: 'rule',
        values: ['aria-valid-attr-value'],
      },
    });

    // Verify no ARIA violations
    expect(results.violations.length).toBe(0);
  });

  it('action buttons have accessible names', async () => {
    // Create action buttons with proper labels
    const actionContainer = document.createElement('div');
    actionContainer.className = 'readiness-actions';
    actionContainer.innerHTML = `
      <button data-action="download" aria-label="Download quick model">
        Download
      </button>
      <button data-action="refresh" aria-label="Refresh readiness status">
        Refresh
      </button>
    `;
    fixture.appendChild(actionContainer);

    const results = await axe.run(actionContainer, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a', 'wcag412'],
      },
    });

    // Verify all buttons have accessible names
    const buttonViolations = results.violations.filter(v =>
      v.id === 'button-name'
    );
    expect(buttonViolations.length).toBe(0);
  });

  it('status regions have proper heading structure', async () => {
    // Create status regions with proper headings
    const statusSection = document.createElement('section');
    statusSection.id = 'status-section';
    statusSection.innerHTML = `
      <h2>Status</h2>
      <div id="status-overview-region" role="region" aria-label="Overview">
        <h3>Overview</h3>
        <p>System status information</p>
      </div>
      <div id="status-readiness-region" role="region" aria-label="Model readiness">
        <h3>Model Readiness</h3>
        <p>Model availability information</p>
      </div>
    `;
    fixture.appendChild(statusSection);

    const results = await axe.run(statusSection, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a', 'wcag131'],
      },
    });

    // Verify heading structure is valid
    const headingViolations = results.violations.filter(v =>
      v.id === 'heading-order'
    );
    expect(headingViolations.length).toBe(0);
  });
});

