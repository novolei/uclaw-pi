use super::*;

#[test]
fn kinded_error_displays_with_bracketed_kind() {
    let err = ToolError::kinded(ToolErrorKind::ResourceNotFound, "Page returned 404");
    assert_eq!(format!("{}", err), "[NotFound] Page returned 404");
}

#[test]
fn kinded_error_serializes_through_existing_serde_path() {
    let err = ToolError::kinded(ToolErrorKind::PermissionDenied, "URL blocked");
    let json = serde_json::to_string(&err).unwrap();
    assert!(json.contains("PermissionDenied"), "got json: {}", json);
    assert!(json.contains("URL blocked"), "got json: {}", json);
}

#[test]
fn kinded_with_source_keeps_source_field() {
    let err = ToolError::kinded_with_source(
        ToolErrorKind::ParseError,
        "Could not parse JSON",
        "expected ',' at line 5",
    );
    match err {
        ToolError::Kinded {
            kind,
            message,
            source_context,
        } => {
            assert_eq!(kind, ToolErrorKind::ParseError);
            assert_eq!(message, "Could not parse JSON");
            assert_eq!(source_context.as_deref(), Some("expected ',' at line 5"));
        }
        _ => panic!("expected Kinded variant"),
    }
}

/// Regression: the `Kinded` Display used to drop `source_context`, so a wrapped
/// reqwest DNS/connect failure reached the model as a bare `[NetworkError]
/// Failed to fetch <url>` and got misread as a timeout. The cause must now be
/// surfaced.
#[test]
fn kinded_display_surfaces_source_cause() {
    let err = ToolError::kinded_with_source(
        ToolErrorKind::NetworkError,
        "Failed to fetch https://example.com",
        "error sending request: dns error: failed to lookup address",
    );
    let shown = err.to_string();
    assert!(shown.starts_with("[NetworkError] Failed to fetch https://example.com"));
    assert!(
        shown.contains("dns error"),
        "cause must reach the model; got: {shown}"
    );
}

/// Cause that merely repeats the message is suppressed — no `(X (X))` noise.
#[test]
fn kinded_display_suppresses_redundant_cause() {
    let err = ToolError::kinded_with_source(ToolErrorKind::Other, "boom", "boom");
    assert_eq!(err.to_string(), "[Other] boom");
}

/// No source → unchanged `[Kind] message` form (back-compat for `kinded`).
#[test]
fn kinded_display_without_source_unchanged() {
    let err = ToolError::kinded(ToolErrorKind::ResourceNotFound, "Page returned 404");
    assert_eq!(err.to_string(), "[NotFound] Page returned 404");
}

#[test]
fn tool_context_derives_subcall_without_losing_parent_context() {
    let ctx = ToolExecutionContext::agent_turn(
        "session-1",
        "tool-1",
        Some(PathBuf::from("/tmp/workspace")),
        Some(SafetyMode::Supervised),
    );

    let sub = ctx.for_subcall("tool-2");

    assert_eq!(sub.session_id, "session-1");
    assert_eq!(sub.tool_call_id, "tool-2");
    assert_eq!(
        sub.workspace_root.as_deref(),
        Some(Path::new("/tmp/workspace"))
    );
    assert_eq!(sub.execution_mode, ToolExecutionMode::AgentTurn);
    assert_eq!(sub.safety_mode, Some(SafetyMode::Supervised));
}

#[test]
fn tool_context_resolves_relative_paths_against_workspace_root() {
    let ctx = ToolExecutionContext::agent_turn(
        "session-1",
        "tool-1",
        Some(PathBuf::from("/tmp/workspace")),
        None,
    );

    assert_eq!(
        ctx.resolve_candidate_path("src/lib.rs"),
        PathBuf::from("/tmp/workspace/src/lib.rs"),
    );
    assert_eq!(
        ctx.resolve_candidate_path("/var/tmp/file.txt"),
        PathBuf::from("/var/tmp/file.txt"),
    );
}

#[test]
fn tool_context_without_workspace_keeps_relative_path_relative() {
    let ctx = ToolExecutionContext::agent_turn("session-1", "tool-1", None, None);

    assert_eq!(
        ctx.resolve_candidate_path("src/lib.rs"),
        PathBuf::from("src/lib.rs"),
    );
}

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "echoes input"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::new(params, 7))
    }
}

#[tokio::test]
async fn execute_tool_with_context_preserves_old_execute_behavior() {
    let ctx = ToolExecutionContext::agent_turn(
        "session-1",
        "tool-1",
        Some(PathBuf::from("/tmp/workspace")),
        Some(SafetyMode::Plan),
    );

    let output = execute_tool_with_context(&EchoTool, serde_json::json!({"hello": "world"}), &ctx)
        .await
        .unwrap();

    assert_eq!(output.result, serde_json::json!({"hello": "world"}));
    assert_eq!(output.duration_ms, 7);
}
