/**
 * DetailPanel — right-side panel showing expanded step details.
 *
 * Progressive disclosure: shows summary by default, sections expand on click.
 */

import React, { useState } from 'react';
import type { StepNodeData } from '../types';

interface DetailPanelProps {
  data: StepNodeData;
  onClose: () => void;
  onOpenInEditor: (sourceFile: string, sourceLine: number) => void;
}

export function DetailPanel({ data, onClose, onOpenInEditor }: DetailPanelProps): React.ReactElement {
  return (
    <div
      style={{
        width: 300,
        borderLeft: '1px solid var(--vscode-panel-border)',
        background: 'var(--vscode-sideBar-background)',
        color: 'var(--vscode-sideBar-foreground)',
        overflow: 'auto',
        padding: 12,
        fontSize: 12,
        fontFamily: 'var(--vscode-font-family)',
      }}
    >
      {/* Header */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
        <strong style={{ fontSize: 14 }}>{data.stepId}</strong>
        <button
          onClick={onClose}
          style={{
            background: 'transparent',
            border: 'none',
            color: 'var(--vscode-icon-foreground)',
            cursor: 'pointer',
            fontSize: 16,
            padding: '2px 6px',
          }}
          title="Close"
        >
          ✕
        </button>
      </div>

      {/* Type badge */}
      <div style={{ marginBottom: 12 }}>
        <TypeBadge type={data.type} />
        {data.model && (
          <span
            style={{
              marginLeft: 6,
              background: 'var(--vscode-badge-background)',
              color: 'var(--vscode-badge-foreground)',
              borderRadius: 3,
              padding: '1px 5px',
              fontSize: 10,
            }}
          >
            {data.model}
          </span>
        )}
      </div>

      {/* Open in Editor link */}
      <div style={{ marginBottom: 12 }}>
        <button
          onClick={() => onOpenInEditor(data.sourceFile, data.sourceLine)}
          style={{
            background: 'transparent',
            border: 'none',
            color: 'var(--vscode-textLink-foreground)',
            cursor: 'pointer',
            padding: 0,
            fontSize: 11,
            textDecoration: 'underline',
          }}
        >
          Open in editor →
        </button>
        <div style={{ fontSize: 10, color: 'var(--vscode-descriptionForeground)', marginTop: 2 }}>
          {data.sourceFile.split('/').slice(-2).join('/')}:{data.sourceLine + 1}
        </div>
      </div>

      {/* Expandable sections */}
      {data.prompt && (
        <ExpandableSection title="Prompt" preview={truncate(data.prompt, 80)}>
          <pre style={preStyle}>{data.prompt}</pre>
        </ExpandableSection>
      )}

      {data.systemPrompt && (
        <ExpandableSection title="System Prompt" preview={truncate(data.systemPrompt, 60)}>
          <pre style={preStyle}>{data.systemPrompt}</pre>
        </ExpandableSection>
      )}

      {data.appendSystemPromptCount != null && data.appendSystemPromptCount > 0 && (
        <div style={sectionStyle}>
          <span style={sectionLabelStyle}>Append System Prompt</span>
          <span style={{ color: 'var(--vscode-descriptionForeground)' }}>
            {data.appendSystemPromptCount} {data.appendSystemPromptCount === 1 ? 'entry' : 'entries'}
          </span>
        </div>
      )}

      {data.tools && (
        <ExpandableSection
          title="Tools"
          preview={`${data.tools.allow.length} allowed${data.tools.deny.length ? `, ${data.tools.deny.length} denied` : ''}`}
        >
          {data.tools.allow.length > 0 && (
            <div style={{ marginBottom: 4 }}>
              <strong style={{ fontSize: 10, color: 'var(--vscode-charts-green)' }}>Allow:</strong>
              <div style={{ paddingLeft: 8 }}>
                {data.tools.allow.map((t) => (
                  <div key={t} style={{ fontSize: 11 }}>{t}</div>
                ))}
              </div>
            </div>
          )}
          {data.tools.deny.length > 0 && (
            <div>
              <strong style={{ fontSize: 10, color: 'var(--vscode-charts-red)' }}>Deny:</strong>
              <div style={{ paddingLeft: 8 }}>
                {data.tools.deny.map((t) => (
                  <div key={t} style={{ fontSize: 11 }}>{t}</div>
                ))}
              </div>
            </div>
          )}
        </ExpandableSection>
      )}

      {data.onResultCount != null && data.onResultCount > 0 && (
        <div style={sectionStyle}>
          <span style={sectionLabelStyle}>on_result</span>
          <span style={{ color: 'var(--vscode-descriptionForeground)' }}>
            {data.onResultCount} {data.onResultCount === 1 ? 'branch' : 'branches'}
          </span>
        </div>
      )}

      {data.subPipelinePath && (
        <div style={sectionStyle}>
          <span style={sectionLabelStyle}>Sub-pipeline</span>
          <span style={{ color: 'var(--vscode-descriptionForeground)', fontSize: 11 }}>
            {data.subPipelinePath}
          </span>
        </div>
      )}
    </div>
  );
}

// ── Sub-components ──────────────────────────────────────────────────────────

function TypeBadge({ type }: { type: StepNodeData['type'] }): React.ReactElement {
  const colors: Record<string, string> = {
    invocation: 'var(--vscode-charts-purple, #a855f7)',
    prompt: 'var(--vscode-charts-blue, #3b82f6)',
    context: 'var(--vscode-charts-green, #22c55e)',
    pipeline: 'var(--vscode-charts-orange, #f97316)',
    action: 'var(--vscode-charts-yellow, #eab308)',
    skill: 'var(--vscode-charts-red, #ef4444)',
  };
  return (
    <span
      style={{
        background: colors[type] ?? colors.prompt,
        color: '#fff',
        borderRadius: 3,
        padding: '2px 6px',
        fontSize: 10,
        fontWeight: 600,
        textTransform: 'uppercase',
      }}
    >
      {type}
    </span>
  );
}

function ExpandableSection({
  title,
  preview,
  children,
}: {
  title: string;
  preview: string;
  children: React.ReactNode;
}): React.ReactElement {
  const [expanded, setExpanded] = useState(false);

  return (
    <div style={{ ...sectionStyle, flexDirection: 'column', alignItems: 'stretch' }}>
      <button
        onClick={() => setExpanded(!expanded)}
        style={{
          background: 'transparent',
          border: 'none',
          color: 'var(--vscode-editor-foreground)',
          cursor: 'pointer',
          padding: 0,
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          width: '100%',
          textAlign: 'left',
        }}
      >
        <span style={sectionLabelStyle}>{title}</span>
        <span style={{ fontSize: 10, color: 'var(--vscode-descriptionForeground)' }}>
          {expanded ? '▾' : '▸'}
        </span>
      </button>
      {!expanded && (
        <div
          style={{
            color: 'var(--vscode-descriptionForeground)',
            fontSize: 11,
            marginTop: 2,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {preview}
        </div>
      )}
      {expanded && <div style={{ marginTop: 4 }}>{children}</div>}
    </div>
  );
}

// ── Styles ──────────────────────────────────────────────────────────────────

const sectionStyle: React.CSSProperties = {
  padding: '8px 0',
  borderBottom: '1px solid var(--vscode-panel-border)',
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
};

const sectionLabelStyle: React.CSSProperties = {
  fontWeight: 600,
  fontSize: 11,
  color: 'var(--vscode-editor-foreground)',
};

const preStyle: React.CSSProperties = {
  background: 'var(--vscode-textCodeBlock-background)',
  padding: 8,
  borderRadius: 4,
  fontSize: 11,
  whiteSpace: 'pre-wrap',
  wordBreak: 'break-word',
  maxHeight: 200,
  overflow: 'auto',
  margin: 0,
};

function truncate(text: string, max: number): string {
  return text.length > max ? text.slice(0, max) + '…' : text;
}
