"use strict";

// src/deciduous.ts
var import_child_process = require("child_process");
var DeciduousClient = class {
  sessionId = null;
  deciduousBin;
  constructor() {
    this.deciduousBin = process.env.DECIDUOUS_BIN || "deciduous";
    this.sessionId = process.env.DECIDUOUS_TRACE_SESSION || null;
  }
  /**
   * Start or resume a trace session
   * If DECIDUOUS_TRACE_SESSION is set (by proxy command), use that session
   * Otherwise, start a new one
   */
  async startSession() {
    if (this.sessionId) {
      if (process.env.DECIDUOUS_TRACE_DEBUG) {
        console.error(`[deciduous-trace] Using existing session: ${this.sessionId.slice(0, 8)}`);
      }
      return this.sessionId;
    }
    try {
      const result = (0, import_child_process.execSync)(
        `${this.deciduousBin} trace start --command "claude"`,
        { encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] }
      );
      const parsed = JSON.parse(result.trim());
      this.sessionId = parsed.session_id;
      if (process.env.DECIDUOUS_TRACE_DEBUG) {
        console.error(`[deciduous-trace] Started session: ${this.sessionId.slice(0, 8)}`);
      }
      return this.sessionId;
    } catch (error) {
      console.error("[deciduous-trace] Failed to start session:", error);
      throw error;
    }
  }
  /**
   * Start a new span before making an API call (for active tracking)
   * Returns the span ID which should be set as DECIDUOUS_TRACE_SPAN env var
   */
  async startSpan(userPreview) {
    if (!this.sessionId) {
      console.error("[deciduous-trace] No active session");
      return null;
    }
    try {
      const result = (0, import_child_process.execSync)(
        `${this.deciduousBin} trace span-start --session ${this.sessionId}`,
        {
          encoding: "utf8",
          stdio: ["pipe", "pipe", "pipe"]
        }
      );
      const parsed = JSON.parse(result.trim());
      if (process.env.DECIDUOUS_TRACE_DEBUG) {
        console.error(`[deciduous-trace] Started span #${parsed.span_id}`);
      }
      return parsed.span_id;
    } catch (error) {
      console.error("[deciduous-trace] Failed to start span:", error);
      return null;
    }
  }
  /**
   * Record/complete a span (request + response)
   * If spanId is provided, completes an existing span; otherwise creates a new one
   */
  async recordSpan(data, spanId) {
    if (!this.sessionId) {
      console.error("[deciduous-trace] No active session");
      return null;
    }
    try {
      const input = JSON.stringify(data);
      const args = [`trace`, `record`, `--session`, this.sessionId, `--stdin`];
      if (spanId !== void 0) {
        args.push(`--span-id`, spanId.toString());
      }
      const result = (0, import_child_process.execSync)(
        `${this.deciduousBin} ${args.join(" ")}`,
        {
          encoding: "utf8",
          input,
          stdio: ["pipe", "pipe", "pipe"]
        }
      );
      const parsed = JSON.parse(result.trim());
      if (process.env.DECIDUOUS_TRACE_DEBUG) {
        console.error(`[deciduous-trace] Recorded span #${parsed.span_id}`);
      }
      return parsed.span_id;
    } catch (error) {
      console.error("[deciduous-trace] Failed to record span:", error);
      return null;
    }
  }
  /**
   * End the current trace session
   * Note: If session was provided by proxy (DECIDUOUS_TRACE_SESSION),
   * the proxy handles ending it, so we just clear our reference
   */
  async endSession() {
    if (!this.sessionId) {
      return;
    }
    if (process.env.DECIDUOUS_TRACE_SESSION) {
      if (process.env.DECIDUOUS_TRACE_DEBUG) {
        console.error(`[deciduous-trace] Session managed by proxy, not ending`);
      }
      this.sessionId = null;
      return;
    }
    try {
      (0, import_child_process.execSync)(
        `${this.deciduousBin} trace end ${this.sessionId}`,
        { encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] }
      );
      if (process.env.DECIDUOUS_TRACE_DEBUG) {
        console.error(`[deciduous-trace] Ended session: ${this.sessionId.slice(0, 8)}`);
      }
    } catch (error) {
      console.error("[deciduous-trace] Failed to end session:", error);
    } finally {
      this.sessionId = null;
    }
  }
  /**
   * Get the current session ID
   */
  getSessionId() {
    return this.sessionId;
  }
};

// src/stream-parser.ts
var ResponseAccumulator = class {
  thinking = "";
  response = "";
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
    if (!line.startsWith("data: ")) {
      return;
    }
    const dataStr = line.slice(6).trim();
    if (dataStr === "[DONE]") {
      return;
    }
    try {
      const event = JSON.parse(dataStr);
      this.processEvent(event);
    } catch {
    }
  }
  /**
   * Process a parsed SSE event
   */
  processEvent(event) {
    switch (event.type) {
      case "message_start":
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
      case "content_block_start":
        if (event.content_block?.type === "tool_use") {
          this.currentToolIndex = this.toolCalls.length;
          this.toolCalls.push({
            id: event.content_block.id,
            name: event.content_block.name,
            input: ""
          });
        }
        break;
      case "content_block_delta":
        if (event.delta) {
          if (event.delta.type === "thinking_delta" && event.delta.thinking) {
            this.thinking += event.delta.thinking;
          } else if (event.delta.type === "text_delta" && event.delta.text) {
            this.response += event.delta.text;
          } else if (event.delta.type === "input_json_delta" && event.delta.text) {
            if (this.currentToolIndex >= 0 && this.toolCalls[this.currentToolIndex]) {
              this.toolCalls[this.currentToolIndex].input = (this.toolCalls[this.currentToolIndex].input || "") + event.delta.text;
            }
          }
        }
        break;
      case "content_block_stop":
        this.currentToolIndex = -1;
        break;
      case "message_delta":
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
      case "message_stop":
        break;
    }
  }
  /**
   * Process a chunk of SSE data
   */
  processChunk(chunk) {
    const lines = chunk.split("\n");
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
      tool_names: this.toolCalls.map((t) => t.name).filter(Boolean).join(",") || void 0,
      thinking: this.thinking || void 0,
      response: this.response || void 0,
      tool_calls: this.toolCalls.length > 0 ? this.toolCalls : void 0
    };
  }
};
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
      const text = decoder.decode(value, { stream: true });
      accumulator.processChunk(text);
      controller.enqueue(value);
    },
    cancel() {
      reader.cancel();
    }
  });
}

// src/index.ts
var originalFetch = globalThis.fetch;
var client = null;
function isAnthropicApi(input) {
  const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  return url.includes("api.anthropic.com");
}
function isStreamingResponse(response) {
  const contentType = response.headers.get("content-type");
  return contentType?.includes("text/event-stream") || false;
}
function extractUserPreview(body) {
  if (!body.messages) return void 0;
  for (let i = body.messages.length - 1; i >= 0; i--) {
    const msg = body.messages[i];
    if (msg.role === "user") {
      if (typeof msg.content === "string") {
        const trimmed = msg.content.trim();
        if (trimmed.length > 10 && !trimmed.startsWith("<system")) {
          return trimmed.slice(0, 500);
        }
      } else if (Array.isArray(msg.content)) {
        for (const block of msg.content) {
          if (block.type === "text" && block.text) {
            const trimmed = block.text.trim();
            if (trimmed.length > 10 && !trimmed.startsWith("<system")) {
              return trimmed.slice(0, 500);
            }
          }
        }
      }
    }
  }
  for (let i = body.messages.length - 1; i >= 0; i--) {
    const msg = body.messages[i];
    if (msg.role === "assistant") {
      if (typeof msg.content === "string" && msg.content.trim().length > 20) {
        const firstLine = msg.content.trim().split("\n")[0];
        if (firstLine.length > 10) {
          return `[continuing] ${firstLine.slice(0, 450)}`;
        }
      }
    }
  }
  return void 0;
}
function extractSystemPrompt(body) {
  if (!body.system) return void 0;
  if (typeof body.system === "string") {
    return body.system;
  }
  const textBlocks = body.system.filter((b) => b.type === "text" && b.text);
  return textBlocks.map((b) => b.text).join("\n") || void 0;
}
function extractToolDefinitions(body) {
  if (!body.tools || !Array.isArray(body.tools)) return void 0;
  const defs = [];
  for (const tool of body.tools) {
    if (typeof tool === "object" && tool !== null && "name" in tool) {
      defs.push({
        name: tool.name,
        description: tool.description,
        input_schema: tool.input_schema
      });
    }
  }
  return defs.length > 0 ? defs : void 0;
}
function parseNonStreamingResponse(data) {
  let thinking = "";
  let response = "";
  const toolCalls = [];
  if (data.content) {
    for (const block of data.content) {
      if (block.type === "thinking" && block.thinking) {
        thinking += block.thinking;
      } else if (block.type === "text" && block.text) {
        response += block.text;
      } else if (block.type === "tool_use") {
        toolCalls.push({
          id: block.id,
          name: block.name,
          input: JSON.stringify(block.input)
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
    thinking_preview: thinking.slice(0, 500) || void 0,
    response_preview: response.slice(0, 500) || void 0,
    tool_names: toolCalls.map((t) => t.name).filter(Boolean).join(",") || void 0,
    thinking: thinking || void 0,
    response: response || void 0,
    tool_calls: toolCalls.length > 0 ? toolCalls : void 0
  };
}
async function ensureSession() {
  if (!client) {
    client = new DeciduousClient();
  }
  await client.startSession();
  return client;
}
async function interceptedFetch(input, init) {
  if (!isAnthropicApi(input)) {
    return originalFetch(input, init);
  }
  const deciduous = await ensureSession();
  const startTime = Date.now();
  let requestBody;
  if (init?.body) {
    try {
      requestBody = JSON.parse(
        typeof init.body === "string" ? init.body : new TextDecoder().decode(init.body)
      );
    } catch {
    }
  }
  const userPreview = requestBody ? extractUserPreview(requestBody) : void 0;
  const systemPrompt = requestBody ? extractSystemPrompt(requestBody) : void 0;
  const toolDefs = requestBody ? extractToolDefinitions(requestBody) : void 0;
  const messageCount = requestBody?.messages?.length;
  const spanId = await deciduous.startSpan(userPreview);
  if (spanId !== null) {
    process.env.DECIDUOUS_TRACE_SPAN = spanId.toString();
  }
  const response = await originalFetch(input, init);
  if (isStreamingResponse(response)) {
    const accumulator = new ResponseAccumulator();
    const onComplete = async () => {
      const duration2 = Date.now() - startTime;
      const spanData2 = {
        ...accumulator.finalize(),
        duration_ms: duration2,
        user_preview: userPreview,
        system_prompt: systemPrompt,
        tool_definitions: toolDefs,
        message_count: messageCount
      };
      await deciduous.recordSpan(spanData2, spanId ?? void 0);
    };
    const wrappedBody = createAccumulatingStream(
      response.body,
      accumulator,
      onComplete
    );
    return new Response(wrappedBody, {
      status: response.status,
      statusText: response.statusText,
      headers: response.headers
    });
  }
  const responseData = await response.clone().json();
  const duration = Date.now() - startTime;
  const spanData = {
    ...parseNonStreamingResponse(responseData),
    duration_ms: duration,
    user_preview: userPreview,
    system_prompt: systemPrompt,
    tool_definitions: toolDefs,
    message_count: messageCount
  };
  await deciduous.recordSpan(spanData, spanId ?? void 0);
  return response;
}
function install() {
  if (process.env.DECIDUOUS_TRACE_DISABLE) {
    return;
  }
  globalThis.fetch = interceptedFetch;
  process.on("beforeExit", async () => {
    if (client) {
      await client.endSession();
    }
  });
  process.on("SIGINT", async () => {
    if (client) {
      await client.endSession();
    }
    process.exit(0);
  });
  process.on("SIGTERM", async () => {
    if (client) {
      await client.endSession();
    }
    process.exit(0);
  });
  if (process.env.DECIDUOUS_TRACE_DEBUG) {
    console.error("[deciduous-trace] Interceptor installed");
  }
}

// src/interceptor-loader.ts
install();
