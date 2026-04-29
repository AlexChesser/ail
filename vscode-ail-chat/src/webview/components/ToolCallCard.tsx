import React, { useCallback, useState } from 'react';

export interface ToolCallData {
  toolUseId: string;
  toolName: string;
  input: unknown;
  result?: string;
  isError?: boolean;
  /** Set to true when the pipeline was stopped before this tool call completed. */
  isStopped?: boolean;
}

export interface ToolCallCardProps {
  data: ToolCallData;
  /** Unix timestamp (ms) when this tool call started. */
  timestamp?: number;
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
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
      return val.length > 60 ? val.slice(0, 57) + '\u2026' : val;
    }
  }
  return null;
}

/** Derive a short result summary (e.g., "30 lines"). */
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

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  }, [text]);

  return (
    <button
      className={`tool-card-copy-btn${copied ? ' copied' : ''}`}
      onClick={handleCopy}
      title={copied ? 'Copied!' : 'Copy to clipboard'}
      aria-label={copied ? 'Copied!' : 'Copy to clipboard'}
    >
      <span className={`codicon ${copied ? 'codicon-check' : 'codicon-copy'}`} />
    </button>
  );
}

function StatusIcon({ data }: { data: ToolCallData }) {
  const hasResult = data.result !== undefined;
  if (!hasResult) {
    if (data.isStopped) {
      return (
        <span className="tool-card-status-icon stopped codicon codicon-circle-slash" />
      );
    }
    return (
      <span className="tool-card-status-icon pending codicon codicon-loading" />
    );
  }
  if (data.isError) {
    return (
      <span className="tool-card-status-icon error codicon codicon-error" />
    );
  }
  return (
    <span className="tool-card-status-icon done codicon codicon-check" />
  );
}

export const ToolCallCard: React.FC<ToolCallCardProps> = ({ data, timestamp }) => {
  const [collapsed, setCollapsed] = useState(true);

  const inputStr = data.input != null
    ? JSON.stringify(data.input, null, 2)
    : '';

  const primaryArg = extractPrimaryArg(data.input);
  const summary = resultSummary(data.result, data.isError);
  const hasResult = data.result !== undefined;

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
        <StatusIcon data={data} />
        <span className={`tool-card-chevron${collapsed ? '' : ' expanded'} codicon codicon-chevron-right`} />
        <span className="tool-card-name">{data.toolName}</span>
        {primaryArg && (
          <span className="tool-card-primary-arg">{primaryArg}</span>
        )}
        {summary && (
          <span className="tool-card-summary">{summary}</span>
        )}
        {timestamp !== undefined && (
          <span className="tool-card-timestamp" title={new Date(timestamp).toLocaleString()}>{formatTime(timestamp)}</span>
        )}
      </div>
      <div className={`tool-card-body${collapsed ? ' collapsed' : ''}`}>
        {inputStr && (
          <>
            <div className="tool-card-section-label">Input</div>
            <div className="tool-card-code-wrapper">
              <pre className="tool-card-code">{inputStr}</pre>
              <CopyButton text={inputStr} />
            </div>
          </>
        )}
        {hasResult && (
          <>
            <div className="tool-card-section-label">Result</div>
            <div className="tool-card-code-wrapper">
              <pre className={`tool-card-code${data.isError ? ' error' : ''}`}>
                {data.result}
              </pre>
              <CopyButton text={data.result ?? ''} />
            </div>
          </>
        )}
      </div>
    </div>
  );
};
