/**
 * Types for the deciduous trace interceptor
 */
export interface SpanData {
    model?: string;
    user_preview?: string;
    duration_ms?: number;
    request_id?: string;
    stop_reason?: string;
    input_tokens?: number;
    output_tokens?: number;
    cache_read?: number;
    cache_write?: number;
    thinking_preview?: string;
    response_preview?: string;
    tool_names?: string;
    thinking?: string;
    response?: string;
    tool_calls?: ToolCall[];
}
export interface ToolCall {
    id?: string;
    name?: string;
    input?: string;
    output?: string;
}
export interface StartSessionResponse {
    session_id: string;
}
export interface RecordSpanResponse {
    span_id: number;
}
export interface AnthropicRequest {
    model?: string;
    messages?: Array<{
        role: string;
        content: string | Array<{
            type: string;
            text?: string;
        }>;
    }>;
    system?: string | Array<{
        type: string;
        text?: string;
    }>;
    tools?: unknown[];
    stream?: boolean;
}
export interface AnthropicResponse {
    id?: string;
    type?: string;
    model?: string;
    stop_reason?: string;
    usage?: {
        input_tokens?: number;
        output_tokens?: number;
        cache_read_input_tokens?: number;
        cache_creation_input_tokens?: number;
    };
    content?: Array<{
        type: string;
        text?: string;
        thinking?: string;
        id?: string;
        name?: string;
        input?: unknown;
    }>;
}
export interface SSEEvent {
    type: string;
    index?: number;
    delta?: {
        type?: string;
        text?: string;
        thinking?: string;
    };
    content_block?: {
        type: string;
        id?: string;
        name?: string;
    };
    message?: AnthropicResponse;
    usage?: {
        input_tokens?: number;
        output_tokens?: number;
        cache_read_input_tokens?: number;
        cache_creation_input_tokens?: number;
    };
}
