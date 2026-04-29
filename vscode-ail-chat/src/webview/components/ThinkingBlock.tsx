import React, { useState } from 'react';

export interface ThinkingBlockProps {
  text: string;
  /** Unix timestamp (ms) when this thinking block was created. */
  timestamp?: number;
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

export const ThinkingBlock: React.FC<ThinkingBlockProps> = ({ text, timestamp }) => {
  const [collapsed, setCollapsed] = useState(true);
  const charCount = text.length;

  return (
    <div className="thinking-block">
      <div
        className="thinking-block-header"
        onClick={() => setCollapsed((c) => !c)}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => e.key === 'Enter' && setCollapsed((c) => !c)}
        aria-expanded={!collapsed}
      >
        <span className={`thinking-block-chevron${collapsed ? '' : ' expanded'} codicon codicon-chevron-right`} />
        <span className="thinking-block-icon codicon codicon-lightbulb" />
        <span className="thinking-block-label">Thinking</span>
        {collapsed && (
          <span className="thinking-block-preview">
            {text.slice(0, 80).replace(/\n/g, ' ')}{charCount > 80 ? '\u2026' : ''}
          </span>
        )}
        <span className="thinking-block-meta">{charCount.toLocaleString()} chars</span>
        {timestamp !== undefined && (
          <span className="thinking-block-timestamp" title={new Date(timestamp).toLocaleString()}>{formatTime(timestamp)}</span>
        )}
      </div>
      <div className={`thinking-block-content${collapsed ? ' collapsed' : ''}`}>
        {text}
      </div>
    </div>
  );
};
