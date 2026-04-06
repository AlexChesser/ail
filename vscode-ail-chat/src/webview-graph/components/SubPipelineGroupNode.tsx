/**
 * SubPipelineGroupNode — a container node representing an expanded sub-pipeline.
 *
 * Rendered as a labeled card with the pipeline name and step count.
 * Edges connect to this node; the inner steps are laid out as regular nodes
 * that visually sit "inside" the group via indented positioning.
 */

import React, { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import type { StepNodeData } from '../types';

function SubPipelineGroupNodeInner({ data }: NodeProps): React.ReactElement {
  const nodeData = data as unknown as StepNodeData;
  const color = 'var(--vscode-charts-orange, #f97316)';

  return (
    <div
      style={{
        background: 'var(--vscode-editor-background)',
        border: `2px dashed ${color}`,
        borderRadius: 10,
        padding: '8px 14px',
        minWidth: 160,
        maxWidth: 240,
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
