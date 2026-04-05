import React, { useState } from 'react';

export type HitlCardState = 'pending' | 'resolved' | 'cancelled';

export interface HitlCardProps {
  stepId: string;
  message?: string;
  cardState: HitlCardState;
  resolvedText?: string;
  onApprove: (stepId: string) => void;
  onReject: (stepId: string) => void;
}

export const HitlCard: React.FC<HitlCardProps> = ({
  stepId,
  message,
  cardState,
  resolvedText,
  onApprove,
  onReject,
}) => {
  const [modifyText, setModifyText] = useState('');
  const [showModify, setShowModify] = useState(false);

  const handleApprove = () => {
    onApprove(stepId);
  };

  const handleReject = () => {
    onReject(stepId);
  };

  const handleModifySubmit = () => {
    if (modifyText.trim()) {
      onApprove(stepId);
    }
  };

  return (
    <div className="hitl-card">
      <div className="hitl-card-title">
        <span className="hitl-card-icon codicon codicon-debug-pause" />
        <span>Pipeline paused — human review required</span>
      </div>
      {message && <div className="hitl-card-message">{message}</div>}

      {cardState === 'pending' && (
        <>
          {showModify ? (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              <textarea
                className="chat-input-textarea"
                value={modifyText}
                onChange={(e) => setModifyText(e.target.value)}
                placeholder="Type your modified instruction\u2026"
                rows={3}
                style={{ width: '100%', border: '1px solid var(--vscode-input-border)', borderRadius: 'var(--ail-radius)', background: 'var(--vscode-input-background)' }}
              />
              <div className="hitl-card-actions">
                <button className="btn-primary" onClick={handleModifySubmit} disabled={!modifyText.trim()}>
                  Submit
                </button>
                <button className="btn-secondary" onClick={() => { setShowModify(false); setModifyText(''); }}>
                  Cancel
                </button>
              </div>
            </div>
          ) : (
            <div className="hitl-card-actions">
              <button className="btn-primary" onClick={handleApprove}>Approve</button>
              <button className="btn-secondary" onClick={() => setShowModify(true)}>Modify</button>
              <button className="btn-danger" onClick={handleReject}>Reject</button>
            </div>
          )}
        </>
      )}

      {cardState === 'resolved' && (
        <div className="hitl-card-resolved">
          {'\u2713'} {resolvedText ?? 'Approved'}
        </div>
      )}

      {cardState === 'cancelled' && (
        <div className="hitl-card-resolved">
          Pipeline ended before response
        </div>
      )}
    </div>
  );
};
