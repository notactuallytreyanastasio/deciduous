//! MCP protocol types — JSON-RPC 2.0 and Model Context Protocol messages.
//!
//! This module is a **functional core**: pure data types with serde
//! serialization. No IO, no side effects. All parsing and building
//! happens through `serde_json`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 base types
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request (or notification when `id` is None).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    /// `None` only when the member is absent (a notification). A present
    /// `"id": null` is `Some(Value::Null)`: it is a request and gets a reply.
    #[serde(default, deserialize_with = "present")]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// A JSON-RPC 2.0 success response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Value,
}

/// A JSON-RPC 2.0 error response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcErrorResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub error: JsonRpcError,
}

/// The `error` object inside a JSON-RPC error response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 standard error codes
// ---------------------------------------------------------------------------

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

// ---------------------------------------------------------------------------
// Constructor helpers (pure functions)
// ---------------------------------------------------------------------------

/// Build a JSON-RPC success response.
pub fn success_response(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result,
    }
}

/// Build a JSON-RPC error response.
pub fn error_response(id: Value, code: i64, message: impl Into<String>) -> JsonRpcErrorResponse {
    JsonRpcErrorResponse {
        jsonrpc: "2.0".to_string(),
        id,
        error: JsonRpcError {
            code,
            message: message.into(),
            data: None,
        },
    }
}

/// Build a JSON-RPC error response with extra data.
pub fn error_response_with_data(
    id: Value,
    code: i64,
    message: impl Into<String>,
    data: Value,
) -> JsonRpcErrorResponse {
    JsonRpcErrorResponse {
        jsonrpc: "2.0".to_string(),
        id,
        error: JsonRpcError {
            code,
            message: message.into(),
            data: Some(data),
        },
    }
}

fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

/// Replace `\uD800`-style escapes that are not half of a surrogate pair
/// with `\uFFFD`.
///
/// JSON's grammar allows them (JavaScript's JSON.stringify emits them for a
/// string cut in the middle of an emoji); serde_json refuses the whole
/// document. Refusing means answering with `"id": null`, which the client
/// cannot match to its request, so it waits forever. Replacing the lone
/// half with U+FFFD is what every lossy UTF-16 decoder does.
pub fn replace_lone_surrogates(input: &str) -> std::borrow::Cow<'_, str> {
    fn hex4(b: &[u8]) -> Option<u16> {
        let s = std::str::from_utf8(b.get(..4)?).ok()?;
        u16::from_str_radix(s, 16).ok()
    }
    let bytes = input.as_bytes();
    if !input.contains("\\u") {
        return std::borrow::Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut last = 0;
    let mut changed = false;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        if bytes.get(i + 1) != Some(&b'u') {
            i += 2; // any other escape, including an escaped backslash
            continue;
        }
        let Some(unit) = hex4(&bytes[i + 2..]) else {
            i += 2;
            continue;
        };
        match unit {
            0xD800..=0xDBFF => {
                let next_is_low = bytes.get(i + 6) == Some(&b'\\')
                    && bytes.get(i + 7) == Some(&b'u')
                    && hex4(&bytes[(i + 8).min(bytes.len())..])
                        .is_some_and(|u| (0xDC00..=0xDFFF).contains(&u));
                if next_is_low {
                    i += 12;
                    continue;
                }
            }
            0xDC00..=0xDFFF => {}
            _ => {
                i += 6;
                continue;
            }
        }
        out.push_str(&input[last..i]);
        out.push_str("\\uFFFD");
        i += 6;
        last = i;
        changed = true;
    }
    if !changed {
        return std::borrow::Cow::Borrowed(input);
    }
    out.push_str(&input[last..]);
    std::borrow::Cow::Owned(out)
}

/// The `id` of a message that failed validation, if it has a usable one,
/// so the error reply reaches the request that caused it. `Null` when the
/// text is not JSON at all or the id is not a string or number.
pub fn request_id(input: &str) -> Value {
    serde_json::from_str::<Value>(&replace_lone_surrogates(input))
        .ok()
        .and_then(|v| v.get("id").cloned())
        .filter(|id| id.is_string() || id.is_number())
        .unwrap_or(Value::Null)
}

/// Parse a raw JSON string into a `JsonRpcRequest`.
pub fn parse_request(input: &str) -> Result<JsonRpcRequest, JsonRpcError> {
    let input = replace_lone_surrogates(input);
    let value: Value = serde_json::from_str(&input).map_err(|e| JsonRpcError {
        code: PARSE_ERROR,
        message: format!("Parse error: {e}"),
        data: None,
    })?;

    // Must be an object
    if !value.is_object() {
        return Err(JsonRpcError {
            code: INVALID_REQUEST,
            message: "Request must be a JSON object".to_string(),
            data: None,
        });
    }

    // Must have jsonrpc: "2.0"
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(JsonRpcError {
            code: INVALID_REQUEST,
            message: "Missing or invalid jsonrpc version (must be \"2.0\")".to_string(),
            data: None,
        });
    }

    // Must have a method string
    if value.get("method").and_then(Value::as_str).is_none() {
        return Err(JsonRpcError {
            code: INVALID_REQUEST,
            message: "Missing or invalid method field".to_string(),
            data: None,
        });
    }

    serde_json::from_value(value).map_err(|e| JsonRpcError {
        code: INVALID_REQUEST,
        message: format!("Invalid request structure: {e}"),
        data: None,
    })
}

// ---------------------------------------------------------------------------
// MCP-specific types
// ---------------------------------------------------------------------------

/// Server info returned during initialization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Server capabilities advertised during initialization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
}

/// Indicates the server supports tools.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolsCapability {
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// The result of `initialize`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

/// Build the initialize result for this server.
pub fn build_initialize_result() -> InitializeResult {
    InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability {
                list_changed: Some(false),
            }),
        },
        server_info: ServerInfo {
            name: "deciduous".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    }
}

// ---------------------------------------------------------------------------
// MCP Tool types
// ---------------------------------------------------------------------------

/// A tool definition returned by `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// The result of `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolListResult {
    pub tools: Vec<ToolDefinition>,
}

/// Parameters for `tools/call`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Option<Value>,
}

/// A single content item in a tool result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResultContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// The result of `tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallResult {
    pub content: Vec<ToolResultContent>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Build a successful text tool result.
pub fn tool_result_text(text: impl Into<String>) -> ToolCallResult {
    ToolCallResult {
        content: vec![ToolResultContent {
            content_type: "text".to_string(),
            text: text.into(),
        }],
        is_error: None,
    }
}

/// Build a JSON tool result (serializes value to pretty-printed text).
pub fn tool_result_json(value: &Value) -> ToolCallResult {
    ToolCallResult {
        content: vec![ToolResultContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string()),
        }],
        is_error: None,
    }
}

/// Build an error tool result.
pub fn tool_result_error(message: impl Into<String>) -> ToolCallResult {
    ToolCallResult {
        content: vec![ToolResultContent {
            content_type: "text".to_string(),
            text: message.into(),
        }],
        is_error: Some(true),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_valid_request() {
        let input = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let req = parse_request(input).unwrap();
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, Some(json!(1)));
    }

    #[test]
    fn test_parse_notification_no_id() {
        let input = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let req = parse_request(input).unwrap();
        assert_eq!(req.method, "notifications/initialized");
        assert!(req.id.is_none());
    }

    #[test]
    fn test_parse_error_invalid_json() {
        let err = parse_request("not json").unwrap_err();
        assert_eq!(err.code, PARSE_ERROR);
    }

    #[test]
    fn test_parse_error_missing_jsonrpc() {
        let err = parse_request(r#"{"method":"foo"}"#).unwrap_err();
        assert_eq!(err.code, INVALID_REQUEST);
        assert!(err.message.contains("jsonrpc"));
    }

    #[test]
    fn test_parse_error_wrong_jsonrpc_version() {
        let err = parse_request(r#"{"jsonrpc":"1.0","method":"foo"}"#).unwrap_err();
        assert_eq!(err.code, INVALID_REQUEST);
    }

    #[test]
    fn test_parse_error_missing_method() {
        let err = parse_request(r#"{"jsonrpc":"2.0","id":1}"#).unwrap_err();
        assert_eq!(err.code, INVALID_REQUEST);
        assert!(err.message.contains("method"));
    }

    #[test]
    fn test_parse_error_not_object() {
        let err = parse_request(r#"[1,2,3]"#).unwrap_err();
        assert_eq!(err.code, INVALID_REQUEST);
    }

    #[test]
    fn test_success_response_serialization() {
        let resp = success_response(json!(1), json!({"ok": true}));
        let serialized = serde_json::to_value(&resp).unwrap();
        assert_eq!(serialized["jsonrpc"], "2.0");
        assert_eq!(serialized["id"], 1);
        assert_eq!(serialized["result"]["ok"], true);
    }

    #[test]
    fn test_error_response_serialization() {
        let resp = error_response(json!(1), METHOD_NOT_FOUND, "not found");
        let serialized = serde_json::to_value(&resp).unwrap();
        assert_eq!(serialized["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(serialized["error"]["message"], "not found");
        // data should be absent (skip_serializing_if)
        assert!(serialized["error"].get("data").is_none());
    }

    #[test]
    fn test_error_response_with_data() {
        let resp =
            error_response_with_data(json!(2), INVALID_PARAMS, "bad param", json!({"field": "x"}));
        let serialized = serde_json::to_value(&resp).unwrap();
        assert_eq!(serialized["error"]["data"]["field"], "x");
    }

    #[test]
    fn test_build_initialize_result() {
        let result = build_initialize_result();
        assert_eq!(result.protocol_version, "2024-11-05");
        assert_eq!(result.server_info.name, "deciduous");
        assert!(result.capabilities.tools.is_some());
    }

    #[test]
    fn test_tool_result_text() {
        let result = tool_result_text("hello");
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0].content_type, "text");
        assert_eq!(result.content[0].text, "hello");
        assert!(result.is_error.is_none());
    }

    #[test]
    fn test_tool_result_json() {
        let result = tool_result_json(&json!({"nodes": 5}));
        assert!(result.content[0].text.contains("\"nodes\": 5"));
        assert!(result.is_error.is_none());
    }

    #[test]
    fn test_tool_result_error() {
        let result = tool_result_error("something broke");
        assert_eq!(result.content[0].text, "something broke");
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn test_tool_call_params_deserialization() {
        let json = r#"{"name":"add_node","arguments":{"node_type":"goal","title":"Test"}}"#;
        let params: ToolCallParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.name, "add_node");
        assert!(params.arguments.is_some());
    }

    #[test]
    fn test_tool_call_params_no_arguments() {
        let json = r#"{"name":"list_nodes"}"#;
        let params: ToolCallParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.name, "list_nodes");
        assert!(params.arguments.is_none());
    }

    #[test]
    fn test_request_roundtrip() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(42)),
            method: "tools/call".to_string(),
            params: Some(json!({"name": "add_node"})),
        };
        let serialized = serde_json::to_string(&req).unwrap();
        let parsed = parse_request(&serialized).unwrap();
        assert_eq!(req, parsed);
    }

    #[test]
    fn test_initialize_result_serialization() {
        let result = build_initialize_result();
        let value = serde_json::to_value(&result).unwrap();
        // Verify camelCase field names
        assert!(value.get("protocolVersion").is_some());
        assert!(value.get("serverInfo").is_some());
        assert!(value["serverInfo"].get("name").is_some());
        assert!(value["capabilities"]["tools"].get("listChanged").is_some());
    }

    #[test]
    fn lone_surrogates_are_replaced_and_pairs_kept() {
        let bs = '\\';
        let esc = |hex: &str| format!("{bs}u{hex}");
        assert_eq!(
            replace_lone_surrogates(&format!("\"a{}b\"", esc("d800"))),
            format!("\"a{}b\"", esc("FFFD"))
        );
        assert_eq!(
            replace_lone_surrogates(&format!("\"{}\"", esc("dc00"))),
            format!("\"{}\"", esc("FFFD"))
        );
        // A real pair (U+1F600) and an escaped backslash are left alone.
        let pair = format!("\"{}{} and {bs}{}\"", esc("d83d"), esc("de00"), esc("d800"));
        let pair = pair.as_str();
        assert_eq!(replace_lone_surrogates(pair), pair);
        assert!(parse_request(r#"{"jsonrpc":"2.0","id":1,"method":"x\ud800"}"#).is_ok());
    }

    #[test]
    fn request_id_recovers_the_id_of_an_invalid_request() {
        assert_eq!(request_id(r#"{"jsonrpc":"1.0","id":7,"method":"x"}"#), 7);
        assert_eq!(request_id(r#"{"id":"abc"}"#), "abc");
        assert_eq!(request_id("not json"), Value::Null);
        assert_eq!(request_id(r#"{"id":{"x":1}}"#), Value::Null);
    }
}
