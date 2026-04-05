/**
 * App — root React component for the ail Chat webview.
 *
 * Handles postMessage from the extension host (HostToWebviewMessage) and
 * accumulates all chat state: messages, tool calls, thinking blocks,
 * HITL gates, permission requests, and step progress.
 */

import React, { useEffect, useReducer, useRef } from 'react';
import { HostToWebviewMessage, WebviewToHostMessage, SessionSummary } from '../types';
import { ChatMessage } from './components/ChatMessage';
import { ThinkingBlock } from './components/ThinkingBlock';
import { ToolCallCard, ToolCallData } from './components/ToolCallCard';
import { HitlCard, HitlCardState } from './components/HitlCard';
import { PermissionCard, PermissionCardState } from './components/PermissionCard';
import { AskUserQuestionCard, AskUserQuestion, AskUserCardState } from './components/AskUserQuestionCard';
import { ErrorBoundary } from './components/ErrorBoundary';
import { StepProgress, StepInfo, StepStatus } from './components/StepProgress';
import { ChatInput } from './components/ChatInput';
import { SessionList } from './components/SessionList';
import { StatusBar } from './components/StatusBar';
import { PipelineBar } from './components/PipelineBar';

// ── VS Code API ────────────────────────────────────────────────────────────────

declare function acquireVsCodeApi(): {
  postMessage: (msg: WebviewToHostMessage) => void;
  getState: () => unknown;
  setState: (state: unknown) => void;
};

// Acquire once; do not call acquireVsCodeApi() more than once per session.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const vscode = (typeof acquireVsCodeApi !== 'undefined' ? acquireVsCodeApi() : null) as any;

function postToHost(msg: WebviewToHostMessage): void {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-member-access
  vscode?.postMessage(msg);
}

// ── Display items ──────────────────────────────────────────────────────────────

/** A single item in the chat display list. */
export type DisplayItem =
  | { kind: 'user-message'; id: string; text: string }
  | { kind: 'assistant-stream'; id: string; text: string; streaming: boolean; stepId?: string }
  | { kind: 'thinking'; id: string; text: string }
  | { kind: 'tool-call'; id: string; data: ToolCallData }
  | { kind: 'hitl'; id: string; stepId: string; message?: string; cardState: HitlCardState; resolvedText?: string }
  | { kind: 'permission'; id: string; displayName: string; displayDetail: string; cardState: PermissionCardState; resolvedAllowed?: boolean }
  | { kind: 'ask-user-question'; id: string; questions: AskUserQuestion[]; cardState: AskUserCardState; resolvedAnswer?: string }
  | { kind: 'error'; id: string; message: string };

// ── State ──────────────────────────────────────────────────────────────────────

interface ChatState {
  items: DisplayItem[];
  steps: StepInfo[];
  totalCostUsd: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  runStartTime: number | null;
  isRunning: boolean;
  sessions: SessionSummary[];
  activeSessionId: string | null;
  showSessions: boolean;
  /** Active pipeline path and display name, or null for passthrough mode. */
  activePipeline: { path: string; displayName: string } | null;
  /** Counter to generate unique IDs for display items. */
  _idCounter: number;
  /** The step ID currently being processed — set on stepStarted, cleared on completion. */
  currentStepId: string | null;
}

const initialState: ChatState = {
  items: [],
  steps: [],
  totalCostUsd: 0,
  totalInputTokens: 0,
  totalOutputTokens: 0,
  runStartTime: null,
  isRunning: false,
  sessions: [],
  activeSessionId: null,
  showSessions: false,
  activePipeline: null,
  _idCounter: 0,
  currentStepId: null,
};

// ── Actions ────────────────────────────────────────────────────────────────────

type Action =
  | { type: 'HOST_MSG'; msg: HostToWebviewMessage }
  | { type: 'USER_SUBMIT'; text: string }
  | { type: 'HITL_APPROVE'; stepId: string; text?: string }
  | { type: 'HITL_REJECT'; stepId: string }
  | { type: 'PERMISSION_ALLOW' }
  | { type: 'PERMISSION_DENY' }
  | { type: 'ASK_USER_SUBMIT'; answer: string }
  | { type: 'ASK_USER_DENY' }
  | { type: 'STOP' }
  | { type: 'SELECT_SESSION'; id: string }
  | { type: 'NEW_SESSION' }
  | { type: 'TOGGLE_SESSIONS' }
  | { type: 'LOAD_PIPELINE' };

function nextId(state: ChatState): [string, ChatState] {
  const id = String(state._idCounter);
  return [id, { ...state, _idCounter: state._idCounter + 1 }];
}

function updateItem(items: DisplayItem[], id: string, updater: (item: DisplayItem) => DisplayItem): DisplayItem[] {
  return items.map((it) => it.id === id ? updater(it) : it);
}

/** Find the last assistant-stream item id that is still actively streaming. */
function lastStreamId(items: DisplayItem[], stepId?: string | null): string | null {
  for (let i = items.length - 1; i >= 0; i--) {
    const item = items[i];
    if (item.kind === 'assistant-stream' && item.streaming) {
      if (stepId && item.stepId && item.stepId !== stepId) continue;
      return item.id;
    }
  }
  return null;
}

/** Find pending HITL card id by stepId. */
function findHitlId(items: DisplayItem[], stepId: string): string | null {
  for (const it of items) {
    if (it.kind === 'hitl' && it.stepId === stepId) return it.id;
  }
  return null;
}

/** Find pending permission card id. */
function findPermissionId(items: DisplayItem[]): string | null {
  for (const it of items) {
    if (it.kind === 'permission' && it.cardState === 'pending') return it.id;
  }
  return null;
}

/** Find pending ask-user-question card id. */
function findAskUserId(items: DisplayItem[]): string | null {
  for (const it of items) {
    if (it.kind === 'ask-user-question' && it.cardState === 'pending') return it.id;
  }
  return null;
}

/** Find tool call by toolUseId. */
function findToolCallId(items: DisplayItem[], toolUseId: string): string | null {
  for (const it of items) {
    if (it.kind === 'tool-call' && it.data.toolUseId === toolUseId) return it.id;
  }
  return null;
}

function updateStep(steps: StepInfo[], stepId: string, updates: Partial<StepInfo>): StepInfo[] {
  return steps.map((s) => s.stepId === stepId ? { ...s, ...updates } : s);
}

function reducer(state: ChatState, action: Action): ChatState {
  switch (action.type) {
    case 'USER_SUBMIT': {
      const [id, s2] = nextId(state);
      return {
        ...s2,
        isRunning: true,
        items: [...s2.items, { kind: 'user-message', id, text: action.text }],
      };
    }

    case 'HOST_MSG': {
      const msg = action.msg;
      switch (msg.type) {
        case 'runStarted': {
          return { ...state, isRunning: true, steps: [], totalCostUsd: 0, totalInputTokens: 0, totalOutputTokens: 0, runStartTime: Date.now() };
        }

        case 'stepStarted': {
          const existing = state.steps.find((s) => s.stepId === msg.stepId);
          if (existing) {
            return { ...state, currentStepId: msg.stepId, steps: updateStep(state.steps, msg.stepId, { status: 'running' }) };
          }
          return {
            ...state,
            currentStepId: msg.stepId,
            steps: [...state.steps, { stepId: msg.stepId, status: 'running' }],
          };
        }

        case 'streamDelta': {
          const streamId = lastStreamId(state.items, state.currentStepId);
          if (streamId !== null) {
            return {
              ...state,
              items: updateItem(state.items, streamId, (it) =>
                it.kind === 'assistant-stream'
                  ? { ...it, text: it.text + msg.text }
                  : it
              ),
            };
          }
          // Start a new stream item
          const [id, s2] = nextId(state);
          return {
            ...s2,
            items: [...s2.items, { kind: 'assistant-stream', id, text: msg.text, streaming: true, stepId: state.currentStepId ?? undefined }],
          };
        }

        case 'thinking': {
          const [id, s2] = nextId(state);
          return {
            ...s2,
            items: [...s2.items, { kind: 'thinking', id, text: msg.text }],
          };
        }

        case 'toolUse': {
          const [id, s2] = nextId(state);
          return {
            ...s2,
            items: [...s2.items, {
              kind: 'tool-call',
              id,
              data: {
                toolUseId: msg.toolUseId,
                toolName: msg.toolName,
                input: msg.input,
              },
            }],
          };
        }

        case 'toolResult': {
          const tcId = findToolCallId(state.items, msg.toolUseId);
          if (tcId !== null) {
            return {
              ...state,
              items: updateItem(state.items, tcId, (it) =>
                it.kind === 'tool-call'
                  ? { ...it, data: { ...it.data, result: msg.content, isError: msg.isError } }
                  : it
              ),
            };
          }
          return state;
        }

        case 'stepCompleted': {
          const costDelta = msg.costUsd ?? 0;
          const hasActiveStream = state.items.some((it) => it.kind === 'assistant-stream' && it.streaming);
          const closedItems = state.items.map((it) =>
            it.kind === 'assistant-stream' && it.streaming ? { ...it, streaming: false } : it
          );
          // Fallback: if no streaming events arrived but the step has a response, add it now.
          let finalState: ChatState = state;
          let finalItems: DisplayItem[] = closedItems;
          if (!hasActiveStream && msg.response) {
            const [id, s2] = nextId(state);
            finalState = s2;
            finalItems = [...closedItems, { kind: 'assistant-stream' as const, id, text: msg.response, streaming: false }];
          }
          return {
            ...finalState,
            totalCostUsd: state.totalCostUsd + costDelta,
            totalInputTokens: state.totalInputTokens + (msg.inputTokens ?? 0),
            totalOutputTokens: state.totalOutputTokens + (msg.outputTokens ?? 0),
            steps: updateStep(state.steps, msg.stepId, { status: 'completed', costUsd: msg.costUsd ?? undefined }),
            items: finalItems,
          };
        }

        case 'stepSkipped':
          return { ...state, steps: updateStep(state.steps, msg.stepId, { status: 'skipped' }) };

        case 'stepFailed': {
          const [id, s2] = nextId(state);
          return {
            ...s2,
            steps: updateStep(s2.steps, msg.stepId, { status: 'failed' }),
            items: [...s2.items, { kind: 'error', id, message: `Step '${msg.stepId}' failed: ${msg.error}` }],
          };
        }

        case 'hitlGate': {
          // Cancel any streaming in progress
          const itemsWithStoppedStream = state.items.map((it) =>
            it.kind === 'assistant-stream' && it.streaming ? { ...it, streaming: false } : it
          );
          const [id, s2] = nextId({ ...state, items: itemsWithStoppedStream });
          return {
            ...s2,
            items: [...s2.items, {
              kind: 'hitl',
              id,
              stepId: msg.stepId,
              message: msg.message,
              cardState: 'pending',
            }],
          };
        }

        case 'permissionRequested': {
          // Detect AskUserQuestion tool by displayName
          if (msg.displayName === 'AskUserQuestion' && msg.toolInput != null) {
            const input = msg.toolInput as {
              questions?: AskUserQuestion[];
              question?: string;
              options?: (AskUserQuestion['options'][number] | string)[];
            };
            // Support both the canonical {questions:[...]} format and the flat {question, options} format
            let questions: AskUserQuestion[] | null = null;
            if (input.questions && Array.isArray(input.questions) && input.questions.length > 0) {
              questions = input.questions;
            } else if (typeof input.question === 'string') {
              questions = [{
                header: 'Question',
                question: input.question,
                multiSelect: false,
                options: (input.options ?? []) as AskUserQuestion['options'],
              }];
            }
            if (questions !== null) {
              const [id, s2] = nextId(state);
              return {
                ...s2,
                items: [...s2.items, {
                  kind: 'ask-user-question',
                  id,
                  questions,
                  cardState: 'pending' as AskUserCardState,
                }],
              };
            }
          }
          const [id, s2] = nextId(state);
          return {
            ...s2,
            items: [...s2.items, {
              kind: 'permission',
              id,
              displayName: msg.displayName,
              displayDetail: msg.displayDetail,
              cardState: 'pending',
            }],
          };
        }

        case 'pipelineCompleted': {
          // Cancel any pending HITL/permission/ask-user cards
          const cancelledItems = state.items.map((it) => {
            if (it.kind === 'hitl' && it.cardState === 'pending') {
              return { ...it, cardState: 'cancelled' as HitlCardState };
            }
            if (it.kind === 'permission' && it.cardState === 'pending') {
              return { ...it, cardState: 'resolved' as PermissionCardState };
            }
            if (it.kind === 'ask-user-question' && it.cardState === 'pending') {
              return { ...it, cardState: 'resolved' as AskUserCardState };
            }
            if (it.kind === 'assistant-stream' && it.streaming) {
              return { ...it, streaming: false };
            }
            return it;
          });
          return { ...state, isRunning: false, runStartTime: null, currentStepId: null, items: cancelledItems };
        }

        case 'pipelineError':
        case 'processError': {
          const [id, s2] = nextId(state);
          const errorMsg = msg.type === 'pipelineError' ? msg.error : msg.message;
          return {
            ...s2,
            isRunning: false,
            runStartTime: null,
            currentStepId: null,
            items: [...s2.items.map((it) =>
              it.kind === 'assistant-stream' && it.streaming ? { ...it, streaming: false } : it
            ), { kind: 'error', id, message: errorMsg }],
          };
        }

        case 'sessionsUpdated':
          return { ...state, sessions: msg.sessions };

        case 'pipelineChanged':
          return {
            ...state,
            activePipeline: msg.path ? { path: msg.path, displayName: msg.displayName ?? msg.path } : null,
          };

        default:
          return state;
      }
    }

    case 'HITL_APPROVE': {
      const hitlId = findHitlId(state.items, action.stepId);
      if (hitlId === null) return state;
      return {
        ...state,
        items: updateItem(state.items, hitlId, (it) =>
          it.kind === 'hitl' ? { ...it, cardState: 'resolved', resolvedText: action.text ?? 'Approved' } : it
        ),
      };
    }

    case 'HITL_REJECT': {
      const hitlId = findHitlId(state.items, action.stepId);
      if (hitlId === null) return state;
      return {
        ...state,
        isRunning: false,
        items: updateItem(state.items, hitlId, (it) =>
          it.kind === 'hitl' ? { ...it, cardState: 'resolved', resolvedText: 'Rejected' } : it
        ),
      };
    }

    case 'PERMISSION_ALLOW': {
      const pId = findPermissionId(state.items);
      if (pId === null) return state;
      return {
        ...state,
        items: updateItem(state.items, pId, (it) =>
          it.kind === 'permission' ? { ...it, cardState: 'resolved', resolvedAllowed: true } : it
        ),
      };
    }

    case 'PERMISSION_DENY': {
      const pId = findPermissionId(state.items);
      if (pId === null) return state;
      return {
        ...state,
        items: updateItem(state.items, pId, (it) =>
          it.kind === 'permission' ? { ...it, cardState: 'resolved', resolvedAllowed: false } : it
        ),
      };
    }

    case 'ASK_USER_SUBMIT': {
      const auId = findAskUserId(state.items);
      if (auId === null) return state;
      return {
        ...state,
        items: updateItem(state.items, auId, (it) =>
          it.kind === 'ask-user-question' ? { ...it, cardState: 'resolved', resolvedAnswer: action.answer } : it
        ),
      };
    }

    case 'ASK_USER_DENY': {
      const auId = findAskUserId(state.items);
      if (auId === null) return state;
      return {
        ...state,
        items: updateItem(state.items, auId, (it) =>
          it.kind === 'ask-user-question' ? { ...it, cardState: 'resolved' } : it
        ),
      };
    }

    case 'STOP':
      return { ...state, isRunning: false, runStartTime: null };

    case 'SELECT_SESSION':
      return { ...state, activeSessionId: action.id };

    case 'NEW_SESSION':
      return { ...state, items: [], steps: [], totalCostUsd: 0, totalInputTokens: 0, totalOutputTokens: 0, runStartTime: null, isRunning: false, activeSessionId: null, currentStepId: null };

    case 'TOGGLE_SESSIONS':
      return { ...state, showSessions: !state.showSessions };

    case 'LOAD_PIPELINE':
      return state; // Side-effected in the component via postToHost

    default:
      return state;
  }
}

// ── App component ──────────────────────────────────────────────────────────────

export const App: React.FC = () => {
  const [state, dispatch] = useReducer(reducer, initialState);
  const messageListRef = useRef<HTMLDivElement>(null);

  // Listen for messages from the extension host
  useEffect(() => {
    const handler = (event: MessageEvent) => {
      const msg = event.data as HostToWebviewMessage;
      dispatch({ type: 'HOST_MSG', msg });
    };
    window.addEventListener('message', handler);
    // Signal readiness to the host
    postToHost({ type: 'ready' });
    return () => window.removeEventListener('message', handler);
  }, []);

  // Auto-scroll when new items arrive
  useEffect(() => {
    if (messageListRef.current) {
      messageListRef.current.scrollTop = messageListRef.current.scrollHeight;
    }
  }, [state.items]);

  const handleSubmit = (text: string) => {
    dispatch({ type: 'USER_SUBMIT', text });
    postToHost({ type: 'submitPrompt', text });
  };

  const handleStop = () => {
    dispatch({ type: 'STOP' });
    postToHost({ type: 'killProcess' });
  };

  const handleHitlApprove = (stepId: string) => {
    dispatch({ type: 'HITL_APPROVE', stepId });
    postToHost({ type: 'hitlResponse', stepId, text: 'Approved' });
  };

  const handleHitlReject = (stepId: string) => {
    dispatch({ type: 'HITL_REJECT', stepId });
    postToHost({ type: 'hitlResponse', stepId, text: 'Rejected' });
  };

  const handlePermissionAllow = () => {
    dispatch({ type: 'PERMISSION_ALLOW' });
    postToHost({ type: 'permissionResponse', allowed: true });
  };

  const handlePermissionDeny = () => {
    dispatch({ type: 'PERMISSION_DENY' });
    postToHost({ type: 'permissionResponse', allowed: false });
  };

  const handleAskUserSubmit = (answer: string) => {
    dispatch({ type: 'ASK_USER_SUBMIT', answer });
    postToHost({ type: 'permissionResponse', allowed: false, reason: answer });
  };

  const handleAskUserDeny = () => {
    dispatch({ type: 'ASK_USER_DENY' });
    postToHost({ type: 'permissionResponse', allowed: false, reason: 'User dismissed the question' });
  };

  const handleSelectSession = (id: string) => {
    dispatch({ type: 'SELECT_SESSION', id });
    postToHost({ type: 'switchSession', sessionId: id });
  };

  const handleNewSession = () => {
    dispatch({ type: 'NEW_SESSION' });
    postToHost({ type: 'newSession' });
  };

  const handleLoadPipeline = () => {
    dispatch({ type: 'LOAD_PIPELINE' });
    postToHost({ type: 'loadPipeline' });
  };

  const hasPendingHitl = state.items.some((it) => it.kind === 'hitl' && it.cardState === 'pending');
  const hasPendingPermission = state.items.some((it) => it.kind === 'permission' && it.cardState === 'pending');
  const hasPendingAskUser = state.items.some((it) => it.kind === 'ask-user-question' && it.cardState === 'pending');
  const inputDisabled = hasPendingHitl || hasPendingPermission || hasPendingAskUser;

  return (
    <>
      {state.showSessions && (
        <SessionList
          sessions={state.sessions}
          activeSessionId={state.activeSessionId}
          onSelectSession={handleSelectSession}
          onNewSession={handleNewSession}
        />
      )}
      <div className="chat-panel">
        <PipelineBar
          displayName={state.activePipeline?.displayName ?? null}
          onLoad={handleLoadPipeline}
        />
        {state.steps.length > 0 && (
          <StepProgress
            steps={state.steps}
            totalCostUsd={!state.isRunning ? state.totalCostUsd : undefined}
          />
        )}
        <div className="message-list" ref={messageListRef}>
          {state.items.length === 0 && (
            <div className="empty-state">
              <div className="empty-state-title">What would you like to build?</div>
              <div className="empty-state-subtitle">Describe a task to get started with your ail pipeline.</div>
            </div>
          )}
          {state.items.map((item) => {
            switch (item.kind) {
              case 'user-message':
                return <ChatMessage key={item.id} role="user" content={item.text} />;
              case 'assistant-stream':
                return <ChatMessage key={item.id} role="assistant" content={item.text} streaming={item.streaming} />;
              case 'thinking':
                return <ThinkingBlock key={item.id} text={item.text} />;
              case 'tool-call':
                return <ToolCallCard key={item.id} data={item.data} />;
              case 'hitl':
                return (
                  <HitlCard
                    key={item.id}
                    stepId={item.stepId}
                    message={item.message}
                    cardState={item.cardState}
                    resolvedText={item.resolvedText}
                    onApprove={handleHitlApprove}
                    onReject={handleHitlReject}
                  />
                );
              case 'permission':
                return (
                  <PermissionCard
                    key={item.id}
                    displayName={item.displayName}
                    displayDetail={item.displayDetail}
                    cardState={item.cardState}
                    resolvedAllowed={item.resolvedAllowed}
                    onAllow={handlePermissionAllow}
                    onDeny={handlePermissionDeny}
                  />
                );
              case 'ask-user-question':
                return (
                  <ErrorBoundary key={item.id}>
                    <AskUserQuestionCard
                      questions={item.questions}
                      cardState={item.cardState}
                      resolvedAnswer={item.resolvedAnswer}
                      onSubmit={handleAskUserSubmit}
                      onDeny={handleAskUserDeny}
                    />
                  </ErrorBoundary>
                );
              case 'error':
                return (
                  <div key={item.id} className="error-message">
                    <span className="error-message-icon codicon codicon-error" />
                    <span>{item.message}</span>
                  </div>
                );
            }
          })}
        </div>
        <StatusBar
          isRunning={state.isRunning}
          startTime={state.runStartTime}
          totalTokens={state.totalInputTokens + state.totalOutputTokens}
          onStop={handleStop}
        />
        <ChatInput
          onSubmit={handleSubmit}
          onStop={handleStop}
          isRunning={state.isRunning}
          disabled={inputDisabled}
          placeholder={
            hasPendingHitl ? 'Waiting for your review…'
            : hasPendingPermission ? 'Waiting for permission response…'
            : hasPendingAskUser ? 'Waiting for your answer…'
            : undefined
          }
        />
      </div>
    </>
  );
};
