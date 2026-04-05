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
      <div className="permission-card-title">
        <span className="permission-card-icon codicon codicon-lock" />
        <span>Permission requested: {displayName}</span>
      </div>
      <div className="permission-card-detail">{displayDetail}</div>
      {cardState === 'pending' && (
        <div className="permission-card-actions">
          <button className="btn-primary" onClick={onAllow}>Allow</button>
          <button className="btn-danger" onClick={onDeny}>Deny</button>
        </div>
      )}
      {cardState === 'resolved' && (
        <div className="permission-card-resolved">
          {resolvedAllowed ? '\u2713 Allowed' : '\u2717 Denied'}
        </div>
      )}
    </div>
  );
};
