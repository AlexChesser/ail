// @vitest-environment jsdom

import { describe, it, expect, vi, afterEach } from 'vitest';
import React from 'react';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { AskUserQuestionCard, AskUserQuestion } from '../../src/webview/components/AskUserQuestionCard';

afterEach(cleanup);

const baseQuestion: AskUserQuestion = {
  header: 'Choose an option',
  question: 'What should we work on?',
  multiSelect: false,
  options: [
    { label: 'Add tests', description: 'Create new test files' },
    { label: 'Fix a bug', description: 'Correct a bug' },
    { label: 'Update docs' },
  ],
};

describe('AskUserQuestionCard — pending state', () => {
  it('renders the question header', () => {
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />);
    expect(screen.getByText('Choose an option')).toBeTruthy();
  });

  it('renders the question text', () => {
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />);
    expect(screen.getByText('What should we work on?')).toBeTruthy();
  });

  it('renders all option labels', () => {
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />);
    expect(screen.getByText('Add tests')).toBeTruthy();
    expect(screen.getByText('Fix a bug')).toBeTruthy();
    expect(screen.getByText('Update docs')).toBeTruthy();
  });

  it('renders option descriptions', () => {
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />);
    expect(screen.getByText('Create new test files')).toBeTruthy();
  });

  it('shows Submit and Dismiss buttons', () => {
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />);
    expect(screen.getByText('Submit')).toBeTruthy();
    expect(screen.getByText('Dismiss')).toBeTruthy();
  });

  it('Submit is disabled before any selection', () => {
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />);
    expect((screen.getByText('Submit') as HTMLButtonElement).disabled).toBe(true);
  });

  it('Submit becomes enabled after selecting an option', () => {
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />);
    fireEvent.click(screen.getByText('Add tests'));
    expect((screen.getByText('Submit') as HTMLButtonElement).disabled).toBe(false);
  });

  it('calls onSubmit with selected label when Submit clicked', () => {
    const onSubmit = vi.fn();
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={onSubmit} onDeny={vi.fn()} />);
    fireEvent.click(screen.getByText('Fix a bug'));
    fireEvent.click(screen.getByText('Submit'));
    expect(onSubmit).toHaveBeenCalledWith('Fix a bug');
  });

  it('calls onDeny when Dismiss clicked', () => {
    const onDeny = vi.fn();
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={vi.fn()} onDeny={onDeny} />);
    fireEvent.click(screen.getByText('Dismiss'));
    expect(onDeny).toHaveBeenCalled();
  });

  it('shows Something else... option', () => {
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />);
    expect(screen.getByText('Something else...')).toBeTruthy();
  });

  it('shows free-text textarea when Something else is selected', () => {
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />);
    fireEvent.click(screen.getByText('Something else...'));
    expect(screen.getByPlaceholderText('Type your answer...')).toBeTruthy();
  });

  it('Submit stays disabled on empty free-text', () => {
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />);
    fireEvent.click(screen.getByText('Something else...'));
    expect((screen.getByText('Submit') as HTMLButtonElement).disabled).toBe(true);
  });

  it('calls onSubmit with free-text value', () => {
    const onSubmit = vi.fn();
    render(<AskUserQuestionCard questions={[baseQuestion]} cardState="pending" onSubmit={onSubmit} onDeny={vi.fn()} />);
    fireEvent.click(screen.getByText('Something else...'));
    fireEvent.change(screen.getByPlaceholderText('Type your answer...'), { target: { value: 'Something custom' } });
    fireEvent.click(screen.getByText('Submit'));
    expect(onSubmit).toHaveBeenCalledWith('Something custom');
  });
});

describe('AskUserQuestionCard — resolved state', () => {
  it('shows resolved answer with checkmark', () => {
    render(
      <AskUserQuestionCard
        questions={[baseQuestion]}
        cardState="resolved"
        resolvedAnswer="Add tests"
        onSubmit={vi.fn()}
        onDeny={vi.fn()}
      />
    );
    expect(screen.getByText(/Add tests/)).toBeTruthy();
    expect(screen.queryByText('Submit')).toBeNull();
  });

  it('shows Dismissed when resolvedAnswer is absent', () => {
    render(
      <AskUserQuestionCard
        questions={[baseQuestion]}
        cardState="resolved"
        onSubmit={vi.fn()}
        onDeny={vi.fn()}
      />
    );
    expect(screen.getByText(/Dismissed/)).toBeTruthy();
  });
});

describe('AskUserQuestionCard — resilience', () => {
  it('renders nothing when questions array is empty', () => {
    const { container } = render(
      <AskUserQuestionCard questions={[]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />
    );
    expect(container.firstChild).toBeNull();
  });

  it('does not crash when options is undefined', () => {
    const q = { header: 'h', question: 'q?', multiSelect: false } as unknown as AskUserQuestion;
    expect(() =>
      render(<AskUserQuestionCard questions={[q]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />)
    ).not.toThrow();
  });

  it('does not crash when options is null', () => {
    const q = { header: 'h', question: 'q?', multiSelect: false, options: null } as unknown as AskUserQuestion;
    expect(() =>
      render(<AskUserQuestionCard questions={[q]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />)
    ).not.toThrow();
  });

  it('handles string option items gracefully', () => {
    const q = { header: 'h', question: 'q?', multiSelect: false, options: ['Blue', 'Green'] } as unknown as AskUserQuestion;
    // should not crash; label is rendered as-is from normalizer
    expect(() =>
      render(<AskUserQuestionCard questions={[q]} cardState="pending" onSubmit={vi.fn()} onDeny={vi.fn()} />)
    ).not.toThrow();
  });
});
