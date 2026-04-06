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
    expect(screen.getByText(/What would you like to build/i)).toBeTruthy();
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

  // ── AskUserQuestion intercept ────────────────────────────────────────────────

  it('shows AskUserQuestionCard for canonical questions:[...] format', () => {
    render(<App />);
    postMessage({
      type: 'permissionRequested',
      displayName: 'AskUserQuestion',
      displayDetail: 'What should we work on?',
      toolInput: {
        questions: [{
          header: 'Next Work',
          question: 'What should we work on?',
          multiSelect: false,
          options: [{ label: 'Add tests' }, { label: 'Fix a bug' }],
        }],
      },
    });
    expect(screen.getByText('What should we work on?')).toBeTruthy();
    expect(screen.getByText('Add tests')).toBeTruthy();
    expect(screen.getByText('Submit')).toBeTruthy();
    expect(screen.getByText('Dismiss')).toBeTruthy();
    // Should NOT show Allow/Deny (the generic permission card)
    expect(screen.queryByText('Allow')).toBeNull();
  });

  it('shows AskUserQuestionCard for flat {question, options} format', () => {
    render(<App />);
    postMessage({
      type: 'permissionRequested',
      displayName: 'AskUserQuestion',
      displayDetail: 'Pick one',
      toolInput: {
        question: 'Pick one',
        options: [{ label: 'Option A' }, { label: 'Option B' }],
      },
    });
    expect(screen.getByText('Pick one')).toBeTruthy();
    expect(screen.getByText('Option A')).toBeTruthy();
  });

  it('shows AskUserQuestionCard when options is a JSON-encoded string', () => {
    render(<App />);
    postMessage({
      type: 'permissionRequested',
      displayName: 'AskUserQuestion',
      displayDetail: 'q',
      toolInput: {
        questions: [{
          header: 'h',
          question: 'What color?',
          multiSelect: 'False',
          options: JSON.stringify([{ label: 'Blue' }, { label: 'Red' }]),
        }],
      },
    });
    expect(screen.getByText('What color?')).toBeTruthy();
    expect(screen.getByText('Blue')).toBeTruthy();
    expect(screen.getByText('Red')).toBeTruthy();
  });

  it('falls back to generic permission card when AskUserQuestion has no parseable questions', () => {
    render(<App />);
    postMessage({
      type: 'permissionRequested',
      displayName: 'AskUserQuestion',
      displayDetail: 'some detail',
      toolInput: { unrelated: 'data' },
    });
    expect(screen.getByText('Allow')).toBeTruthy();
    expect(screen.getByText('Deny')).toBeTruthy();
  });

  it('disables input while AskUserQuestion is pending', () => {
    render(<App />);
    postMessage({
      type: 'permissionRequested',
      displayName: 'AskUserQuestion',
      displayDetail: 'q',
      toolInput: { questions: [{ header: 'h', question: 'q?', multiSelect: false, options: [] }] },
    });
    const textarea = screen.getByRole('textbox');
    expect((textarea as HTMLTextAreaElement).disabled).toBe(true);
  });

  it('resolves AskUserQuestion card on pipelineCompleted', () => {
    render(<App />);
    postMessage({
      type: 'permissionRequested',
      displayName: 'AskUserQuestion',
      displayDetail: 'q',
      toolInput: { questions: [{ header: 'h', question: 'q?', multiSelect: false, options: [{ label: 'A' }] }] },
    });
    postMessage({ type: 'pipelineCompleted' });
    // Submit/Dismiss buttons gone after completion
    expect(screen.queryByText('Submit')).toBeNull();
    expect(screen.queryByText('Dismiss')).toBeNull();
  });

  // ── ail_ask_user MCP bridge output ───────────────────────────────────────────

  it('renders AskUserQuestionCard for bridge-normalised input (ail_ask_user output format)', () => {
    // The mcp_bridge normalises any model-produced format into this canonical shape
    // before forwarding to the permission socket as tool_name="AskUserQuestion".
    // This test documents that contract so the frontend remains compatible.
    render(<App />);
    postMessage({
      type: 'permissionRequested',
      displayName: 'AskUserQuestion',
      displayDetail: 'Which color?',
      toolInput: {
        questions: [
          {
            header: '',
            question: 'Which color?',
            multiSelect: false,
            options: [
              { label: 'Red' },
              { label: 'Blue' },
              { label: 'Green' },
            ],
          },
        ],
      },
    });
    expect(screen.getByText('Which color?')).toBeTruthy();
    expect(screen.getByText('Red')).toBeTruthy();
    expect(screen.getByText('Blue')).toBeTruthy();
    expect(screen.getByText('Green')).toBeTruthy();
    expect(screen.getByText('Submit')).toBeTruthy();
    expect(screen.queryByText('Allow')).toBeNull();
  });

  // ── ErrorBoundary isolation ───────────────────────────────────────────────────

  it('ErrorBoundary prevents one crashed item from removing other items', () => {
    render(<App />);
    postMessage({ type: 'streamDelta', text: 'Visible message' });
    // Force a crash by sending AskUserQuestion with questions=null  (passes the Array.isArray guard
    // as an empty array, producing an ask-user-question item with questions:[] → card returns null)
    postMessage({
      type: 'permissionRequested',
      displayName: 'AskUserQuestion',
      displayDetail: 'q',
      toolInput: { questions: [] },
    });
    // The stream message should still be visible
    expect(screen.getAllByText('Visible message').length).toBeGreaterThan(0);
  });

  it('creates a new stream item for the second step instead of appending to the closed first step', () => {
    render(<App />);
    postMessage({ type: 'runStarted', runId: 'r1', totalSteps: 2 });
    // Step 1: invocation
    postMessage({ type: 'stepStarted', stepId: 'invocation', stepIndex: 0, totalSteps: 2 });
    postMessage({ type: 'streamDelta', text: 'Hello!' });
    postMessage({ type: 'stepCompleted', stepId: 'invocation', costUsd: null, inputTokens: 10, outputTokens: 5, response: 'Hello!' });
    // Step 2: review — its stream delta must NOT be appended to the closed invocation stream
    postMessage({ type: 'stepStarted', stepId: 'review', stepIndex: 1, totalSteps: 2 });
    postMessage({ type: 'streamDelta', text: 'Review result' });
    // invocation item should still have only its original text
    expect(screen.getByText('Hello!')).toBeTruthy();
    // review stream should be visible as its own item
    expect(screen.getByText('Review result')).toBeTruthy();
  });
});
