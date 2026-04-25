import React from 'react';

interface SetupCardProps {
  title: string;
  subtitle: string;
  buttonLabel: string;
  onAction: () => void;
}

export const SetupCard: React.FC<SetupCardProps> = ({ title, subtitle, buttonLabel, onAction }) => (
  <div className="setup-card">
    <div className="setup-card-title">{title}</div>
    <div className="setup-card-subtitle">{subtitle}</div>
    <button className="setup-card-button" onClick={onAction}>{buttonLabel}</button>
  </div>
);
