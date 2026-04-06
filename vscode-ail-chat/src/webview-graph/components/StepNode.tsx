/**
 * StepNode — custom React Flow node for a pipeline step.
 *
 * Collapsed view: shows step ID + type badge.
 * Click to select → detail panel opens on the right.
 */

import React, { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import type { StepNodeData } from '../types';

const TYPE_COLORS: Record<StepNodeData['type'], string> = {
  invocation: 'var(--vscode-charts-purple, #a855f7)',
  prompt: 'var(--vscode-charts-blue, #3b82f6)',
  context: 'var(--vscode-charts-green, #22c55e)',
  pipeline: 'var(--vscode-charts-orange, #f97316)',
  action: 'var(--vscode-charts-yellow, #eab308)',
  skill: 'var(--vscode-charts-red, #ef4444)',
};

const TYPE_LABELS: Record<StepNodeData['type'], string> = {
  invocation: 'INV',
  prompt: 'PRM',
  context: 'CTX',
  pipeline: 'SUB',
  action: 'ACT',
  skill: 'SKL',
};

function StepNodeInner({ data }: NodeProps): React.ReactElement {
  const nodeData = data as unknown as StepNodeData;
  const color = TYPE_COLORS[nodeData.type] ?? 'var(--vscode-charts-blue)';
  const label = TYPE_LABELS[nodeData.type] ?? '?';

  return (
    <div
      style={{
        background: 'var(--vscode-editor-background)',
        border: `2px solid ${color}`,
        borderRadius: 8,
        padding: '8px 12px',
        minWidth: 160,
        maxWidth: 220,
        cursor: 'pointer',
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        fontSize: 12,
        fontFamily: 'var(--vscode-font-family)',
        color: 'var(--vscode-editor-foreground)',
      }}
    >
      <Handle type="target" position={Position.Top} style={{ background: color }} />

      <span
        style={{
          background: color,
          color: '#fff',
          borderRadius: 3,
          padding: '1px 5px',
          fontSize: 9,
          fontWeight: 700,
          letterSpacing: '0.5px',
          flexShrink: 0,
        }}
      >
        {label}
      </span>
      <span
        style={{
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
          fontWeight: 500,
        }}
      >
        {nodeData.stepId}
      </span>
      {nodeData.onResultCount != null && nodeData.onResultCount > 0 && (
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
        >
          {nodeData.onResultCount}
        </span>
      )}

      <Handle type="source" position={Position.Bottom} style={{ background: color }} />
    </div>
  );
}

export const StepNode = memo(StepNodeInner);
