/**
 * Deciduous Trace Interceptor
 *
 * Intercepts fetch() calls to the Anthropic API and records them to deciduous.
 *
 * Usage: NODE_OPTIONS="--require /path/to/dist/interceptor-loader.js" claude
 */

import { DeciduousClient } from './deciduous';
import { ResponseAccumulator, createAccumulatingStream } from './stream-parser';
import type { SpanData, AnthropicRequest, AnthropicResponse, ToolDefinition, ToolResult, ContentBlock } from './types';
import * as fs from 'fs';
import * as path from 'path';

// Store original fetch
const originalFetch = globalThis.fetch;

// Debug logging to file
const DEBUG_LOG = process.env.DECIDUOUS_TRACE_DEBUG ?
  path.join(process.env.HOME || '/tmp', '.deciduous', 'trace-debug.log') : null;

function debugLog(msg: string): void {
  if (DEBUG_LOG) {
    try {
      fs.appendFileSync(DEBUG_LOG, `${new Date().toISOString()} ${msg}\n`);
    } catch {
      // Ignore write errors
    }
  }
}

// Client instance (lazy initialized)
let client: DeciduousClient | null = null;

/**
 * Check if this is an Anthropic API request
 */
function isAnthropicApi(input: RequestInfo | URL): boolean {
  const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
  return url.includes('api.anthropic.com');
}

/**
 * Check if response is streaming SSE
 */
function isStreamingResponse(response: Response): boolean {
  const contentType = response.headers.get('content-type');
  return contentType?.includes('text/event-stream') || false;
}

/**
 * Check if text is a system-injected message (not actual user input)
 */
function isSystemInjectedText(text: string): boolean {
  const trimmed = text.trim();
  // Skip exact internal check messages
  if (trimmed === 'quota' || trimmed === 'foo' || trimmed === '#') return true;
  // Skip system-injected content blocks (various Claude Code internals)
  if (trimmed.startsWith('<system-reminder>')) return true;
  if (trimmed.startsWith('<system>')) return true;
  if (trimmed.startsWith('<policy_spec>')) return true;
  if (trimmed.startsWith('<context>')) return true;
  if (trimmed.startsWith('<command-message>')) return true;
  if (trimmed.startsWith('Files modified by user:')) return true;
  if (trimmed.startsWith('Files modified by other')) return true;
  // Everything else is potentially user content
  return false;
}

/**
 * Extract user message preview from request body
 * Looks for actual user text content, skipping:
 * - tool_result blocks (these are tool outputs, not user input)
 * - system-injected reminders
 * - internal check messages
 */
function extractUserPreview(body: AnthropicRequest): string | undefined {
  if (!body.messages || body.messages.length === 0) {
    if (process.env.DECIDUOUS_TRACE_DEBUG) {
      debugLog(' extractUserPreview: no messages');
    }
    return undefined;
  }

  if (process.env.DECIDUOUS_TRACE_DEBUG) {
    debugLog(` extractUserPreview: ${body.messages.length} messages`);
    // Log summary of all messages for debugging
    for (let i = 0; i < body.messages.length; i++) {
      const m = body.messages[i];
      const contentDesc = typeof m.content === 'string'
        ? `string(${m.content.length}): "${m.content.slice(0, 30)}..."`
        : Array.isArray(m.content)
          ? `array[${m.content.length}]: ${(m.content as ContentBlock[]).map(b => b.type).join(',')}`
          : 'unknown';
      debugLog(`   msg[${i}] role=${m.role} content=${contentDesc}`);
    }
  }

  // Find the last user message with actual text content (not system-injected)
  for (let i = body.messages.length - 1; i >= 0; i--) {
    const msg = body.messages[i];
    if (msg.role !== 'user') continue;

    // String content
    if (typeof msg.content === 'string') {
      const text = msg.content.trim();
      const filtered = isSystemInjectedText(text);
      if (process.env.DECIDUOUS_TRACE_DEBUG) {
        debugLog(` msg[${i}] string: len=${text.length}, filtered=${filtered}, text="${text.slice(0, 40)}"`);
      }
      if (text.length > 0 && !filtered) {
        return text.slice(0, 500);
      }
      continue; // System-injected or empty, try earlier message
    }

    // Array content - look for text blocks (skip tool_result, tool_use, image blocks)
    if (Array.isArray(msg.content)) {
      const blocks = msg.content as ContentBlock[];
      const textBlocks = blocks.filter(b => b.type === 'text');
      if (process.env.DECIDUOUS_TRACE_DEBUG) {
        debugLog(` msg[${i}] array: ${blocks.length} blocks, ${textBlocks.length} text blocks`);
      }
      for (const block of textBlocks) {
        if (typeof block.text === 'string') {
          const text = block.text.trim();
          const filtered = isSystemInjectedText(text);
          if (process.env.DECIDUOUS_TRACE_DEBUG) {
            debugLog(`   text block: len=${text.length}, filtered=${filtered}, text="${text.slice(0, 40)}"`);
          }
          if (text.length > 0 && !filtered) {
            return text.slice(0, 500);
          }
        }
      }
      continue;
    }
  }

  if (process.env.DECIDUOUS_TRACE_DEBUG) {
    debugLog(' No user text found in any message');
  }
  return undefined;
}

/**
 * Extract system prompt from request body
 */
function extractSystemPrompt(body: AnthropicRequest): string | undefined {
  if (!body.system) return undefined;

  // System can be a string or array of content blocks
  if (typeof body.system === 'string') {
    return body.system;
  }

  // Array of content blocks
  const textBlocks = body.system.filter((b) => b.type === 'text' && b.text);
  return textBlocks.map((b) => b.text).join('\n') || undefined;
}

/**
 * Extract tool definitions from request body
 */
function extractToolDefinitions(body: AnthropicRequest): ToolDefinition[] | undefined {
  if (!body.tools || !Array.isArray(body.tools)) return undefined;

  const defs: ToolDefinition[] = [];
  for (const tool of body.tools) {
    if (typeof tool === 'object' && tool !== null && 'name' in tool) {
      defs.push({
        name: (tool as { name: string }).name,
        description: (tool as { description?: string }).description,
        input_schema: (tool as { input_schema?: unknown }).input_schema,
      });
    }
  }
  return defs.length > 0 ? defs : undefined;
}

/**
 * Extract tool results from request body (results from previous tool calls)
 */
function extractToolResults(body: AnthropicRequest): ToolResult[] | undefined {
  if (!body.messages || body.messages.length === 0) return undefined;

  const results: ToolResult[] = [];

  for (const msg of body.messages) {
    if (msg.role !== 'user') continue;
    if (!Array.isArray(msg.content)) continue;

    for (const block of msg.content as ContentBlock[]) {
      if (block.type === 'tool_result' && block.tool_use_id) {
        // Content can be a string or nested content blocks
        let content: string;
        if (typeof block.content === 'string') {
          content = block.content;
        } else if (Array.isArray(block.content)) {
          // Extract text from nested blocks
          content = block.content
            .filter((b) => b.type === 'text' && b.text)
            .map((b) => b.text)
            .join('\n');
        } else {
          content = '';
        }

        results.push({
          tool_use_id: block.tool_use_id,
          content: content.slice(0, 5000), // Limit to 5KB per result
          is_error: block.is_error,
        });
      }
    }
  }

  return results.length > 0 ? results : undefined;
}

/**
 * Parse non-streaming response
 */
function parseNonStreamingResponse(data: AnthropicResponse): Omit<SpanData, 'duration_ms'> {
  let thinking = '';
  let response = '';
  const toolCalls: Array<{ id?: string; name?: string; input?: string }> = [];

  if (data.content) {
    for (const block of data.content) {
      if (block.type === 'thinking' && block.thinking) {
        thinking += block.thinking;
      } else if (block.type === 'text' && block.text) {
        response += block.text;
      } else if (block.type === 'tool_use') {
        toolCalls.push({
          id: block.id,
          name: block.name,
          input: JSON.stringify(block.input),
        });
      }
    }
  }

  return {
    model: data.model,
    request_id: data.id,
    stop_reason: data.stop_reason,
    input_tokens: data.usage?.input_tokens,
    output_tokens: data.usage?.output_tokens,
    cache_read: data.usage?.cache_read_input_tokens,
    cache_write: data.usage?.cache_creation_input_tokens,
    thinking_preview: thinking.slice(0, 500) || undefined,
    response_preview: response.slice(0, 500) || undefined,
    tool_names: toolCalls.map((t) => t.name).filter(Boolean).join(',') || undefined,
    thinking: thinking || undefined,
    response: response || undefined,
    tool_calls: toolCalls.length > 0 ? toolCalls : undefined,
  };
}

/**
 * Ensure client is initialized and session started
 */
async function ensureSession(): Promise<DeciduousClient> {
  if (!client) {
    client = new DeciduousClient();
  }
  await client.startSession();
  return client;
}

/**
 * Intercepted fetch function
 */
async function interceptedFetch(
  input: RequestInfo | URL,
  init?: RequestInit
): Promise<Response> {
  // Log ALL fetch calls for debugging
  if (process.env.DECIDUOUS_TRACE_DEBUG) {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : (input as Request).url;
    debugLog(` FETCH: ${url.slice(0, 60)}...`);
  }

  // Pass through non-Anthropic requests
  if (!isAnthropicApi(input)) {
    return originalFetch(input, init);
  }

  // Initialize session
  const deciduous = await ensureSession();
  const startTime = Date.now();

  // Parse request body for preview
  let requestBody: AnthropicRequest | undefined;
  if (init?.body) {
    try {
      let bodyStr: string;
      if (typeof init.body === 'string') {
        bodyStr = init.body;
      } else if (init.body instanceof Uint8Array || init.body instanceof ArrayBuffer) {
        bodyStr = new TextDecoder().decode(init.body);
      } else if (ArrayBuffer.isView(init.body)) {
        bodyStr = new TextDecoder().decode(init.body);
      } else {
        // Unknown type - try toString
        if (init.body && typeof (init.body as unknown as { toString: () => string }).toString === 'function') {
          const strVal = String(init.body);
          if (strVal !== '[object Object]' && strVal !== '[object ReadableStream]') {
            bodyStr = strVal;
          } else {
            bodyStr = '';
          }
        } else {
          bodyStr = '';
        }
      }

      if (bodyStr) {
        requestBody = JSON.parse(bodyStr);
        if (process.env.DECIDUOUS_TRACE_DEBUG) {
          debugLog(` Parsed body, messages: ${requestBody?.messages?.length || 0}`);
        }
      }
    } catch (e) {
      if (process.env.DECIDUOUS_TRACE_DEBUG) {
        debugLog(` Body parse error: ${e}`);
      }
    }
  }

  const userPreview = requestBody ? extractUserPreview(requestBody) : undefined;
  if (process.env.DECIDUOUS_TRACE_DEBUG) {
    debugLog(` userPreview: ${userPreview ? userPreview.slice(0, 50) + '...' : 'null'}`);
  }
  const systemPrompt = requestBody ? extractSystemPrompt(requestBody) : undefined;
  const toolDefs = requestBody ? extractToolDefinitions(requestBody) : undefined;
  const toolResults = requestBody ? extractToolResults(requestBody) : undefined;
  const messageCount = requestBody?.messages?.length;

  // Start span BEFORE making the request (active span tracking)
  // This enables nodes created during the API call to be linked to this span
  const spanId = await deciduous.startSpan(userPreview);
  if (spanId !== null) {
    process.env.DECIDUOUS_TRACE_SPAN = spanId.toString();
  }

  // Make the actual request
  const response = await originalFetch(input, init);

  // Handle streaming response
  if (isStreamingResponse(response)) {
    const accumulator = new ResponseAccumulator();

    const onComplete = async () => {
      const duration = Date.now() - startTime;
      const spanData: SpanData = {
        ...accumulator.finalize(),
        duration_ms: duration,
        user_preview: userPreview,
        system_prompt: systemPrompt,
        tool_definitions: toolDefs,
        tool_results: toolResults,
        message_count: messageCount,
      };

      await deciduous.recordSpan(spanData, spanId ?? undefined);

      // NOTE: Don't delete DECIDUOUS_TRACE_SPAN here!
      // Tools are executed AFTER the API response completes but BEFORE the next API call.
      // The span ID needs to persist so `deciduous add` can link nodes to this span.
      // The env var will be overwritten when the next span starts (line 145).
    };

    // Wrap the response body
    const wrappedBody = createAccumulatingStream(
      response.body!,
      accumulator,
      onComplete
    );

    return new Response(wrappedBody, {
      status: response.status,
      statusText: response.statusText,
      headers: response.headers,
    });
  }

  // Handle non-streaming response
  const responseData = await response.clone().json();
  const duration = Date.now() - startTime;

  const spanData: SpanData = {
    ...parseNonStreamingResponse(responseData),
    duration_ms: duration,
    user_preview: userPreview,
    system_prompt: systemPrompt,
    tool_definitions: toolDefs,
    tool_results: toolResults,
    message_count: messageCount,
  };

  await deciduous.recordSpan(spanData, spanId ?? undefined);

  // NOTE: Don't delete DECIDUOUS_TRACE_SPAN here!
  // Same as streaming case - tools run after response, need span ID.

  return response;
}

/**
 * Install the interceptor
 */
export function install(): void {
  if (process.env.DECIDUOUS_TRACE_DISABLE) {
    return;
  }

  // Replace global fetch
  globalThis.fetch = interceptedFetch;

  // End session on process exit
  process.on('beforeExit', async () => {
    if (client) {
      await client.endSession();
    }
  });

  process.on('SIGINT', async () => {
    if (client) {
      await client.endSession();
    }
    process.exit(0);
  });

  process.on('SIGTERM', async () => {
    if (client) {
      await client.endSession();
    }
    process.exit(0);
  });

  if (process.env.DECIDUOUS_TRACE_DEBUG) {
    debugLog(' Interceptor installed');
  }
}

// Export for testing
export { DeciduousClient, ResponseAccumulator };
