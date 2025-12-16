"use strict";
/**
 * Client for communicating with the deciduous CLI
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.DeciduousClient = void 0;
const child_process_1 = require("child_process");
class DeciduousClient {
    sessionId = null;
    deciduousBin;
    constructor() {
        // Use DECIDUOUS_BIN env var or default to 'deciduous'
        this.deciduousBin = process.env.DECIDUOUS_BIN || 'deciduous';
    }
    /**
     * Start a new trace session
     */
    async startSession() {
        if (this.sessionId) {
            return this.sessionId;
        }
        try {
            const result = (0, child_process_1.execSync)(`${this.deciduousBin} trace start --command "claude"`, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] });
            const parsed = JSON.parse(result.trim());
            this.sessionId = parsed.session_id;
            if (process.env.DECIDUOUS_TRACE_DEBUG) {
                console.error(`[deciduous-trace] Started session: ${this.sessionId.slice(0, 8)}`);
            }
            return this.sessionId;
        }
        catch (error) {
            console.error('[deciduous-trace] Failed to start session:', error);
            throw error;
        }
    }
    /**
     * Record a complete span (request + response)
     */
    async recordSpan(data) {
        if (!this.sessionId) {
            console.error('[deciduous-trace] No active session');
            return null;
        }
        try {
            const input = JSON.stringify(data);
            const result = (0, child_process_1.execSync)(`${this.deciduousBin} trace record --session ${this.sessionId} --stdin`, {
                encoding: 'utf8',
                input,
                stdio: ['pipe', 'pipe', 'pipe'],
            });
            const parsed = JSON.parse(result.trim());
            if (process.env.DECIDUOUS_TRACE_DEBUG) {
                console.error(`[deciduous-trace] Recorded span #${parsed.span_id}`);
            }
            return parsed.span_id;
        }
        catch (error) {
            console.error('[deciduous-trace] Failed to record span:', error);
            return null;
        }
    }
    /**
     * End the current trace session
     */
    async endSession() {
        if (!this.sessionId) {
            return;
        }
        try {
            (0, child_process_1.execSync)(`${this.deciduousBin} trace end ${this.sessionId}`, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] });
            if (process.env.DECIDUOUS_TRACE_DEBUG) {
                console.error(`[deciduous-trace] Ended session: ${this.sessionId.slice(0, 8)}`);
            }
        }
        catch (error) {
            console.error('[deciduous-trace] Failed to end session:', error);
        }
        finally {
            this.sessionId = null;
        }
    }
    /**
     * Get the current session ID
     */
    getSessionId() {
        return this.sessionId;
    }
}
exports.DeciduousClient = DeciduousClient;
