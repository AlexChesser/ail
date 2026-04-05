"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.ChatMessage = void 0;
const react_1 = __importDefault(require("react"));
const ChatMessage = ({ role, content, streaming }) => {
    return (react_1.default.createElement("div", { className: `chat-message ${role}` },
        react_1.default.createElement("div", { className: "chat-message-role" }, role === 'user' ? 'You' : 'ail'),
        react_1.default.createElement("div", { className: `chat-message-content${streaming ? ' streaming' : ''}` }, content)));
};
exports.ChatMessage = ChatMessage;
//# sourceMappingURL=ChatMessage.js.map