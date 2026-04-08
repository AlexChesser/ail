/**
 * SubPipelineGroupNode — a container node representing a sub-pipeline.
 *
 * When collapsed (default): shows pipeline name + step count. Double-click or
 * click the toggle button to expand and reveal child steps.
 * When expanded: the group node is hidden and child steps are shown inline.
 */

import React, { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import type { StepNodeData } from '../types';

interface GroupNodeExtras {
  _expanded?: boolean;
  _onToggle?: () => void;
}

function SubPipelineGroupNodeInner({ data }: NodeProps): React.ReactElement {
  const nodeData = data as unknown as StepNodeData & GroupNodeExtras;
  const color = 'var(--vscode-charts-orange, #f97316)';
  const isExpanded = nodeData._expanded ?? false;
  const onToggle = nodeData._onToggle;

  return (
    <div
      style={{
        background: 'var(--vscode-editor-background)',
        border: `2px dashed ${color}`,
        borderRadius: 10,
        padding: '8px 14px',
        minWidth: 170,
        maxWidth: 260,
        cursor: 'pointer',
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        fontSize: 12,
        fontFamily: 'var(--vscode-font-family)',
        color: 'var(--vscode-editor-foreground)',
      }}
    >
      <Handle type="target" position={Position.Top} style={{ background: color, width: 8, height: 8 }} />

      {/* Expand/collapse toggle */}
      <button
        onClick={(e) => {
          e.stopPropagation();
          onToggle?.();
        }}
        style={{
          background: 'transparent',
          border: 'none',
          color: 'var(--vscode-icon-foreground)',
          cursor: 'pointer',
          padding: 0,
          fontSize: 11,
          lineHeight: 1,
          flexShrink: 0,
          width: 14,
          textAlign: 'center',
        }}
        title={isExpanded ? 'Collapse sub-pipeline' : 'Expand sub-pipeline'}
      >
        {isExpanded ? '▾' : '▸'}
      </button>

      <span
        style={{
          background: color,
          color: 'var(--vscode-button-foreground, #fff)',
          borderRadius: 3,
          padding: '2px 6px',
          fontSize: 9,
          fontWeight: 700,
          letterSpacing: '0.5px',
          flexShrink: 0,
        }}
      >
        SUB
      </span>
      <span
        style={{
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
          fontWeight: 600,
        }}
      >
        {nodeData.pipelineName ?? nodeData.stepId}
      </span>
      {nodeData.childStepCount != null && (
        <span
          style={{
            marginLeft: 'auto',
            background: 'var(--vscode-badge-background)',
            color: 'var(--vscode-badge-foreground)',
            borderRadius: 8,
            padding: '0 5px',
            fontSize: 9,
            flexShrink: 0,
          }}
          title={`${nodeData.childStepCount} steps`}
        >
          {nodeData.childStepCount}
        </span>
      )}

      <Handle type="source" position={Position.Bottom} style={{ background: color, width: 8, height: 8 }} />
    </div>
  );
}

export const SubPipelineGroupNode = memo(SubPipelineGroupNodeInner);
