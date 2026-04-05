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
    case 'running':   return <span className="step-glyph running">●</span>;
    case 'completed': return <span className="step-glyph completed">●</span>;
    case 'failed':    return <span className="step-glyph failed">●</span>;
    case 'skipped':   return <span className="step-glyph skipped">○</span>;
    default:          return <span className="step-glyph pending">○</span>;
  }
}

function rowClass(index: number, total: number): string {
  if (total === 1) return 'step-row step-row--only';
  if (index === total - 1) return 'step-row step-row--last';
  return 'step-row';
}

export const StepProgress: React.FC<StepProgressProps> = ({ steps, totalCostUsd }) => {
  if (steps.length === 0) return null;

  return (
    <div className="step-progress">
      {steps.map((step, i) => (
        <div key={step.stepId} className={rowClass(i, steps.length)}>
          {glyph(step.status)}
          <span className="step-id">{step.stepId}</span>
          {step.costUsd !== undefined && step.costUsd > 0 && (
            <span className="step-cost">${step.costUsd.toFixed(4)}</span>
          )}
        </div>
      ))}
      {totalCostUsd !== undefined && totalCostUsd > 0 && (
        <div className="run-summary">
          Total: ${totalCostUsd.toFixed(4)}
        </div>
      )}
    </div>
  );
};
