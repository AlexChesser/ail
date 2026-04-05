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
exports.HitlCard = void 0;
const react_1 = __importStar(require("react"));
const HitlCard = ({ stepId, message, cardState, resolvedText, onApprove, onReject, }) => {
    const [modifyText, setModifyText] = (0, react_1.useState)('');
    const [showModify, setShowModify] = (0, react_1.useState)(false);
    const handleApprove = () => {
        onApprove(stepId);
    };
    const handleReject = () => {
        onReject(stepId);
    };
    const handleModifySubmit = () => {
        if (modifyText.trim()) {
            onApprove(stepId);
        }
    };
    return (react_1.default.createElement("div", { className: "hitl-card" },
        react_1.default.createElement("div", { className: "hitl-card-title" },
            react_1.default.createElement("span", null, "\u23F8"),
            react_1.default.createElement("span", null, "Pipeline paused \u2014 human review required")),
        message && react_1.default.createElement("div", { className: "hitl-card-message" }, message),
        cardState === 'pending' && (react_1.default.createElement(react_1.default.Fragment, null, showModify ? (react_1.default.createElement("div", { style: { display: 'flex', flexDirection: 'column', gap: 6 } },
            react_1.default.createElement("textarea", { className: "chat-input-textarea", value: modifyText, onChange: (e) => setModifyText(e.target.value), placeholder: "Type your modified instruction\u2026", rows: 3, style: { width: '100%' } }),
            react_1.default.createElement("div", { className: "hitl-card-actions" },
                react_1.default.createElement("button", { className: "btn-primary", onClick: handleModifySubmit, disabled: !modifyText.trim() }, "Submit"),
                react_1.default.createElement("button", { className: "btn-secondary", onClick: () => { setShowModify(false); setModifyText(''); } }, "Cancel")))) : (react_1.default.createElement("div", { className: "hitl-card-actions" },
            react_1.default.createElement("button", { className: "btn-primary", onClick: handleApprove }, "Approve"),
            react_1.default.createElement("button", { className: "btn-secondary", onClick: () => setShowModify(true) }, "Modify"),
            react_1.default.createElement("button", { className: "btn-danger", onClick: handleReject }, "Reject"))))),
        cardState === 'resolved' && (react_1.default.createElement("div", { className: "hitl-card-resolved" },
            "\u2713 ",
            resolvedText ?? 'Approved')),
        cardState === 'cancelled' && (react_1.default.createElement("div", { className: "hitl-card-resolved" }, "Pipeline ended before response"))));
};
exports.HitlCard = HitlCard;
//# sourceMappingURL=HitlCard.js.map