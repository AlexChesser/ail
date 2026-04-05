// @vitest-environment jsdom

import { describe, it, expect, afterEach } from 'vitest';
import React from 'react';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { ToolCallCard, ToolCallData } from '../../src/webview/components/ToolCallCard';

afterEach(cleanup);

const baseData: ToolCallData = {
  toolUseId: 'tu-1',
  toolName: 'Read',
  input: { file_path: '/src/main.rs' },
};

describe('ToolCallCard', () => {
  it('renders tool name in the header', () => {
    render(<ToolCallCard data={baseData} />);
    expect(screen.getByText('Read')).toBeTruthy();
  });

  it('shows primary arg from file_path input', () => {
    render(<ToolCallCard data={baseData} />);
    expect(screen.getByText('/src/main.rs')).toBeTruthy();
  });

  it('body is collapsed by default (no Input label visible)', () => {
    const { container } = render(<ToolCallCard data={baseData} />);
    expect(container.querySelector('.tool-card-body.collapsed')).toBeTruthy();
  });

  it('expands body on header click', () => {
    const { container } = render(<ToolCallCard data={baseData} />);
    fireEvent.click(container.querySelector('.tool-card-header')!);
    expect(container.querySelector('.tool-card-body:not(.collapsed)')).toBeTruthy();
    expect(screen.getByText('Input')).toBeTruthy();
  });

  it('collapses again on second header click', () => {
    const { container } = render(<ToolCallCard data={baseData} />);
    fireEvent.click(container.querySelector('.tool-card-header')!);
    fireEvent.click(container.querySelector('.tool-card-header')!);
    expect(container.querySelector('.tool-card-body.collapsed')).toBeTruthy();
  });

  it('shows result section when result is provided', () => {
    const data: ToolCallData = { ...baseData, result: 'fn main() {}' };
    const { container } = render(<ToolCallCard data={data} />);
    fireEvent.click(container.querySelector('.tool-card-header')!);
    expect(screen.getByText('Result')).toBeTruthy();
    expect(screen.getByText('fn main() {}')).toBeTruthy();
  });

  it('shows "error" status icon when isError=true', () => {
    const data: ToolCallData = { ...baseData, result: 'not found', isError: true };
    const { container } = render(<ToolCallCard data={data} />);
    expect(container.querySelector('.tool-card-status-icon.error')).toBeTruthy();
  });

  it('shows "done" status icon when result is present and no error', () => {
    const data: ToolCallData = { ...baseData, result: 'ok' };
    const { container } = render(<ToolCallCard data={data} />);
    expect(container.querySelector('.tool-card-status-icon.done')).toBeTruthy();
  });

  it('shows "pending" status icon when no result yet', () => {
    const { container } = render(<ToolCallCard data={baseData} />);
    expect(container.querySelector('.tool-card-status-icon.pending')).toBeTruthy();
  });

  it('shows line count summary for multi-line result', () => {
    const data: ToolCallData = { ...baseData, result: 'line1\nline2\nline3' };
    render(<ToolCallCard data={data} />);
    expect(screen.getByText('3 lines')).toBeTruthy();
  });

  it('truncates primary arg longer than 60 chars', () => {
    const longPath = '/'.repeat(62);
    const data: ToolCallData = { ...baseData, input: { file_path: longPath } };
    render(<ToolCallCard data={data} />);
    // Should show truncated with ellipsis
    const primaryArgEl = document.querySelector('.tool-card-primary-arg');
    expect(primaryArgEl?.textContent?.endsWith('…')).toBe(true);
  });

  it('expands/collapses on Enter keydown', () => {
    const { container } = render(<ToolCallCard data={baseData} />);
    const header = container.querySelector('.tool-card-header')!;
    fireEvent.keyDown(header, { key: 'Enter' });
    expect(container.querySelector('.tool-card-body:not(.collapsed)')).toBeTruthy();
  });
});
