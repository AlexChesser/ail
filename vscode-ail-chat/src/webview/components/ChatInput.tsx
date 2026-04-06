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

  const resetHeight = () => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
  };

  const handleInput = () => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight}px`;
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      const value = textareaRef.current?.value.trim() ?? '';
      if (value && !isRunning && !disabled) {
        if (textareaRef.current) textareaRef.current.value = '';
        resetHeight();
        onSubmit(value);
      }
    }
  };

  const handleSendClick = () => {
    const value = textareaRef.current?.value.trim() ?? '';
    if (value && !isRunning && !disabled) {
      if (textareaRef.current) textareaRef.current.value = '';
      resetHeight();
      onSubmit(value);
    }
  };

  return (
    <div className="chat-input-area">
      <div className="chat-input-container">
        <textarea
          ref={textareaRef}
          className="chat-input-textarea"
          placeholder={placeholder ?? (isRunning ? 'Running\u2026' : 'Describe what to build')}
          disabled={isRunning || disabled}
          onKeyDown={handleKeyDown}
          onInput={handleInput}
          rows={1}
        />
        <div className="chat-input-send">
          {isRunning ? (
            <button className="btn-stop" onClick={onStop} title="Stop the current run">
              <span className="codicon codicon-debug-stop" /> Stop
            </button>
          ) : (
            <button
              className="btn-primary"
              onClick={handleSendClick}
              disabled={disabled}
              title="Send (Enter)"
            >
              Send
            </button>
          )}
        </div>
      </div>
      {!isRunning && !disabled && (
        <div className="chat-input-hint">
          Enter to send, Shift+Enter for newline
        </div>
      )}
    </div>
  );
};
