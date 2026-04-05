"use strict";
/**
 * App — root React component for the ail Chat webview.
 *
 * Handles postMessage from the extension host (HostToWebviewMessage) and
 * accumulates all chat state: messages, tool calls, thinking blocks,
 * HITL gates, permission requests, and step progress.
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.App = void 0;
const react_1 = __importStar(require("react"));
const ChatMessage_1 = require("./components/ChatMessage");
const ThinkingBlock_1 = require("./components/ThinkingBlock");
const ToolCallCard_1 = require("./components/ToolCallCard");
const HitlCard_1 = require("./components/HitlCard");
const PermissionCard_1 = require("./components/PermissionCard");
const StepProgress_1 = require("./components/StepProgress");
const ChatInput_1 = require("./components/ChatInput");
const SessionList_1 = require("./components/SessionList");
// Acquire once; do not call acquireVsCodeApi() more than once per session.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const vscode = (typeof acquireVsCodeApi !== 'undefined' ? acquireVsCodeApi() : null);
function postToHost(msg) {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-member-access
    vscode?.postMessage(msg);
}
const initialState = {
    items: [],
    steps: [],
    totalCostUsd: 0,
    isRunning: false,
    sessions: [],
    activeSessionId: null,
    showSessions: false,
    _idCounter: 0,
};
function nextId(state) {
    const id = String(state._idCounter);
    return [id, { ...state, _idCounter: state._idCounter + 1 }];
}
function updateItem(items, id, updater) {
    return items.map((it) => it.id === id ? updater(it) : it);
}
/** Find the last assistant-stream item id. */
function lastStreamId(items) {
    for (let i = items.length - 1; i >= 0; i--) {
        if (items[i].kind === 'assistant-stream')
            return items[i].id;
    }
    return null;
}
/** Find pending HITL card id by stepId. */
function findHitlId(items, stepId) {
    for (const it of items) {
        if (it.kind === 'hitl' && it.stepId === stepId)
            return it.id;
    }
    return null;
}
/** Find pending permission card id. */
function findPermissionId(items) {
    for (const it of items) {
        if (it.kind === 'permission' && it.cardState === 'pending')
            return it.id;
    }
    return null;
}
/** Find tool call by toolUseId. */
function findToolCallId(items, toolUseId) {
    for (const it of items) {
        if (it.kind === 'tool-call' && it.data.toolUseId === toolUseId)
            return it.id;
    }
    return null;
}
function updateStep(steps, stepId, updates) {
    return steps.map((s) => s.stepId === stepId ? { ...s, ...updates } : s);
}
function reducer(state, action) {
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
                    return { ...state, isRunning: true, steps: [], totalCostUsd: 0 };
                }
                case 'stepStarted': {
                    const existing = state.steps.find((s) => s.stepId === msg.stepId);
                    if (existing) {
                        return { ...state, steps: updateStep(state.steps, msg.stepId, { status: 'running' }) };
                    }
                    return {
                        ...state,
                        steps: [...state.steps, { stepId: msg.stepId, status: 'running' }],
                    };
                }
                case 'streamDelta': {
                    const streamId = lastStreamId(state.items);
                    if (streamId !== null) {
                        return {
                            ...state,
                            items: updateItem(state.items, streamId, (it) => it.kind === 'assistant-stream'
                                ? { ...it, text: it.text + msg.text }
                                : it),
                        };
                    }
                    // Start a new stream item
                    const [id, s2] = nextId(state);
                    return {
                        ...s2,
                        items: [...s2.items, { kind: 'assistant-stream', id, text: msg.text, streaming: true }],
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
                            items: updateItem(state.items, tcId, (it) => it.kind === 'tool-call'
                                ? { ...it, data: { ...it.data, result: msg.content, isError: msg.isError } }
                                : it),
                        };
                    }
                    return state;
                }
                case 'stepCompleted': {
                    const costDelta = msg.costUsd ?? 0;
                    return {
                        ...state,
                        totalCostUsd: state.totalCostUsd + costDelta,
                        steps: updateStep(state.steps, msg.stepId, { status: 'completed', costUsd: msg.costUsd ?? undefined }),
                        // Mark any pending stream item as no longer streaming
                        items: state.items.map((it) => it.kind === 'assistant-stream' && it.streaming ? { ...it, streaming: false } : it),
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
                    const itemsWithStoppedStream = state.items.map((it) => it.kind === 'assistant-stream' && it.streaming ? { ...it, streaming: false } : it);
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
                    // Cancel any pending HITL/permission cards
                    const cancelledItems = state.items.map((it) => {
                        if (it.kind === 'hitl' && it.cardState === 'pending') {
                            return { ...it, cardState: 'cancelled' };
                        }
                        if (it.kind === 'permission' && it.cardState === 'pending') {
                            return { ...it, cardState: 'resolved' };
                        }
                        if (it.kind === 'assistant-stream' && it.streaming) {
                            return { ...it, streaming: false };
                        }
                        return it;
                    });
                    return { ...state, isRunning: false, items: cancelledItems };
                }
                case 'pipelineError':
                case 'processError': {
                    const [id, s2] = nextId(state);
                    const errorMsg = msg.type === 'pipelineError' ? msg.error : msg.message;
                    return {
                        ...s2,
                        isRunning: false,
                        items: [...s2.items.map((it) => it.kind === 'assistant-stream' && it.streaming ? { ...it, streaming: false } : it), { kind: 'error', id, message: errorMsg }],
                    };
                }
                case 'sessionsUpdated':
                    return { ...state, sessions: msg.sessions };
                default:
                    return state;
            }
        }
        case 'HITL_APPROVE': {
            const hitlId = findHitlId(state.items, action.stepId);
            if (hitlId === null)
                return state;
            return {
                ...state,
                items: updateItem(state.items, hitlId, (it) => it.kind === 'hitl' ? { ...it, cardState: 'resolved', resolvedText: action.text ?? 'Approved' } : it),
            };
        }
        case 'HITL_REJECT': {
            const hitlId = findHitlId(state.items, action.stepId);
            if (hitlId === null)
                return state;
            return {
                ...state,
                isRunning: false,
                items: updateItem(state.items, hitlId, (it) => it.kind === 'hitl' ? { ...it, cardState: 'resolved', resolvedText: 'Rejected' } : it),
            };
        }
        case 'PERMISSION_ALLOW': {
            const pId = findPermissionId(state.items);
            if (pId === null)
                return state;
            return {
                ...state,
                items: updateItem(state.items, pId, (it) => it.kind === 'permission' ? { ...it, cardState: 'resolved', resolvedAllowed: true } : it),
            };
        }
        case 'PERMISSION_DENY': {
            const pId = findPermissionId(state.items);
            if (pId === null)
                return state;
            return {
                ...state,
                items: updateItem(state.items, pId, (it) => it.kind === 'permission' ? { ...it, cardState: 'resolved', resolvedAllowed: false } : it),
            };
        }
        case 'STOP':
            return { ...state, isRunning: false };
        case 'SELECT_SESSION':
            return { ...state, activeSessionId: action.id };
        case 'NEW_SESSION':
            return { ...state, items: [], steps: [], totalCostUsd: 0, isRunning: false, activeSessionId: null };
        case 'TOGGLE_SESSIONS':
            return { ...state, showSessions: !state.showSessions };
        default:
            return state;
    }
}
// ── App component ──────────────────────────────────────────────────────────────
const App = () => {
    const [state, dispatch] = (0, react_1.useReducer)(reducer, initialState);
    const messageListRef = (0, react_1.useRef)(null);
    // Listen for messages from the extension host
    (0, react_1.useEffect)(() => {
        const handler = (event) => {
            const msg = event.data;
            dispatch({ type: 'HOST_MSG', msg });
        };
        window.addEventListener('message', handler);
        // Signal readiness to the host
        postToHost({ type: 'ready' });
        return () => window.removeEventListener('message', handler);
    }, []);
    // Auto-scroll when new items arrive
    (0, react_1.useEffect)(() => {
        if (messageListRef.current) {
            messageListRef.current.scrollTop = messageListRef.current.scrollHeight;
        }
    }, [state.items]);
    const handleSubmit = (text) => {
        dispatch({ type: 'USER_SUBMIT', text });
        postToHost({ type: 'submitPrompt', text });
    };
    const handleStop = () => {
        dispatch({ type: 'STOP' });
        postToHost({ type: 'killProcess' });
    };
    const handleHitlApprove = (stepId) => {
        dispatch({ type: 'HITL_APPROVE', stepId });
        postToHost({ type: 'hitlResponse', stepId, text: 'Approved' });
    };
    const handleHitlReject = (stepId) => {
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
    const handleSelectSession = (id) => {
        dispatch({ type: 'SELECT_SESSION', id });
        postToHost({ type: 'switchSession', sessionId: id });
    };
    const handleNewSession = () => {
        dispatch({ type: 'NEW_SESSION' });
        postToHost({ type: 'newSession' });
    };
    const hasPendingHitl = state.items.some((it) => it.kind === 'hitl' && it.cardState === 'pending');
    const hasPendingPermission = state.items.some((it) => it.kind === 'permission' && it.cardState === 'pending');
    const inputDisabled = hasPendingHitl || hasPendingPermission;
    return (react_1.default.createElement(react_1.default.Fragment, null,
        state.showSessions && (react_1.default.createElement(SessionList_1.SessionList, { sessions: state.sessions, activeSessionId: state.activeSessionId, onSelectSession: handleSelectSession, onNewSession: handleNewSession })),
        react_1.default.createElement("div", { className: "chat-panel" },
            state.steps.length > 0 && (react_1.default.createElement(StepProgress_1.StepProgress, { steps: state.steps, totalCostUsd: !state.isRunning ? state.totalCostUsd : undefined })),
            react_1.default.createElement("div", { className: "message-list", ref: messageListRef },
                state.items.map((item) => {
                    switch (item.kind) {
                        case 'user-message':
                            return react_1.default.createElement(ChatMessage_1.ChatMessage, { key: item.id, role: "user", content: item.text });
                        case 'assistant-stream':
                            return react_1.default.createElement(ChatMessage_1.ChatMessage, { key: item.id, role: "assistant", content: item.text, streaming: item.streaming });
                        case 'thinking':
                            return react_1.default.createElement(ThinkingBlock_1.ThinkingBlock, { key: item.id, text: item.text });
                        case 'tool-call':
                            return react_1.default.createElement(ToolCallCard_1.ToolCallCard, { key: item.id, data: item.data });
                        case 'hitl':
                            return (react_1.default.createElement(HitlCard_1.HitlCard, { key: item.id, stepId: item.stepId, message: item.message, cardState: item.cardState, resolvedText: item.resolvedText, onApprove: handleHitlApprove, onReject: handleHitlReject }));
                        case 'permission':
                            return (react_1.default.createElement(PermissionCard_1.PermissionCard, { key: item.id, displayName: item.displayName, displayDetail: item.displayDetail, cardState: item.cardState, resolvedAllowed: item.resolvedAllowed, onAllow: handlePermissionAllow, onDeny: handlePermissionDeny }));
                        case 'error':
                            return react_1.default.createElement("div", { key: item.id, className: "error-message" }, item.message);
                    }
                }),
                state.items.length === 0 && (react_1.default.createElement("div", { className: "status-banner" }, "Send a prompt to get started."))),
            react_1.default.createElement(ChatInput_1.ChatInput, { onSubmit: handleSubmit, onStop: handleStop, isRunning: state.isRunning, disabled: inputDisabled, placeholder: hasPendingHitl ? 'Waiting for your review…'
                    : hasPendingPermission ? 'Waiting for permission response…'
                        : undefined }))));
};
exports.App = App;
//# sourceMappingURL=App.js.map