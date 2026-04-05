"use strict";
/**
 * Webview entry point — mounts the React app into the DOM.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const react_1 = __importDefault(require("react"));
const client_1 = require("react-dom/client");
const App_1 = require("./App");
require("./styles.css");
const rootEl = document.getElementById('root');
if (rootEl) {
    const root = (0, client_1.createRoot)(rootEl);
    root.render(react_1.default.createElement(App_1.App, null));
}
//# sourceMappingURL=index.js.map