import React, { useState } from 'react';

export interface ToolCallData {
  toolUseId: string;
  toolName: string;
  input: unknown;
  result?: string;
  isError?: boolean;
}

export interface ToolCallCardProps {
  data: ToolCallData;
}

/** Extract the most relevant argument from tool input for compact display. */
function extractPrimaryArg(input: unknown): string | null {
  if (input == null || typeof input !== 'object') return null;
  const obj = input as Record<string, unknown>;
  // Priority order: file_path, command, query, pattern, path, url
  for (const key of ['file_path', 'command', 'query', 'pattern', 'path', 'url', 'content']) {
    if (typeof obj[key] === 'string') {
      const val = obj[key] as string;
      // Truncate long values
      return val.length > 60 ? val.slice(0, 57) + '…' : val;
    }
  }
  return null;
}

/** Derive a short result summary (e.g., "Read 30 lines"). */
function resultSummary(result: string | undefined, isError: boolean | undefined): string | null {
  if (result === undefined) return null;
  if (isError) return 'error';
  const lines = result.split('\n').length;
  if (lines > 1) {
    return `${lines} lines`;
  }
  const chars = result.length;
  if (chars > 80) {
    return `${chars} chars`;
  }
  return null;
}

export const ToolCallCard: React.FC<ToolCallCardProps> = ({ data }) => {
  const [collapsed, setCollapsed] = useState(true);
  const hasResult = data.result !== undefined;

  const inputStr = data.input != null
    ? JSON.stringify(data.input, null, 2)
    : '';

  const primaryArg = extractPrimaryArg(data.input);
  const summary = resultSummary(data.result, data.isError);
  const statusLabel = hasResult
    ? (data.isError ? 'error' : 'done')
    : 'pending';

  return (
    <div className="tool-card">
      <div
        className="tool-card-header"
        onClick={() => setCollapsed((c) => !c)}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => e.key === 'Enter' && setCollapsed((c) => !c)}
        aria-expanded={!collapsed}
      >
        <span className="tool-card-name">
          {data.toolName}{primaryArg ? `(${primaryArg})` : ''}
        </span>
        {summary && (
          <span className="tool-card-summary">
            {summary}
          </span>
        )}
        <span className={`tool-card-status${data.isError ? ' error' : ''}`}>
          {statusLabel}
        </span>
        <span className="tool-card-toggle">
          {collapsed ? '(expand)' : '(collapse)'}
        </span>
      </div>
      <div className={`tool-card-body${collapsed ? ' collapsed' : ''}`}>
        {inputStr && (
          <>
            <div className="tool-card-section-label">Input</div>
            <pre className="tool-card-code">{inputStr}</pre>
          </>
        )}
        {hasResult && (
          <>
            <div className="tool-card-section-label">Result</div>
            <pre className={`tool-card-code${data.isError ? ' error' : ''}`}>
              {data.result}
            </pre>
          </>
        )}
      </div>
    </div>
  );
};
