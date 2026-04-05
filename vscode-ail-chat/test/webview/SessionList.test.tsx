// @vitest-environment jsdom

import { describe, it, expect, vi, afterEach } from 'vitest';
import React from 'react';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { SessionList } from '../../src/webview/components/SessionList';
import { SessionSummary } from '../../src/types';

afterEach(cleanup);

const sessions: SessionSummary[] = [
  { id: 's1', title: 'Refactor auth module', timestamp: 1700000000000, totalCostUsd: 0.005 },
  { id: 's2', title: 'Fix the login bug', timestamp: 1700000001000, totalCostUsd: 0.003 },
];

describe('SessionList', () => {
  it('renders all session titles', () => {
    render(
      <SessionList
        sessions={sessions}
        activeSessionId={null}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
      />
    );
    expect(screen.getByText('Refactor auth module')).toBeTruthy();
    expect(screen.getByText('Fix the login bug')).toBeTruthy();
  });

  it('shows "No sessions yet" when list is empty', () => {
    render(
      <SessionList
        sessions={[]}
        activeSessionId={null}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
      />
    );
    expect(screen.getByText('No sessions yet')).toBeTruthy();
  });

  it('marks the active session with "active" class', () => {
    const { container } = render(
      <SessionList
        sessions={sessions}
        activeSessionId="s1"
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
      />
    );
    const items = container.querySelectorAll('.session-item');
    expect(items[0].classList.contains('active')).toBe(true);
    expect(items[1].classList.contains('active')).toBe(false);
  });

  it('calls onSelectSession with the clicked session id', () => {
    const onSelectSession = vi.fn();
    render(
      <SessionList
        sessions={sessions}
        activeSessionId={null}
        onSelectSession={onSelectSession}
        onNewSession={vi.fn()}
      />
    );
    fireEvent.click(screen.getByText('Fix the login bug'));
    expect(onSelectSession).toHaveBeenCalledWith('s2');
  });

  it('calls onSelectSession on Enter keydown', () => {
    const onSelectSession = vi.fn();
    const { container } = render(
      <SessionList
        sessions={sessions}
        activeSessionId={null}
        onSelectSession={onSelectSession}
        onNewSession={vi.fn()}
      />
    );
    const items = container.querySelectorAll('.session-item');
    fireEvent.keyDown(items[0], { key: 'Enter' });
    expect(onSelectSession).toHaveBeenCalledWith('s1');
  });

  it('calls onNewSession when "+" button is clicked', () => {
    const onNewSession = vi.fn();
    render(
      <SessionList
        sessions={sessions}
        activeSessionId={null}
        onSelectSession={vi.fn()}
        onNewSession={onNewSession}
      />
    );
    fireEvent.click(screen.getByTitle('New session'));
    expect(onNewSession).toHaveBeenCalledOnce();
  });

  it('renders "(untitled)" for sessions with empty title', () => {
    const untitled: SessionSummary[] = [
      { id: 'x1', title: '', timestamp: 1700000000000, totalCostUsd: 0 },
    ];
    render(
      <SessionList
        sessions={untitled}
        activeSessionId={null}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
      />
    );
    expect(screen.getByText('(untitled)')).toBeTruthy();
  });
});
