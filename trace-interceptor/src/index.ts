/**
 * Deciduous Trace Interceptor
 *
 * Intercepts fetch() calls to the Anthropic API and records them to deciduous.
 *
 * Usage: NODE_OPTIONS="--require /path/to/dist/interceptor-loader.js" claude
 */

import { DeciduousClient } from './deciduous';
import { ResponseAccumulator, createAccumulatingStream } from './stream-parser';
import type { SpanData, AnthropicRequest, AnthropicResponse, ToolDefinition } from './types';

// Store original fetch
const originalFetch = globalThis.fetch;

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
 * Extract user message preview from request body
 * Looks for actual text content, skipping tool_result blocks
 */
function extractUserPreview(body: AnthropicRequest): string | undefined {
  if (!body.messages) return undefined;

  // Find the last user message with actual text content
  for (let i = body.messages.length - 1; i >= 0; i--) {
    const msg = body.messages[i];
    if (msg.role === 'user') {
      if (typeof msg.content === 'string') {
        const trimmed = msg.content.trim();
        // Skip very short or system-like content
        if (trimmed.length > 10 && !trimmed.startsWith('<system')) {
          return trimmed.slice(0, 500);
        }
      } else if (Array.isArray(msg.content)) {
        // Look for text blocks, skip tool_result blocks
        for (const block of msg.content) {
          if (block.type === 'text' && block.text) {
            const trimmed = block.text.trim();
            // Skip very short or system-like content
            if (trimmed.length > 10 && !trimmed.startsWith('<system')) {
              return trimmed.slice(0, 500);
            }
          }
        }
      }
    }
  }

  // If no good user text found, try to get SOMETHING useful
  // Look at assistant messages for context (what was Claude responding to?)
  for (let i = body.messages.length - 1; i >= 0; i--) {
    const msg = body.messages[i];
    if (msg.role === 'assistant') {
      if (typeof msg.content === 'string' && msg.content.trim().length > 20) {
        // Use first line of assistant response as context hint
        const firstLine = msg.content.trim().split('\n')[0];
        if (firstLine.length > 10) {
          return `[continuing] ${firstLine.slice(0, 450)}`;
        }
      }
    }
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
      requestBody = JSON.parse(
        typeof init.body === 'string' ? init.body : new TextDecoder().decode(init.body as ArrayBuffer)
      );
    } catch {
      // Ignore parse errors
    }
  }

  const userPreview = requestBody ? extractUserPreview(requestBody) : undefined;
  const systemPrompt = requestBody ? extractSystemPrompt(requestBody) : undefined;
  const toolDefs = requestBody ? extractToolDefinitions(requestBody) : undefined;
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
    console.error('[deciduous-trace] Interceptor installed');
  }
}

// Export for testing
export { DeciduousClient, ResponseAccumulator };
