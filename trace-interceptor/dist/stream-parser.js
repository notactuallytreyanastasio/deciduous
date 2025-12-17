"use strict";
/**
 * SSE stream parser and response accumulator
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.ResponseAccumulator = void 0;
exports.createAccumulatingStream = createAccumulatingStream;
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const DEBUG_LOG = process.env.DECIDUOUS_TRACE_DEBUG ?
    path.join(process.env.HOME || '/tmp', '.deciduous', 'trace-debug.log') : null;
function debugLog(msg) {
    if (DEBUG_LOG) {
        try {
            fs.appendFileSync(DEBUG_LOG, `${new Date().toISOString()} [stream] ${msg}\n`);
        }
        catch { /* ignore */ }
    }
}
class ResponseAccumulator {
    thinking = '';
    response = '';
    toolCalls = [];
    currentToolIndex = -1;
    inputTokens = 0;
    outputTokens = 0;
    cacheRead = 0;
    cacheWrite = 0;
    stopReason;
    requestId;
    model;
    /**
     * Process an SSE event line
     */
    processLine(line) {
        if (!line.startsWith('data: ')) {
            return;
        }
        const dataStr = line.slice(6).trim();
        if (dataStr === '[DONE]') {
            return;
        }
        try {
            const event = JSON.parse(dataStr);
            this.processEvent(event);
        }
        catch {
            // Ignore parse errors for incomplete chunks
        }
    }
    /**
     * Process a parsed SSE event
     */
    processEvent(event) {
        switch (event.type) {
            case 'message_start':
                if (event.message) {
                    this.requestId = event.message.id;
                    this.model = event.message.model;
                    if (event.message.usage) {
                        this.inputTokens = event.message.usage.input_tokens || 0;
                        this.cacheRead = event.message.usage.cache_read_input_tokens || 0;
                        this.cacheWrite = event.message.usage.cache_creation_input_tokens || 0;
                    }
                }
                break;
            case 'content_block_start':
                if (event.content_block?.type === 'tool_use') {
                    this.currentToolIndex = this.toolCalls.length;
                    this.toolCalls.push({
                        id: event.content_block.id,
                        name: event.content_block.name,
                        input: '',
                    });
                    debugLog(`tool_use start: ${event.content_block.name} (${event.content_block.id})`);
                }
                break;
            case 'content_block_delta':
                if (event.delta) {
                    if (event.delta.type === 'thinking_delta' && event.delta.thinking) {
                        this.thinking += event.delta.thinking;
                    }
                    else if (event.delta.type === 'text_delta' && event.delta.text) {
                        this.response += event.delta.text;
                    }
                    else if (event.delta.type === 'input_json_delta') {
                        // Tool input comes as partial_json, not text!
                        const partialJson = event.delta.partial_json;
                        if (partialJson && this.currentToolIndex >= 0 && this.toolCalls[this.currentToolIndex]) {
                            this.toolCalls[this.currentToolIndex].input =
                                (this.toolCalls[this.currentToolIndex].input || '') + partialJson;
                            debugLog(`input_json_delta: +${partialJson.length} chars for tool ${this.currentToolIndex}`);
                        }
                    }
                }
                break;
            case 'content_block_stop':
                this.currentToolIndex = -1;
                break;
            case 'message_delta':
                if (event.delta) {
                    const delta = event.delta;
                    if (delta.stop_reason) {
                        this.stopReason = delta.stop_reason;
                    }
                }
                if (event.usage) {
                    this.outputTokens = event.usage.output_tokens || 0;
                }
                break;
            case 'message_stop':
                // Final message
                break;
        }
    }
    /**
     * Process a chunk of SSE data
     */
    processChunk(chunk) {
        const lines = chunk.split('\n');
        for (const line of lines) {
            this.processLine(line);
        }
    }
    /**
     * Get the accumulated span data
     */
    finalize() {
        if (this.toolCalls.length > 0) {
            debugLog(`finalize: ${this.toolCalls.length} tool calls`);
            for (const tc of this.toolCalls) {
                debugLog(`  - ${tc.name}: input len=${tc.input?.length || 0}`);
            }
        }
        return {
            model: this.model,
            request_id: this.requestId,
            stop_reason: this.stopReason,
            input_tokens: this.inputTokens,
            output_tokens: this.outputTokens,
            cache_read: this.cacheRead,
            cache_write: this.cacheWrite,
            thinking_preview: this.thinking.slice(0, 500),
            response_preview: this.response.slice(0, 500),
            tool_names: this.toolCalls.map(t => t.name).filter(Boolean).join(',') || undefined,
            thinking: this.thinking || undefined,
            response: this.response || undefined,
            tool_calls: this.toolCalls.length > 0 ? this.toolCalls : undefined,
        };
    }
}
exports.ResponseAccumulator = ResponseAccumulator;
/**
 * Create a passthrough stream that accumulates response data
 */
function createAccumulatingStream(originalBody, accumulator, onComplete) {
    const reader = originalBody.getReader();
    const decoder = new TextDecoder();
    return new ReadableStream({
        async pull(controller) {
            const { done, value } = await reader.read();
            if (done) {
                onComplete();
                controller.close();
                return;
            }
            // Accumulate the data
            const text = decoder.decode(value, { stream: true });
            accumulator.processChunk(text);
            // Pass through unchanged
            controller.enqueue(value);
        },
        cancel() {
            reader.cancel();
        },
    });
}
