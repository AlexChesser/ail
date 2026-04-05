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

export const ToolCallCard: React.FC<ToolCallCardProps> = ({ data }) => {
  const [collapsed, setCollapsed] = useState(true);
  const hasResult = data.result !== undefined;

  const inputStr = data.input != null
    ? JSON.stringify(data.input, null, 2)
    : '';

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
        <span>{collapsed ? '▶' : '▼'}</span>
        <span className="tool-card-name">{data.toolName}</span>
        <span className={`tool-card-status${data.isError ? ' error' : ''}`}>
          {statusLabel}
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
