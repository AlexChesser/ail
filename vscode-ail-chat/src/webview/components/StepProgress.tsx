import React from 'react';

export type StepStatus = 'pending' | 'running' | 'completed' | 'failed' | 'skipped';

export interface StepInfo {
  stepId: string;
  status: StepStatus;
  costUsd?: number;
}

export interface StepProgressProps {
  steps: StepInfo[];
  totalCostUsd?: number;
}

function glyph(status: StepStatus): React.ReactElement {
  switch (status) {
    case 'running':   return <span className="step-glyph running">⟳</span>;
    case 'completed': return <span className="step-glyph">✓</span>;
    case 'failed':    return <span className="step-glyph" style={{ color: 'var(--vscode-errorForeground)' }}>✗</span>;
    case 'skipped':   return <span className="step-glyph" style={{ opacity: 0.5 }}>–</span>;
    default:          return <span className="step-glyph" style={{ opacity: 0.4 }}>○</span>;
  }
}

export const StepProgress: React.FC<StepProgressProps> = ({ steps, totalCostUsd }) => {
  if (steps.length === 0) return null;

  return (
    <div className="step-progress">
      <div className="step-progress-title">Steps</div>
      {steps.map((step) => (
        <div key={step.stepId} className="step-row">
          {glyph(step.status)}
          <span className="step-id">{step.stepId}</span>
          {step.costUsd !== undefined && step.costUsd > 0 && (
            <span className="step-cost">${step.costUsd.toFixed(4)}</span>
          )}
        </div>
      ))}
      {totalCostUsd !== undefined && totalCostUsd > 0 && (
        <div className="run-summary">
          Total cost: ${totalCostUsd.toFixed(4)}
        </div>
      )}
    </div>
  );
};
