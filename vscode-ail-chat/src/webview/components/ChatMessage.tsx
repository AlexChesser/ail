import React from 'react';

export type ChatRole = 'user' | 'assistant';

export interface ChatMessageProps {
  role: ChatRole;
  content: string;
  /** When true, show a blinking cursor at the end (streaming). */
  streaming?: boolean;
}

export const ChatMessage: React.FC<ChatMessageProps> = ({ role, content, streaming }) => {
  return (
    <div className={`chat-message ${role}`}>
      <div className="chat-message-role">{role === 'user' ? 'You' : 'ail'}</div>
      <div className={`chat-message-content${streaming ? ' streaming' : ''}`}>
        {content}
      </div>
    </div>
  );
};
