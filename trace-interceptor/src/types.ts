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
  tool_results?: ToolResult[];      // Results from previous tool calls (from request)
  // New fields for comprehensive tracing
  system_prompt?: string;           // The hidden system instructions
  tool_definitions?: ToolDefinition[]; // Available tools with schemas
  message_count?: number;           // Number of messages in conversation
}

export interface ToolResult {
  tool_use_id: string;
  content: string;                  // The tool's output
  is_error?: boolean;
}

export interface ToolDefinition {
  name: string;
  description?: string;
  input_schema?: unknown;
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

export interface ContentBlock {
  type: string;
  text?: string;
  tool_use_id?: string;     // For tool_result blocks
  content?: string | ContentBlock[];  // For tool_result blocks (can be string or nested blocks)
  is_error?: boolean;       // For tool_result blocks
}

export interface AnthropicRequest {
  model?: string;
  messages?: Array<{
    role: string;
    content: string | ContentBlock[];
  }>;
  system?: string | Array<{ type: string; text?: string }>;
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
