/**
 * Client for communicating with the deciduous CLI
 */
import type { SpanData } from './types';
export declare class DeciduousClient {
    private sessionId;
    private deciduousBin;
    constructor();
    /**
     * Start or resume a trace session
     * If DECIDUOUS_TRACE_SESSION is set (by proxy command), use that session
     * Otherwise, start a new one
     */
    startSession(): Promise<string>;
    /**
     * Start a new span before making an API call (for active tracking)
     * Returns the span ID which should be set as DECIDUOUS_TRACE_SPAN env var
     */
    startSpan(userPreview?: string): Promise<number | null>;
    /**
     * Record/complete a span (request + response)
     * If spanId is provided, completes an existing span; otherwise creates a new one
     */
    recordSpan(data: SpanData, spanId?: number): Promise<number | null>;
    /**
     * End the current trace session
     * Note: If session was provided by proxy (DECIDUOUS_TRACE_SESSION),
     * the proxy handles ending it, so we just clear our reference
     */
    endSession(): Promise<void>;
    /**
     * Get the current session ID
     */
    getSessionId(): string | null;
}
