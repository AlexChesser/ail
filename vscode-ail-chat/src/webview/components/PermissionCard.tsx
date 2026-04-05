import React from 'react';

export type PermissionCardState = 'pending' | 'resolved';

export interface PermissionCardProps {
  displayName: string;
  displayDetail: string;
  cardState: PermissionCardState;
  resolvedAllowed?: boolean;
  onAllow: () => void;
  onDeny: () => void;
}

export const PermissionCard: React.FC<PermissionCardProps> = ({
  displayName,
  displayDetail,
  cardState,
  resolvedAllowed,
  onAllow,
  onDeny,
}) => {
  return (
    <div className="permission-card">
      <div className="permission-card-title">🔒 Permission requested: {displayName}</div>
      <div className="permission-card-detail">{displayDetail}</div>
      {cardState === 'pending' && (
        <div className="permission-card-actions">
          <button className="btn-primary" onClick={onAllow}>Allow</button>
          <button className="btn-danger" onClick={onDeny}>Deny</button>
        </div>
      )}
      {cardState === 'resolved' && (
        <div className="hitl-card-resolved">
          {resolvedAllowed ? '✓ Allowed' : '✗ Denied'}
        </div>
      )}
    </div>
  );
};
