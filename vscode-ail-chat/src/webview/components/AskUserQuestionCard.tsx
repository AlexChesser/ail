import React, { useState } from 'react';

export interface AskUserQuestionOption {
  label: string;
  description?: string;
}

export interface AskUserQuestion {
  header: string;
  question: string;
  multiSelect?: boolean;
  options: AskUserQuestionOption[];
}

export type AskUserCardState = 'pending' | 'resolved';

export interface AskUserQuestionCardProps {
  questions: AskUserQuestion[];
  cardState: AskUserCardState;
  resolvedAnswer?: string;
  onSubmit: (answer: string) => void;
  onDeny: () => void;
}

export const AskUserQuestionCard: React.FC<AskUserQuestionCardProps> = ({
  questions,
  cardState,
  resolvedAnswer,
  onSubmit,
  onDeny,
}) => {
  const question = questions[0];
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [somethingElse, setSomethingElse] = useState(false);
  const [freeText, setFreeText] = useState('');

  if (!question) {
    return null;
  }

  const isMultiSelect = question.multiSelect ?? false;

  const handleOptionToggle = (label: string) => {
    setSomethingElse(false);
    if (isMultiSelect) {
      setSelected((prev) => {
        const next = new Set(prev);
        if (next.has(label)) {
          next.delete(label);
        } else {
          next.add(label);
        }
        return next;
      });
    } else {
      setSelected(new Set([label]));
    }
  };

  const handleSomethingElse = () => {
    setSomethingElse(true);
    setSelected(new Set());
  };

  const handleSubmit = () => {
    if (somethingElse) {
      onSubmit(freeText);
    } else {
      onSubmit(Array.from(selected).join(', '));
    }
  };

  const canSubmit = somethingElse ? freeText.trim().length > 0 : selected.size > 0;

  return (
    <div className="permission-card">
      <div className="permission-card-title">
        <span className="permission-card-icon codicon codicon-question" />
        <span>{question.header}</span>
      </div>
      <div className="permission-card-detail">{question.question}</div>
      {cardState === 'pending' && (
        <div className="ask-user-options">
          {question.options.map((opt) => (
            <label
              key={opt.label}
              className={`ask-user-option ${selected.has(opt.label) ? 'ask-user-option-selected' : ''}`}
            >
              <input
                type={isMultiSelect ? 'checkbox' : 'radio'}
                name="ask-user-choice"
                checked={selected.has(opt.label)}
                onChange={() => handleOptionToggle(opt.label)}
              />
              <span className="ask-user-option-label">{opt.label}</span>
              {opt.description && (
                <span className="ask-user-option-description">{opt.description}</span>
              )}
            </label>
          ))}
          <label
            className={`ask-user-option ${somethingElse ? 'ask-user-option-selected' : ''}`}
          >
            <input
              type={isMultiSelect ? 'checkbox' : 'radio'}
              name="ask-user-choice"
              checked={somethingElse}
              onChange={handleSomethingElse}
            />
            <span className="ask-user-option-label">Something else...</span>
          </label>
          {somethingElse && (
            <textarea
              className="ask-user-freetext"
              value={freeText}
              onChange={(e) => setFreeText(e.target.value)}
              placeholder="Type your answer..."
              rows={3}
            />
          )}
          <div className="permission-card-actions">
            <button className="btn-primary" disabled={!canSubmit} onClick={handleSubmit}>
              Submit
            </button>
            <button className="btn-danger" onClick={onDeny}>
              Dismiss
            </button>
          </div>
        </div>
      )}
      {cardState === 'resolved' && (
        <div className="permission-card-resolved">
          {resolvedAnswer != null ? `\u2713 ${resolvedAnswer}` : '\u2717 Dismissed'}
        </div>
      )}
    </div>
  );
};
