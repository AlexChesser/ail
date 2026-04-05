import React from 'react';
import { SessionSummary } from '../../types';

export interface SessionListProps {
  sessions: SessionSummary[];
  activeSessionId: string | null;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
}

export const SessionList: React.FC<SessionListProps> = ({
  sessions,
  activeSessionId,
  onSelectSession,
  onNewSession,
}) => {
  return (
    <div className="sessions-panel">
      <div className="session-list-header">
        <span>Sessions</span>
        <button className="btn-icon" onClick={onNewSession} title="New session">+</button>
      </div>
      <div className="session-list-items">
        {sessions.map((s) => (
          <div
            key={s.id}
            className={`session-item${s.id === activeSessionId ? ' active' : ''}`}
            onClick={() => onSelectSession(s.id)}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => e.key === 'Enter' && onSelectSession(s.id)}
          >
            <div>{s.title || '(untitled)'}</div>
            <div className="session-item-date">
              {new Date(s.timestamp).toLocaleString(undefined, {
                month: 'short',
                day: 'numeric',
                hour: '2-digit',
                minute: '2-digit',
              })}
            </div>
          </div>
        ))}
        {sessions.length === 0 && (
          <div style={{ padding: '8px 10px', fontSize: 11, opacity: 0.6 }}>
            No sessions yet
          </div>
        )}
      </div>
    </div>
  );
};
