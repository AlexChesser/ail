import React from 'react';

export type ChatRole = 'user' | 'assistant';

export interface ChatMessageProps {
  role: ChatRole;
  content: string;
  /** When true, show a blinking cursor at the end (streaming). */
  streaming?: boolean;
  /** Unix timestamp (ms) when this message was created. */
  timestamp?: number;
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export const ChatMessage: React.FC<ChatMessageProps> = ({ role, content, streaming, timestamp }) => {
  return (
    <div className={`chat-message ${role}`}>
      <div className="chat-message-role">{role === 'user' ? 'You' : 'ail'}</div>
      <div className={`chat-message-content${streaming ? ' streaming' : ''}`}>
        {content}
      </div>
      {timestamp !== undefined && (
        <span className="chat-message-timestamp">{formatTime(timestamp)}</span>
      )}
    </div>
  );
};
