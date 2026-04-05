/**
 * Tests for the App reducer logic.
 *
 * We test the reducer directly without rendering, since all display logic
 * flows through the reducer via useReducer.
 */

// @vitest-environment jsdom

import { describe, it, expect, afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';
import { HostToWebviewMessage } from '../../src/types';

// Import the reducer and types from App without JSX
// We re-implement a minimal version of the reducer types here to avoid
// pulling in React rendering in this test — the reducer is a pure function.

// Instead: render a minimal test that exercises the key state transitions
// by importing and calling the reducer directly.

// Since App.tsx is a React module we can still import it; we only call the
// reducer, not render the component.

// We extract the reducer by re-exporting it in a testable way.
// For now, we test the DisplayItem accumulation via rendering.

import React from 'react';
import { render, screen, act } from '@testing-library/react';
import { App } from '../../src/webview/App';

// Stub acquireVsCodeApi globally for jsdom
(globalThis as unknown as Record<string, unknown>)['acquireVsCodeApi'] = () => ({
  postMessage: () => {},
  getState: () => null,
  setState: () => {},
});

function postMessage(msg: HostToWebviewMessage) {
  act(() => {
    window.dispatchEvent(new MessageEvent('message', { data: msg }));
  });
}

describe('App webview', () => {
  afterEach(cleanup);

  it('renders empty state banner initially', () => {
    render(<App />);
    expect(screen.getByText(/Send a prompt to get started/i)).toBeTruthy();
  });

  it('shows streamed text after streamDelta messages', () => {
    render(<App />);
    postMessage({ type: 'runStarted', runId: 'r1', totalSteps: 1 });
    postMessage({ type: 'stepStarted', stepId: 'invocation', stepIndex: 0, totalSteps: 1 });
    postMessage({ type: 'streamDelta', text: 'Hello' });
    postMessage({ type: 'streamDelta', text: ' world' });
    expect(screen.getAllByText('Hello world')).toHaveLength(1);
  });

  it('accumulates multiple stream deltas into one message block', () => {
    render(<App />);
    postMessage({ type: 'runStarted', runId: 'r1', totalSteps: 1 });
    postMessage({ type: 'streamDelta', text: 'Part 1 ' });
    postMessage({ type: 'streamDelta', text: 'Part 2' });
    // Should be one element with combined text
    expect(screen.getAllByText('Part 1 Part 2')).toHaveLength(1);
  });

  it('shows error message on pipelineError', () => {
    render(<App />);
    postMessage({ type: 'pipelineError', error: 'Pipeline not found' });
    expect(screen.getByText('Pipeline not found')).toBeTruthy();
  });

  it('shows error message on processError', () => {
    render(<App />);
    postMessage({ type: 'processError', message: 'spawn failed' });
    expect(screen.getByText('spawn failed')).toBeTruthy();
  });

  it('shows HITL card when hitlGate received', () => {
    render(<App />);
    postMessage({ type: 'hitlGate', stepId: 'review', message: 'Review required' });
    // The card-level message text (not the title which says "human review required")
    expect(screen.getByText('Review required')).toBeTruthy();
    expect(screen.getByText('Approve')).toBeTruthy();
    expect(screen.getByText('Reject')).toBeTruthy();
  });

  it('shows permission card when permissionRequested received', () => {
    render(<App />);
    postMessage({
      type: 'permissionRequested',
      displayName: 'Delete files',
      displayDetail: 'rm /tmp/foo',
    });
    expect(screen.getByText(/Delete files/i)).toBeTruthy();
    expect(screen.getByText('Allow')).toBeTruthy();
    expect(screen.getByText('Deny')).toBeTruthy();
  });

  it('shows step progress when steps arrive', () => {
    render(<App />);
    postMessage({ type: 'runStarted', runId: 'r1', totalSteps: 2 });
    postMessage({ type: 'stepStarted', stepId: 'invocation', stepIndex: 0, totalSteps: 2 });
    postMessage({ type: 'stepStarted', stepId: 'check', stepIndex: 1, totalSteps: 2 });
    // Step IDs should appear in the progress panel
    expect(screen.getAllByText('invocation').length).toBeGreaterThan(0);
    expect(screen.getAllByText('check').length).toBeGreaterThan(0);
  });

  it('marks pipeline as done on pipelineCompleted', () => {
    render(<App />);
    postMessage({ type: 'runStarted', runId: 'r1', totalSteps: 1 });
    postMessage({ type: 'streamDelta', text: 'done' });
    postMessage({ type: 'pipelineCompleted' });
    // Input should no longer show "Running…" placeholder (isRunning = false)
    const textarea = screen.getByRole('textbox');
    expect((textarea as HTMLTextAreaElement).disabled).toBe(false);
  });

  it('disables input while HITL gate is pending', () => {
    render(<App />);
    postMessage({ type: 'hitlGate', stepId: 'review' });
    const textarea = screen.getByRole('textbox');
    expect((textarea as HTMLTextAreaElement).disabled).toBe(true);
  });

  it('shows thinking block', () => {
    render(<App />);
    postMessage({ type: 'thinking', text: 'Deep thought here' });
    expect(screen.getByText('Thinking')).toBeTruthy();
  });
});
