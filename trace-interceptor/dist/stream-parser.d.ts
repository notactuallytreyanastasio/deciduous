/**
 * SSE stream parser and response accumulator
 */
import type { SpanData } from './types';
export declare class ResponseAccumulator {
    private thinking;
    private response;
    private toolCalls;
    private currentToolIndex;
    private inputTokens;
    private outputTokens;
    private cacheRead;
    private cacheWrite;
    private stopReason;
    private requestId;
    private model;
    /**
     * Process an SSE event line
     */
    processLine(line: string): void;
    /**
     * Process a parsed SSE event
     */
    private processEvent;
    /**
     * Process a chunk of SSE data
     */
    processChunk(chunk: string): void;
    /**
     * Get the accumulated span data
     */
    finalize(): Omit<SpanData, 'duration_ms'>;
}
/**
 * Create a passthrough stream that accumulates response data
 */
export declare function createAccumulatingStream(originalBody: ReadableStream<Uint8Array>, accumulator: ResponseAccumulator, onComplete: () => void): ReadableStream<Uint8Array>;
