"use strict";
/**
 * Deciduous Trace Interceptor
 *
 * Intercepts fetch() calls to the Anthropic API and records them to deciduous.
 *
 * Usage: NODE_OPTIONS="--require /path/to/dist/interceptor-loader.js" claude
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.ResponseAccumulator = exports.DeciduousClient = void 0;
exports.install = install;
const deciduous_1 = require("./deciduous");
Object.defineProperty(exports, "DeciduousClient", { enumerable: true, get: function () { return deciduous_1.DeciduousClient; } });
const stream_parser_1 = require("./stream-parser");
Object.defineProperty(exports, "ResponseAccumulator", { enumerable: true, get: function () { return stream_parser_1.ResponseAccumulator; } });
// Store original fetch
const originalFetch = globalThis.fetch;
// Client instance (lazy initialized)
let client = null;
/**
 * Check if this is an Anthropic API request
 */
function isAnthropicApi(input) {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    return url.includes('api.anthropic.com');
}
/**
 * Check if response is streaming SSE
 */
function isStreamingResponse(response) {
    const contentType = response.headers.get('content-type');
    return contentType?.includes('text/event-stream') || false;
}
/**
 * Extract user message preview from request body
 */
function extractUserPreview(body) {
    if (!body.messages)
        return undefined;
    // Find the last user message
    for (let i = body.messages.length - 1; i >= 0; i--) {
        const msg = body.messages[i];
        if (msg.role === 'user') {
            if (typeof msg.content === 'string') {
                return msg.content.slice(0, 500);
            }
            // Array of content blocks
            const textBlocks = msg.content.filter((b) => b.type === 'text' && b.text);
            if (textBlocks.length > 0 && textBlocks[0].text) {
                return textBlocks[0].text.slice(0, 500);
            }
        }
    }
    return undefined;
}
/**
 * Parse non-streaming response
 */
function parseNonStreamingResponse(data) {
    let thinking = '';
    let response = '';
    const toolCalls = [];
    if (data.content) {
        for (const block of data.content) {
            if (block.type === 'thinking' && block.thinking) {
                thinking += block.thinking;
            }
            else if (block.type === 'text' && block.text) {
                response += block.text;
            }
            else if (block.type === 'tool_use') {
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
async function ensureSession() {
    if (!client) {
        client = new deciduous_1.DeciduousClient();
    }
    await client.startSession();
    return client;
}
/**
 * Intercepted fetch function
 */
async function interceptedFetch(input, init) {
    // Pass through non-Anthropic requests
    if (!isAnthropicApi(input)) {
        return originalFetch(input, init);
    }
    // Initialize session
    const deciduous = await ensureSession();
    const startTime = Date.now();
    // Parse request body for preview
    let requestBody;
    if (init?.body) {
        try {
            requestBody = JSON.parse(typeof init.body === 'string' ? init.body : new TextDecoder().decode(init.body));
        }
        catch {
            // Ignore parse errors
        }
    }
    const userPreview = requestBody ? extractUserPreview(requestBody) : undefined;
    // Make the actual request
    const response = await originalFetch(input, init);
    // Handle streaming response
    if (isStreamingResponse(response)) {
        const accumulator = new stream_parser_1.ResponseAccumulator();
        const onComplete = async () => {
            const duration = Date.now() - startTime;
            const spanData = {
                ...accumulator.finalize(),
                duration_ms: duration,
                user_preview: userPreview,
            };
            await deciduous.recordSpan(spanData);
        };
        // Wrap the response body
        const wrappedBody = (0, stream_parser_1.createAccumulatingStream)(response.body, accumulator, onComplete);
        return new Response(wrappedBody, {
            status: response.status,
            statusText: response.statusText,
            headers: response.headers,
        });
    }
    // Handle non-streaming response
    const responseData = await response.clone().json();
    const duration = Date.now() - startTime;
    const spanData = {
        ...parseNonStreamingResponse(responseData),
        duration_ms: duration,
        user_preview: userPreview,
    };
    await deciduous.recordSpan(spanData);
    return response;
}
/**
 * Install the interceptor
 */
function install() {
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
