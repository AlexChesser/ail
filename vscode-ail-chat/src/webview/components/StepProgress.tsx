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

function glyphChar(status: StepStatus): string {
  switch (status) {
    case 'running':
    case 'completed':
    case 'failed':
      return '\u25CF'; // ●
    default:
      return '\u25CB'; // ○
  }
}

export const StepProgress: React.FC<StepProgressProps> = ({ steps, totalCostUsd }) => {
  if (steps.length === 0) return null;

  return (
    <div className="step-progress">
      {steps.map((step) => (
        <div key={step.stepId} className="step-badge">
          <span className={`step-glyph ${step.status}`}>{glyphChar(step.status)}</span>
          <span className="step-id">{step.stepId}</span>
          {step.costUsd !== undefined && step.costUsd > 0 && (
            <span className="step-cost">${step.costUsd.toFixed(4)}</span>
          )}
        </div>
      ))}
      {totalCostUsd !== undefined && totalCostUsd > 0 && (
        <span className="run-summary">
          Total: ${totalCostUsd.toFixed(4)}
        </span>
      )}
    </div>
  );
};
