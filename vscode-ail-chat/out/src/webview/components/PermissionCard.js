"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.PermissionCard = void 0;
const react_1 = __importDefault(require("react"));
const PermissionCard = ({ displayName, displayDetail, cardState, resolvedAllowed, onAllow, onDeny, }) => {
    return (react_1.default.createElement("div", { className: "permission-card" },
        react_1.default.createElement("div", { className: "permission-card-title" },
            "\uD83D\uDD12 Permission requested: ",
            displayName),
        react_1.default.createElement("div", { className: "permission-card-detail" }, displayDetail),
        cardState === 'pending' && (react_1.default.createElement("div", { className: "permission-card-actions" },
            react_1.default.createElement("button", { className: "btn-primary", onClick: onAllow }, "Allow"),
            react_1.default.createElement("button", { className: "btn-danger", onClick: onDeny }, "Deny"))),
        cardState === 'resolved' && (react_1.default.createElement("div", { className: "hitl-card-resolved" }, resolvedAllowed ? '✓ Allowed' : '✗ Denied'))));
};
exports.PermissionCard = PermissionCard;
//# sourceMappingURL=PermissionCard.js.map