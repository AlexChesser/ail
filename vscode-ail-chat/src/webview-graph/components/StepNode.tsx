/**
 * StepNode — custom React Flow node for a pipeline step.
 *
 * Collapsed view: shows step ID + type badge.
 * Invocation nodes get a distinct double-border diamond-corner style.
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

function StepNodeInner({ data, selected }: NodeProps): React.ReactElement {
  const nodeData = data as unknown as StepNodeData;
  const color = TYPE_COLORS[nodeData.type] ?? 'var(--vscode-charts-blue)';
  const label = TYPE_LABELS[nodeData.type] ?? '?';
  const isInvocation = nodeData.type === 'invocation';

  return (
    <div
      style={{
        background: isInvocation
          ? 'var(--vscode-editor-background)'
          : 'var(--vscode-editor-background)',
        border: `2px solid ${color}`,
        borderRadius: isInvocation ? 12 : 8,
        padding: isInvocation ? '10px 16px' : '8px 12px',
        minWidth: isInvocation ? 180 : 160,
        maxWidth: 220,
        cursor: 'pointer',
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        fontSize: 12,
        fontFamily: 'var(--vscode-font-family)',
        color: 'var(--vscode-editor-foreground)',
        boxShadow: isInvocation
          ? `0 0 0 3px var(--vscode-editor-background), 0 0 0 5px ${color}`
          : selected
            ? `0 0 0 2px ${color}40`
            : 'none',
        transition: 'box-shadow 0.15s ease',
      }}
    >
      <Handle type="target" position={Position.Top} style={{ background: color, width: 8, height: 8 }} />

      <span
        style={{
          background: color,
          color: '#fff',
          borderRadius: 3,
          padding: '2px 6px',
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
          fontWeight: isInvocation ? 700 : 500,
          fontSize: isInvocation ? 13 : 12,
        }}
      >
        {nodeData.stepId}
      </span>
      {nodeData.onResultCount != null && nodeData.onResultCount > 0 && (
        <span
          style={{
            marginLeft: 'auto',
            background: 'var(--vscode-charts-orange, #f97316)',
            color: '#fff',
            borderRadius: 8,
            padding: '0 5px',
            fontSize: 9,
            fontWeight: 600,
            flexShrink: 0,
          }}
          title={`${nodeData.onResultCount} on_result branches`}
        >
          {nodeData.onResultCount}
        </span>
      )}

      <Handle type="source" position={Position.Bottom} style={{ background: color, width: 8, height: 8 }} />
    </div>
  );
}

export const StepNode = memo(StepNodeInner);
