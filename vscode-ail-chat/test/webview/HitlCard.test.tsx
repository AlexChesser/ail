// @vitest-environment jsdom

import { describe, it, expect, vi, afterEach } from 'vitest';
import React from 'react';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { HitlCard } from '../../src/webview/components/HitlCard';

afterEach(cleanup);

describe('HitlCard', () => {
  it('renders pending card with Approve and Reject buttons', () => {
    render(
      <HitlCard
        stepId="review"
        message="Please confirm"
        cardState="pending"
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    expect(screen.getByText('Approve')).toBeTruthy();
    expect(screen.getByText('Reject')).toBeTruthy();
  });

  it('renders the gate message', () => {
    render(
      <HitlCard
        stepId="review"
        message="Deploy to production?"
        cardState="pending"
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    expect(screen.getByText('Deploy to production?')).toBeTruthy();
  });

  it('calls onApprove with stepId when Approve is clicked', () => {
    const onApprove = vi.fn();
    render(
      <HitlCard
        stepId="review"
        cardState="pending"
        onApprove={onApprove}
        onReject={vi.fn()}
      />
    );
    fireEvent.click(screen.getByText('Approve'));
    expect(onApprove).toHaveBeenCalledWith('review');
  });

  it('calls onReject with stepId when Reject is clicked', () => {
    const onReject = vi.fn();
    render(
      <HitlCard
        stepId="gate-1"
        cardState="pending"
        onApprove={vi.fn()}
        onReject={onReject}
      />
    );
    fireEvent.click(screen.getByText('Reject'));
    expect(onReject).toHaveBeenCalledWith('gate-1');
  });

  it('shows Modify button that reveals textarea when clicked', () => {
    render(
      <HitlCard
        stepId="review"
        cardState="pending"
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    fireEvent.click(screen.getByText('Modify'));
    expect(screen.getByRole('textbox')).toBeTruthy();
    expect(screen.getByText('Submit')).toBeTruthy();
    expect(screen.getByText('Cancel')).toBeTruthy();
  });

  it('Submit is disabled when modify textarea is empty', () => {
    render(
      <HitlCard
        stepId="review"
        cardState="pending"
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    fireEvent.click(screen.getByText('Modify'));
    expect((screen.getByText('Submit') as HTMLButtonElement).disabled).toBe(true);
  });

  it('Cancel restores normal button view', () => {
    render(
      <HitlCard
        stepId="review"
        cardState="pending"
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    fireEvent.click(screen.getByText('Modify'));
    fireEvent.click(screen.getByText('Cancel'));
    expect(screen.getByText('Approve')).toBeTruthy();
  });

  it('shows resolved state with default "Approved" text', () => {
    render(
      <HitlCard
        stepId="review"
        cardState="resolved"
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    expect(screen.getByText(/Approved/)).toBeTruthy();
    expect(screen.queryByText('Approve')).toBeNull();
  });

  it('shows resolved state with custom resolvedText', () => {
    render(
      <HitlCard
        stepId="review"
        cardState="resolved"
        resolvedText="Modified: deploy to staging"
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    expect(screen.getByText(/Modified: deploy to staging/)).toBeTruthy();
  });

  it('shows cancelled state text', () => {
    render(
      <HitlCard
        stepId="review"
        cardState="cancelled"
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    expect(screen.getByText(/Pipeline ended before response/)).toBeTruthy();
  });
});
