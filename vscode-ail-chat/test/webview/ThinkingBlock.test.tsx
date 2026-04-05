// @vitest-environment jsdom

import { describe, it, expect, afterEach } from 'vitest';
import React from 'react';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { ThinkingBlock } from '../../src/webview/components/ThinkingBlock';

afterEach(cleanup);

describe('ThinkingBlock', () => {
  it('renders "Thinking" label', () => {
    render(<ThinkingBlock text="Let me reason about this." />);
    expect(screen.getByText('Thinking')).toBeTruthy();
  });

  it('shows a preview of the text when collapsed', () => {
    render(<ThinkingBlock text="Deep thought here" />);
    // Text appears in both preview span and collapsed content div
    expect(screen.getAllByText('Deep thought here').length).toBeGreaterThanOrEqual(1);
    expect(document.querySelector('.thinking-block-preview')?.textContent).toBe('Deep thought here');
  });

  it('truncates preview to 80 chars with ellipsis', () => {
    const long = 'A'.repeat(100);
    render(<ThinkingBlock text={long} />);
    const preview = document.querySelector('.thinking-block-preview');
    expect(preview?.textContent).toContain('…');
    expect(preview?.textContent?.length).toBeLessThanOrEqual(82); // 80 + ellipsis char
  });

  it('content area is collapsed by default', () => {
    const { container } = render(<ThinkingBlock text="think" />);
    expect(container.querySelector('.thinking-block-content.collapsed')).toBeTruthy();
  });

  it('expands content on header click', () => {
    const { container } = render(<ThinkingBlock text="think" />);
    fireEvent.click(container.querySelector('.thinking-block-header')!);
    expect(container.querySelector('.thinking-block-content:not(.collapsed)')).toBeTruthy();
  });

  it('collapses again on second header click', () => {
    const { container } = render(<ThinkingBlock text="think" />);
    fireEvent.click(container.querySelector('.thinking-block-header')!);
    fireEvent.click(container.querySelector('.thinking-block-header')!);
    expect(container.querySelector('.thinking-block-content.collapsed')).toBeTruthy();
  });

  it('hides preview when expanded (aria-expanded=true)', () => {
    const { container } = render(<ThinkingBlock text="think" />);
    const header = container.querySelector('.thinking-block-header')!;
    fireEvent.click(header);
    expect(header.getAttribute('aria-expanded')).toBe('true');
    expect(container.querySelector('.thinking-block-preview')).toBeNull();
  });

  it('expands on Enter keydown', () => {
    const { container } = render(<ThinkingBlock text="think" />);
    const header = container.querySelector('.thinking-block-header')!;
    fireEvent.keyDown(header, { key: 'Enter' });
    expect(container.querySelector('.thinking-block-content:not(.collapsed)')).toBeTruthy();
  });
});
