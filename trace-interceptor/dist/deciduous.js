"use strict";
/**
 * Client for communicating with the deciduous CLI
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.DeciduousClient = void 0;
const child_process_1 = require("child_process");
// Silent logging helper - only logs if DECIDUOUS_TRACE_DEBUG is explicitly set
const debugLog = (msg) => {
    if (process.env.DECIDUOUS_TRACE_DEBUG === '1' || process.env.DECIDUOUS_TRACE_DEBUG === 'true') {
        // Write to stderr but with a format that won't trigger error detection
        process.stderr.write(`[deciduous] ${msg}\n`);
    }
};
class DeciduousClient {
    sessionId = null;
    deciduousBin;
    constructor() {
        // Use DECIDUOUS_BIN env var or default to 'deciduous'
        this.deciduousBin = process.env.DECIDUOUS_BIN || 'deciduous';
        // Use existing session from proxy command if available
        this.sessionId = process.env.DECIDUOUS_TRACE_SESSION || null;
    }
    /**
     * Start or resume a trace session
     * If DECIDUOUS_TRACE_SESSION is set (by proxy command), use that session
     * Otherwise, start a new one
     */
    async startSession() {
        if (this.sessionId) {
            debugLog(`Using existing session: ${this.sessionId.slice(0, 8)}`);
            return this.sessionId;
        }
        try {
            const result = (0, child_process_1.execSync)(`${this.deciduousBin} trace start --command "claude"`, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] });
            const parsed = JSON.parse(result.trim());
            this.sessionId = parsed.session_id;
            debugLog(`Started session: ${this.sessionId.slice(0, 8)}`);
            return this.sessionId;
        }
        catch (error) {
            debugLog(`Failed to start session: ${error}`);
            throw error;
        }
    }
    /**
     * Start a new span before making an API call (for active tracking)
     * Returns the span ID which should be set as DECIDUOUS_TRACE_SPAN env var
     */
    async startSpan(userPreview) {
        if (!this.sessionId) {
            debugLog('No active session');
            return null;
        }
        try {
            // Use simple command without user_preview to avoid shell escaping issues
            // The user_preview will be sent via recordSpan which uses stdin
            const result = (0, child_process_1.execSync)(`${this.deciduousBin} trace span-start --session ${this.sessionId}`, {
                encoding: 'utf8',
                stdio: ['pipe', 'pipe', 'pipe'],
            });
            const parsed = JSON.parse(result.trim());
            debugLog(`Started span #${parsed.span_id}`);
            return parsed.span_id;
        }
        catch (error) {
            debugLog(`Failed to start span: ${error}`);
            return null;
        }
    }
    /**
     * Record/complete a span (request + response)
     * If spanId is provided, completes an existing span; otherwise creates a new one
     */
    async recordSpan(data, spanId) {
        if (!this.sessionId) {
            debugLog('No active session');
            return null;
        }
        try {
            const input = JSON.stringify(data);
            const args = [`trace`, `record`, `--session`, this.sessionId, `--stdin`];
            if (spanId !== undefined) {
                args.push(`--span-id`, spanId.toString());
            }
            const result = (0, child_process_1.execSync)(`${this.deciduousBin} ${args.join(' ')}`, {
                encoding: 'utf8',
                input,
                stdio: ['pipe', 'pipe', 'pipe'],
            });
            const parsed = JSON.parse(result.trim());
            debugLog(`Recorded span #${parsed.span_id}`);
            return parsed.span_id;
        }
        catch (error) {
            debugLog(`Failed to record span: ${error}`);
            return null;
        }
    }
    /**
     * End the current trace session
     * Note: If session was provided by proxy (DECIDUOUS_TRACE_SESSION),
     * the proxy handles ending it, so we just clear our reference
     */
    async endSession() {
        if (!this.sessionId) {
            return;
        }
        // If session was provided by proxy, don't end it - proxy handles that
        if (process.env.DECIDUOUS_TRACE_SESSION) {
            debugLog('Session managed by proxy, not ending');
            this.sessionId = null;
            return;
        }
        try {
            (0, child_process_1.execSync)(`${this.deciduousBin} trace end ${this.sessionId}`, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] });
            debugLog(`Ended session: ${this.sessionId.slice(0, 8)}`);
        }
        catch (error) {
            debugLog(`Failed to end session: ${error}`);
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
