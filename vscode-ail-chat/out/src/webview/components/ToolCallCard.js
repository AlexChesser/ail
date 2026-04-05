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
exports.ToolCallCard = void 0;
const react_1 = __importStar(require("react"));
const ToolCallCard = ({ data }) => {
    const [collapsed, setCollapsed] = (0, react_1.useState)(true);
    const hasResult = data.result !== undefined;
    const inputStr = data.input != null
        ? JSON.stringify(data.input, null, 2)
        : '';
    const statusLabel = hasResult
        ? (data.isError ? 'error' : 'done')
        : 'pending';
    return (react_1.default.createElement("div", { className: "tool-card" },
        react_1.default.createElement("div", { className: "tool-card-header", onClick: () => setCollapsed((c) => !c), role: "button", tabIndex: 0, onKeyDown: (e) => e.key === 'Enter' && setCollapsed((c) => !c), "aria-expanded": !collapsed },
            react_1.default.createElement("span", null, collapsed ? '▶' : '▼'),
            react_1.default.createElement("span", { className: "tool-card-name" }, data.toolName),
            react_1.default.createElement("span", { className: `tool-card-status${data.isError ? ' error' : ''}` }, statusLabel)),
        react_1.default.createElement("div", { className: `tool-card-body${collapsed ? ' collapsed' : ''}` },
            inputStr && (react_1.default.createElement(react_1.default.Fragment, null,
                react_1.default.createElement("div", { className: "tool-card-section-label" }, "Input"),
                react_1.default.createElement("pre", { className: "tool-card-code" }, inputStr))),
            hasResult && (react_1.default.createElement(react_1.default.Fragment, null,
                react_1.default.createElement("div", { className: "tool-card-section-label" }, "Result"),
                react_1.default.createElement("pre", { className: `tool-card-code${data.isError ? ' error' : ''}` }, data.result))))));
};
exports.ToolCallCard = ToolCallCard;
//# sourceMappingURL=ToolCallCard.js.map