"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.StepProgress = void 0;
const react_1 = __importDefault(require("react"));
function glyph(status) {
    switch (status) {
        case 'running': return react_1.default.createElement("span", { className: "step-glyph running" }, "\u27F3");
        case 'completed': return react_1.default.createElement("span", { className: "step-glyph" }, "\u2713");
        case 'failed': return react_1.default.createElement("span", { className: "step-glyph", style: { color: 'var(--vscode-errorForeground)' } }, "\u2717");
        case 'skipped': return react_1.default.createElement("span", { className: "step-glyph", style: { opacity: 0.5 } }, "\u2013");
        default: return react_1.default.createElement("span", { className: "step-glyph", style: { opacity: 0.4 } }, "\u25CB");
    }
}
const StepProgress = ({ steps, totalCostUsd }) => {
    if (steps.length === 0)
        return null;
    return (react_1.default.createElement("div", { className: "step-progress" },
        react_1.default.createElement("div", { className: "step-progress-title" }, "Steps"),
        steps.map((step) => (react_1.default.createElement("div", { key: step.stepId, className: "step-row" },
            glyph(step.status),
            react_1.default.createElement("span", { className: "step-id" }, step.stepId),
            step.costUsd !== undefined && step.costUsd > 0 && (react_1.default.createElement("span", { className: "step-cost" },
                "$",
                step.costUsd.toFixed(4)))))),
        totalCostUsd !== undefined && totalCostUsd > 0 && (react_1.default.createElement("div", { className: "run-summary" },
            "Total cost: $",
            totalCostUsd.toFixed(4)))));
};
exports.StepProgress = StepProgress;
//# sourceMappingURL=StepProgress.js.map