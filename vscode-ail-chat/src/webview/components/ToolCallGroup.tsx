import React, { useState, useRef, useEffect } from 'react';
import { DisplayItem } from '../App';

export interface ToolCallGroupProps {
  items: DisplayItem[];
  allResolved: boolean;
  renderItem: (item: DisplayItem) => React.ReactNode;
}

function groupStatusIcon(items: DisplayItem[]): React.ReactNode {
  const hasError = items.some(
    (it) => (it.kind === 'tool-call' && it.data.isError) ||
             (it.kind === 'permission' && it.cardState === 'resolved' && it.resolvedAllowed === false)
  );
  const hasPending = items.some(
    (it) => (it.kind === 'tool-call' && it.data.result === undefined) ||
             (it.kind === 'permission' && it.cardState === 'pending')
  );
  if (hasPending) {
    return <span className="tool-card-status-icon pending codicon codicon-loading" />;
  }
  if (hasError) {
    return <span className="tool-card-status-icon error codicon codicon-error" />;
  }
  return <span className="tool-card-status-icon done codicon codicon-check" />;
}

export const ToolCallGroup: React.FC<ToolCallGroupProps> = ({ items, allResolved, renderItem }) => {
  const [groupCollapsed, setGroupCollapsed] = useState(false);
  const prevAllResolved = useRef(allResolved);

  // Auto-collapse once everything in the group resolves
  useEffect(() => {
    if (!prevAllResolved.current && allResolved) {
      setGroupCollapsed(true);
    }
    prevAllResolved.current = allResolved;
  }, [allResolved]);

  const toolCount = items.filter((it) => it.kind === 'tool-call').length;
  const permCount = items.filter((it) => it.kind === 'permission').length;
  const parts: string[] = [];
  if (toolCount > 0) parts.push(`${toolCount} tool call${toolCount !== 1 ? 's' : ''}`);
  if (permCount > 0) parts.push(`${permCount} permission${permCount !== 1 ? 's' : ''}`);
  const label = parts.join(', ');

  return (
    <div className="tool-group">
      <div
        className="tool-group-header"
        onClick={() => setGroupCollapsed((c) => !c)}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => e.key === 'Enter' && setGroupCollapsed((c) => !c)}
        aria-expanded={!groupCollapsed}
      >
        {groupStatusIcon(items)}
        <span className={`tool-card-chevron${groupCollapsed ? '' : ' expanded'} codicon codicon-chevron-right`} />
        <span className="tool-group-count">{label}</span>
      </div>
      <div className={`tool-group-body${groupCollapsed ? ' collapsed' : ''}`}>
        {items.map((item) => (
          <div key={item.id}>{renderItem(item)}</div>
        ))}
      </div>
    </div>
  );
};
