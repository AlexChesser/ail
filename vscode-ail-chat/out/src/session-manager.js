"use strict";
/**
 * SessionManager — persists chat sessions to VS Code globalState.
 *
 * A session is created when the user first submits a prompt.
 * The title is set from the first prompt, truncated to 50 chars.
 * Sessions are stored as an array ordered newest-first.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.SessionManager = void 0;
const SESSIONS_KEY = 'ail-chat.sessions';
const MAX_SESSIONS = 50;
class SessionManager {
    constructor(context) {
        this._currentSessionId = null;
        this._context = context;
    }
    _load() {
        return this._context.globalState.get(SESSIONS_KEY, []);
    }
    async _save(sessions) {
        await this._context.globalState.update(SESSIONS_KEY, sessions);
    }
    /** Return all sessions as summaries (newest first). */
    async getSessions() {
        return this._load().map((s) => ({
            id: s.id,
            title: s.title,
            timestamp: s.timestamp,
            totalCostUsd: s.totalCostUsd,
        }));
    }
    /**
     * Record the first prompt of a new session. If this is the first prompt in
     * the current session, creates the session record. Otherwise no-op (session
     * title is already set).
     */
    async recordPrompt(prompt) {
        if (this._currentSessionId !== null) {
            // Session already exists; only the first prompt sets the title.
            return;
        }
        const id = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
        this._currentSessionId = id;
        const title = prompt.length > 50 ? prompt.slice(0, 50) + '…' : prompt;
        const session = {
            id,
            title,
            timestamp: Date.now(),
            totalCostUsd: 0,
            firstPrompt: prompt,
        };
        const sessions = this._load();
        sessions.unshift(session);
        // Cap at MAX_SESSIONS
        if (sessions.length > MAX_SESSIONS) {
            sessions.splice(MAX_SESSIONS);
        }
        await this._save(sessions);
    }
    /** Update the cumulative cost for the current session. */
    async updateCost(costDelta) {
        if (this._currentSessionId === null)
            return;
        const sessions = this._load();
        const idx = sessions.findIndex((s) => s.id === this._currentSessionId);
        if (idx >= 0) {
            sessions[idx].totalCostUsd += costDelta;
            await this._save(sessions);
        }
    }
    /**
     * Switch to a historical session. Returns the session if found.
     * Resets the current session ID so the next prompt starts a new session.
     */
    async switchSession(sessionId) {
        this._currentSessionId = null;
        const sessions = this._load();
        return sessions.find((s) => s.id === sessionId) ?? null;
    }
    /** Start a fresh session (called when user clicks "New Session"). */
    newSession() {
        this._currentSessionId = null;
    }
}
exports.SessionManager = SessionManager;
//# sourceMappingURL=session-manager.js.map