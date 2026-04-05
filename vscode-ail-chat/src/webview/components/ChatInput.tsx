import React, { useRef, KeyboardEvent } from 'react';

export interface ChatInputProps {
  onSubmit: (text: string) => void;
  onStop: () => void;
  isRunning: boolean;
  disabled?: boolean;
  placeholder?: string;
}

export const ChatInput: React.FC<ChatInputProps> = ({
  onSubmit,
  onStop,
  isRunning,
  disabled,
  placeholder,
}) => {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      const value = textareaRef.current?.value.trim() ?? '';
      if (value && !isRunning && !disabled) {
        if (textareaRef.current) textareaRef.current.value = '';
        onSubmit(value);
      }
    }
  };

  const handleSendClick = () => {
    const value = textareaRef.current?.value.trim() ?? '';
    if (value && !isRunning && !disabled) {
      if (textareaRef.current) textareaRef.current.value = '';
      onSubmit(value);
    }
  };

  return (
    <div className="chat-input-area">
      <div className="chat-input-row">
        <textarea
          ref={textareaRef}
          className="chat-input-textarea"
          placeholder={placeholder ?? (isRunning ? 'Running…' : 'Send a prompt (Enter to send, Shift+Enter for newline)')}
          disabled={isRunning || disabled}
          onKeyDown={handleKeyDown}
          rows={2}
        />
        {isRunning ? (
          <button className="btn-danger" onClick={onStop} title="Stop">■ Stop</button>
        ) : (
          <button
            className="btn-primary"
            onClick={handleSendClick}
            disabled={disabled}
          >
            Send
          </button>
        )}
      </div>
    </div>
  );
};
