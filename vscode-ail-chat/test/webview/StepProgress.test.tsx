// @vitest-environment jsdom

import { describe, it, expect, afterEach } from 'vitest';
import React from 'react';
import { render, screen, cleanup } from '@testing-library/react';
import { StepProgress, StepInfo } from '../../src/webview/components/StepProgress';

afterEach(cleanup);

describe('StepProgress', () => {
  it('renders nothing when steps array is empty', () => {
    const { container } = render(<StepProgress steps={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders a badge for each step', () => {
    const steps: StepInfo[] = [
      { stepId: 'invocation', status: 'completed' },
      { stepId: 'review', status: 'running' },
      { stepId: 'check', status: 'pending' },
    ];
    render(<StepProgress steps={steps} />);
    expect(screen.getByText('invocation')).toBeTruthy();
    expect(screen.getByText('review')).toBeTruthy();
    expect(screen.getByText('check')).toBeTruthy();
  });

  it('applies status class to each glyph', () => {
    const steps: StepInfo[] = [
      { stepId: 'a', status: 'completed' },
      { stepId: 'b', status: 'failed' },
      { stepId: 'c', status: 'skipped' },
    ];
    const { container } = render(<StepProgress steps={steps} />);
    expect(container.querySelector('.step-glyph.completed')).toBeTruthy();
    expect(container.querySelector('.step-glyph.failed')).toBeTruthy();
    expect(container.querySelector('.step-glyph.skipped')).toBeTruthy();
  });

  it('shows cost when costUsd > 0', () => {
    const steps: StepInfo[] = [{ stepId: 'review', status: 'completed', costUsd: 0.0025 }];
    render(<StepProgress steps={steps} />);
    expect(screen.getByText('$0.0025')).toBeTruthy();
  });

  it('does not show cost element when costUsd is 0', () => {
    const steps: StepInfo[] = [{ stepId: 'review', status: 'completed', costUsd: 0 }];
    const { container } = render(<StepProgress steps={steps} />);
    expect(container.querySelector('.step-cost')).toBeNull();
  });

  it('shows total cost summary when totalCostUsd > 0', () => {
    const steps: StepInfo[] = [{ stepId: 'a', status: 'completed' }];
    render(<StepProgress steps={steps} totalCostUsd={0.015} />);
    expect(screen.getByText('Total: $0.0150')).toBeTruthy();
  });

  it('does not show total cost when totalCostUsd is 0', () => {
    const steps: StepInfo[] = [{ stepId: 'a', status: 'completed' }];
    const { container } = render(<StepProgress steps={steps} totalCostUsd={0} />);
    expect(container.querySelector('.run-summary')).toBeNull();
  });

  it('uses filled circle glyph for running/completed/failed', () => {
    const steps: StepInfo[] = [
      { stepId: 'a', status: 'running' },
      { stepId: 'b', status: 'completed' },
      { stepId: 'c', status: 'failed' },
    ];
    const { container } = render(<StepProgress steps={steps} />);
    const glyphs = container.querySelectorAll('.step-glyph');
    for (const g of glyphs) {
      expect(g.textContent).toBe('\u25CF');
    }
  });

  it('uses open circle glyph for pending/skipped', () => {
    const steps: StepInfo[] = [
      { stepId: 'a', status: 'pending' },
      { stepId: 'b', status: 'skipped' },
    ];
    const { container } = render(<StepProgress steps={steps} />);
    const glyphs = container.querySelectorAll('.step-glyph');
    for (const g of glyphs) {
      expect(g.textContent).toBe('\u25CB');
    }
  });
});
