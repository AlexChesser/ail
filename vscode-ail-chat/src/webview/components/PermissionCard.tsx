import React, { useState } from 'react';

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
  const isResolved = cardState === 'resolved';
  const [collapsed, setCollapsed] = useState(false);

  // When pending, always show expanded. When resolved, respect collapsed state.
  const showBody = !isResolved || !collapsed;

  return (
    <div className="permission-card">
      <div
        className={`permission-card-header${isResolved ? ' clickable' : ''}`}
        onClick={isResolved ? () => setCollapsed((c) => !c) : undefined}
        role={isResolved ? 'button' : undefined}
        tabIndex={isResolved ? 0 : undefined}
        onKeyDown={isResolved ? (e) => e.key === 'Enter' && setCollapsed((c) => !c) : undefined}
        aria-expanded={isResolved ? !collapsed : undefined}
      >
        {isResolved && (
          <span className={`tool-card-chevron${collapsed ? '' : ' expanded'} codicon codicon-chevron-right`} />
        )}
        {isResolved ? (
          resolvedAllowed
            ? <span className="tool-card-status-icon done codicon codicon-check" />
            : <span className="tool-card-status-icon error codicon codicon-close" />
        ) : (
          <span className="permission-card-icon codicon codicon-lock" />
        )}
        <span className="permission-card-title-text">
          {isResolved
            ? `${resolvedAllowed ? 'Allowed' : 'Denied'}: ${displayName}`
            : `Permission requested: ${displayName}`}
        </span>
      </div>
      {showBody && (
        <div className="permission-card-body">
          <div className="permission-card-detail">{displayDetail}</div>
          {cardState === 'pending' && (
            <div className="permission-card-actions">
              <button className="btn-primary" onClick={onAllow}>Allow</button>
              <button className="btn-danger" onClick={onDeny}>Deny</button>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
