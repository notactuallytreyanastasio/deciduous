"use strict";
/**
 * SSE stream parser and response accumulator
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.ResponseAccumulator = void 0;
exports.createAccumulatingStream = createAccumulatingStream;
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
                    else if (event.delta.type === 'input_json_delta' && event.delta.text) {
                        // Tool input comes as partial JSON
                        if (this.currentToolIndex >= 0 && this.toolCalls[this.currentToolIndex]) {
                            this.toolCalls[this.currentToolIndex].input =
                                (this.toolCalls[this.currentToolIndex].input || '') + event.delta.text;
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
