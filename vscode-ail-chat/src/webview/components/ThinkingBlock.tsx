import React, { useState } from 'react';

export interface ThinkingBlockProps {
  text: string;
}

export const ThinkingBlock: React.FC<ThinkingBlockProps> = ({ text }) => {
  const [collapsed, setCollapsed] = useState(true);

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
        <span>✦</span>
        <span>Thinking...</span>
        {collapsed && <span style={{ marginLeft: 4, fontSize: 10, opacity: 0.7 }}>
          {text.slice(0, 60).replace(/\n/g, ' ')}{text.length > 60 ? '…' : ''}
        </span>}
      </div>
      <div className={`thinking-block-content${collapsed ? ' collapsed' : ''}`}>
        {text}
      </div>
    </div>
  );
};
