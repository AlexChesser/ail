/**
 * DetailPanel — right-side panel showing expanded step details.
 *
 * Progressive disclosure: shows summary by default, sections expand on click.
 * P3: rich content for on_result branches, append_system_prompt entries,
 * shell commands, action kinds, conditions, and file path detection.
 */

import React, { useState } from 'react';
import type { StepNodeData, OnResultBranch, AppendSystemPromptEntry } from '../types';

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
          style={closeBtnStyle}
          title="Close"
        >
          ✕
        </button>
      </div>

      {/* Type badge + model + condition */}
      <div style={{ marginBottom: 12, display: 'flex', flexWrap: 'wrap', gap: 4, alignItems: 'center' }}>
        <TypeBadge type={data.type} />
        {data.model && <Badge text={data.model} />}
        {data.condition === 'never' && <Badge text="skipped" color="var(--vscode-charts-yellow, #eab308)" />}
        {data.resume && <Badge text="resume" color="var(--vscode-charts-green, #22c55e)" />}
      </div>

      {/* Open in Editor link */}
      <div style={{ marginBottom: 12 }}>
        <button
          onClick={() => onOpenInEditor(data.sourceFile, data.sourceLine)}
          style={linkBtnStyle}
        >
          Open in editor →
        </button>
        <div style={{ fontSize: 10, color: 'var(--vscode-descriptionForeground)', marginTop: 2 }}>
          {data.sourceFile.split('/').slice(-2).join('/')}:{data.sourceLine + 1}
        </div>
      </div>

      {/* Prompt */}
      {data.prompt && (
        <ExpandableSection
          title="Prompt"
          preview={data.promptIsFile ? data.prompt : truncate(data.prompt, 80)}
          icon={data.promptIsFile ? 'file' : 'text'}
        >
          {data.promptIsFile ? (
            <div style={filePathStyle}>{data.prompt}</div>
          ) : (
            <pre style={preStyle}>{data.prompt}</pre>
          )}
        </ExpandableSection>
      )}

      {/* System Prompt */}
      {data.systemPrompt && (
        <ExpandableSection
          title="System Prompt"
          preview={data.systemPromptIsFile ? data.systemPrompt : truncate(data.systemPrompt, 60)}
          icon={data.systemPromptIsFile ? 'file' : 'text'}
        >
          {data.systemPromptIsFile ? (
            <div style={filePathStyle}>{data.systemPrompt}</div>
          ) : (
            <pre style={preStyle}>{data.systemPrompt}</pre>
          )}
        </ExpandableSection>
      )}

      {/* Append System Prompt */}
      {data.appendSystemPromptEntries && data.appendSystemPromptEntries.length > 0 && (
        <ExpandableSection
          title="Append System Prompt"
          preview={`${data.appendSystemPromptEntries.length} ${data.appendSystemPromptEntries.length === 1 ? 'entry' : 'entries'}`}
        >
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {data.appendSystemPromptEntries.map((entry, i) => (
              <AppendEntryRow key={i} entry={entry} />
            ))}
          </div>
        </ExpandableSection>
      )}

      {/* Shell Command (context steps) */}
      {data.shellCommand && (
        <ExpandableSection title="Shell Command" preview={truncate(data.shellCommand.trim(), 60)}>
          <pre style={preStyle}>{data.shellCommand}</pre>
        </ExpandableSection>
      )}

      {/* Tools */}
      {data.tools && (
        <ExpandableSection
          title="Tools"
          preview={`${data.tools.allow.length} allowed${data.tools.deny.length ? `, ${data.tools.deny.length} denied` : ''}`}
        >
          {data.tools.allow.length > 0 && (
            <div style={{ marginBottom: 4 }}>
              <strong style={{ fontSize: 10, color: 'var(--vscode-charts-green, #22c55e)' }}>Allow:</strong>
              <div style={toolListStyle}>
                {data.tools.allow.map((t) => (
                  <span key={t} style={toolChipStyle}>{t}</span>
                ))}
              </div>
            </div>
          )}
          {data.tools.deny.length > 0 && (
            <div>
              <strong style={{ fontSize: 10, color: 'var(--vscode-charts-red, #ef4444)' }}>Deny:</strong>
              <div style={toolListStyle}>
                {data.tools.deny.map((t) => (
                  <span key={t} style={{ ...toolChipStyle, borderColor: 'var(--vscode-charts-red, #ef4444)' }}>{t}</span>
                ))}
              </div>
            </div>
          )}
        </ExpandableSection>
      )}

      {/* on_result branches */}
      {data.onResultBranches && data.onResultBranches.length > 0 && (
        <ExpandableSection
          title="on_result"
          preview={`${data.onResultBranches.length} ${data.onResultBranches.length === 1 ? 'branch' : 'branches'}`}
        >
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {data.onResultBranches.map((branch, i) => (
              <OnResultRow key={i} branch={branch} />
            ))}
          </div>
        </ExpandableSection>
      )}

      {/* Action kind */}
      {data.actionKind && (
        <div style={sectionStyle}>
          <span style={sectionLabelStyle}>Action</span>
          <Badge text={data.actionKind} color="var(--vscode-charts-yellow, #eab308)" />
        </div>
      )}

      {/* HITL Message */}
      {data.message && (
        <ExpandableSection title="HITL Message" preview={truncate(data.message, 60)}>
          <pre style={preStyle}>{data.message}</pre>
        </ExpandableSection>
      )}

      {/* Sub-pipeline path */}
      {data.subPipelinePath && (
        <div style={sectionStyle}>
          <span style={sectionLabelStyle}>Sub-pipeline</span>
          <span style={{ color: 'var(--vscode-descriptionForeground)', fontSize: 11 }}>
            {data.subPipelinePath}
          </span>
        </div>
      )}

      {/* Group step count */}
      {data.isSubPipelineGroup && data.childStepCount != null && (
        <div style={sectionStyle}>
          <span style={sectionLabelStyle}>Steps</span>
          <span style={{ color: 'var(--vscode-descriptionForeground)' }}>
            {data.childStepCount} {data.childStepCount === 1 ? 'step' : 'steps'}
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

function Badge({ text, color }: { text: string; color?: string }): React.ReactElement {
  return (
    <span
      style={{
        background: color ?? 'var(--vscode-badge-background)',
        color: color ? '#fff' : 'var(--vscode-badge-foreground)',
        borderRadius: 3,
        padding: '1px 5px',
        fontSize: 10,
        fontWeight: 500,
      }}
    >
      {text}
    </span>
  );
}

function OnResultRow({ branch }: { branch: OnResultBranch }): React.ReactElement {
  return (
    <div
      style={{
        background: 'var(--vscode-textCodeBlock-background)',
        borderRadius: 4,
        padding: '6px 8px',
        fontSize: 11,
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 4 }}>
        <span style={{ color: 'var(--vscode-charts-orange, #f97316)', fontWeight: 600 }}>
          {branch.matcher}
        </span>
        <span style={{ color: 'var(--vscode-descriptionForeground)', fontSize: 10, flexShrink: 0 }}>
          → {branch.action}
        </span>
      </div>
      {branch.prompt && (
        <div
          style={{
            marginTop: 3,
            color: 'var(--vscode-descriptionForeground)',
            fontSize: 10,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          prompt: {branch.prompt}
        </div>
      )}
    </div>
  );
}

function AppendEntryRow({ entry }: { entry: AppendSystemPromptEntry }): React.ReactElement {
  const typeIcons: Record<string, string> = { text: 'T', file: 'F', shell: '$' };
  const typeColors: Record<string, string> = {
    text: 'var(--vscode-charts-blue, #3b82f6)',
    file: 'var(--vscode-charts-orange, #f97316)',
    shell: 'var(--vscode-charts-green, #22c55e)',
  };
  return (
    <div
      style={{
        background: 'var(--vscode-textCodeBlock-background)',
        borderRadius: 4,
        padding: '4px 8px',
        fontSize: 11,
        display: 'flex',
        alignItems: 'flex-start',
        gap: 6,
      }}
    >
      <span
        style={{
          background: typeColors[entry.type] ?? typeColors.text,
          color: '#fff',
          borderRadius: 2,
          padding: '0 4px',
          fontSize: 9,
          fontWeight: 700,
          flexShrink: 0,
          marginTop: 1,
        }}
      >
        {typeIcons[entry.type] ?? '?'}
      </span>
      <span
        style={{
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: entry.value.includes('\n') ? 'pre-wrap' : 'nowrap',
          maxHeight: 60,
          color: 'var(--vscode-editor-foreground)',
        }}
      >
        {truncate(entry.value.trim(), 120)}
      </span>
    </div>
  );
}

function ExpandableSection({
  title,
  preview,
  icon,
  children,
}: {
  title: string;
  preview: string;
  icon?: 'file' | 'text';
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
        <span style={{ ...sectionLabelStyle, display: 'flex', alignItems: 'center', gap: 4 }}>
          {title}
          {icon === 'file' && <span style={{ fontSize: 9, color: 'var(--vscode-charts-orange, #f97316)' }}>file</span>}
        </span>
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

const filePathStyle: React.CSSProperties = {
  background: 'var(--vscode-textCodeBlock-background)',
  padding: '6px 8px',
  borderRadius: 4,
  fontSize: 11,
  fontFamily: 'var(--vscode-editor-font-family, monospace)',
  color: 'var(--vscode-charts-orange, #f97316)',
};

const toolListStyle: React.CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  gap: 3,
  paddingTop: 3,
};

const toolChipStyle: React.CSSProperties = {
  fontSize: 10,
  padding: '1px 5px',
  borderRadius: 3,
  border: '1px solid var(--vscode-panel-border)',
  background: 'var(--vscode-textCodeBlock-background)',
};

const closeBtnStyle: React.CSSProperties = {
  background: 'transparent',
  border: 'none',
  color: 'var(--vscode-icon-foreground)',
  cursor: 'pointer',
  fontSize: 16,
  padding: '2px 6px',
};

const linkBtnStyle: React.CSSProperties = {
  background: 'transparent',
  border: 'none',
  color: 'var(--vscode-textLink-foreground)',
  cursor: 'pointer',
  padding: 0,
  fontSize: 11,
  textDecoration: 'underline',
};

function truncate(text: string, max: number): string {
  return text.length > max ? text.slice(0, max) + '…' : text;
}
