"use strict";
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
exports.ChatInput = void 0;
const react_1 = __importStar(require("react"));
const ChatInput = ({ onSubmit, onStop, isRunning, disabled, placeholder, }) => {
    const textareaRef = (0, react_1.useRef)(null);
    const handleKeyDown = (e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            const value = textareaRef.current?.value.trim() ?? '';
            if (value && !isRunning && !disabled) {
                if (textareaRef.current)
                    textareaRef.current.value = '';
                onSubmit(value);
            }
        }
    };
    const handleSendClick = () => {
        const value = textareaRef.current?.value.trim() ?? '';
        if (value && !isRunning && !disabled) {
            if (textareaRef.current)
                textareaRef.current.value = '';
            onSubmit(value);
        }
    };
    return (react_1.default.createElement("div", { className: "chat-input-area" },
        react_1.default.createElement("div", { className: "chat-input-row" },
            react_1.default.createElement("textarea", { ref: textareaRef, className: "chat-input-textarea", placeholder: placeholder ?? (isRunning ? 'Running…' : 'Send a prompt (Enter to send, Shift+Enter for newline)'), disabled: isRunning || disabled, onKeyDown: handleKeyDown, rows: 2 }),
            isRunning ? (react_1.default.createElement("button", { className: "btn-danger", onClick: onStop, title: "Stop" }, "\u25A0 Stop")) : (react_1.default.createElement("button", { className: "btn-primary", onClick: handleSendClick, disabled: disabled }, "Send")))));
};
exports.ChatInput = ChatInput;
//# sourceMappingURL=ChatInput.js.map