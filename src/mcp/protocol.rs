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
    #[serde(default)]
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

/// Parse a raw JSON string into a `JsonRpcRequest`.
pub fn parse_request(input: &str) -> Result<JsonRpcRequest, JsonRpcError> {
    let value: Value = serde_json::from_str(input).map_err(|e| JsonRpcError {
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
}
