/**
 * Client for communicating with the deciduous CLI
 */
import type { SpanData } from './types';
export declare class DeciduousClient {
    private sessionId;
    private deciduousBin;
    constructor();
    /**
     * Start a new trace session
     */
    startSession(): Promise<string>;
    /**
     * Record a complete span (request + response)
     */
    recordSpan(data: SpanData): Promise<number | null>;
    /**
     * End the current trace session
     */
    endSession(): Promise<void>;
    /**
     * Get the current session ID
     */
    getSessionId(): string | null;
}
