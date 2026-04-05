// @vitest-environment jsdom

import { describe, it, expect, afterEach } from 'vitest';
import React from 'react';
import { render, screen, cleanup } from '@testing-library/react';
import { ChatMessage } from '../../src/webview/components/ChatMessage';

afterEach(cleanup);

describe('ChatMessage', () => {
  it('renders user message with role label "You"', () => {
    render(<ChatMessage role="user" content="Hello there" />);
    expect(screen.getByText('You')).toBeTruthy();
    expect(screen.getByText('Hello there')).toBeTruthy();
  });

  it('renders assistant message with role label "ail"', () => {
    render(<ChatMessage role="assistant" content="I can help" />);
    expect(screen.getByText('ail')).toBeTruthy();
    expect(screen.getByText('I can help')).toBeTruthy();
  });

  it('applies "user" class to container for user messages', () => {
    const { container } = render(<ChatMessage role="user" content="hi" />);
    expect(container.querySelector('.chat-message.user')).toBeTruthy();
  });

  it('applies "assistant" class to container for assistant messages', () => {
    const { container } = render(<ChatMessage role="assistant" content="hi" />);
    expect(container.querySelector('.chat-message.assistant')).toBeTruthy();
  });

  it('adds "streaming" class to content when streaming=true', () => {
    const { container } = render(<ChatMessage role="assistant" content="..." streaming />);
    expect(container.querySelector('.chat-message-content.streaming')).toBeTruthy();
  });

  it('does not add "streaming" class when streaming is absent', () => {
    const { container } = render(<ChatMessage role="assistant" content="done" />);
    expect(container.querySelector('.streaming')).toBeNull();
  });
});
