"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.SessionList = void 0;
const react_1 = __importDefault(require("react"));
const SessionList = ({ sessions, activeSessionId, onSelectSession, onNewSession, }) => {
    return (react_1.default.createElement("div", { className: "sessions-panel" },
        react_1.default.createElement("div", { className: "session-list-header" },
            react_1.default.createElement("span", null, "Sessions"),
            react_1.default.createElement("button", { className: "btn-icon", onClick: onNewSession, title: "New session" }, "+")),
        react_1.default.createElement("div", { className: "session-list-items" },
            sessions.map((s) => (react_1.default.createElement("div", { key: s.id, className: `session-item${s.id === activeSessionId ? ' active' : ''}`, onClick: () => onSelectSession(s.id), role: "button", tabIndex: 0, onKeyDown: (e) => e.key === 'Enter' && onSelectSession(s.id) },
                react_1.default.createElement("div", null, s.title || '(untitled)'),
                react_1.default.createElement("div", { className: "session-item-date" }, new Date(s.timestamp).toLocaleString(undefined, {
                    month: 'short',
                    day: 'numeric',
                    hour: '2-digit',
                    minute: '2-digit',
                }))))),
            sessions.length === 0 && (react_1.default.createElement("div", { style: { padding: '8px 10px', fontSize: 11, opacity: 0.6 } }, "No sessions yet")))));
};
exports.SessionList = SessionList;
//# sourceMappingURL=SessionList.js.map