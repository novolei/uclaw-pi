use async_trait::async_trait;
use std::collections::HashMap;
use tracing::{debug, info, warn};
use url::Url;

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput};

/// Default request timeout in milliseconds.
const DEFAULT_TIMEOUT_MS: u64 = 15_000;

/// Maximum response body size (10 MB). Modern news / finance / search pages
/// often weigh 2-5 MB after decompression; 1 MB rejected real-world fetches
/// (yahoo finance was the trigger). 10 MB still bounds memory pressure but
/// covers the long tail. Oversized responses are now TRUNCATED at this cap
/// rather than rejected — partial content beats no content for the LLM.
const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

/// User-Agent header for the generic `http_request` tool (honest client id).
const USER_AGENT: &str = "uClaw/0.1";

/// User-Agent for `web_fetch`. A bare `uClaw/0.1` with no `Accept` headers reads
/// as a bot to CDN bot managers (Akamai/Cloudflare), which on protected paths
/// challenge or RST the connection — surfacing to reqwest as a non-timeout
/// `NetworkError`. A real browser UA paired with browser `Accept` headers raises
/// the pass rate at zero cost.
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// Largest prefix of `s` that's `<= max_bytes` AND ends at a UTF-8 char
/// boundary. Returns the original on input shorter than the cap.
///
/// Why this exists: `s[..n]` panics when `n` lands inside a multi-byte
/// codepoint. Real-world bug — fetching a CJK / emoji-heavy page and
/// capping at 1 MB landed exactly inside a Chinese character, crashing
/// the tokio worker.
fn truncate_at_byte_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut i = max_bytes;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    &s[..i]
}

/// Validate a URL to prevent SSRF attacks.
/// Rejects non-http(s) schemes and private/loopback addresses.
fn validate_url(url: &str) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only http/https URLs are allowed".into());
    }
    if let Some(host) = parsed.host_str() {
        if host == "localhost" || host == "::1" || host.starts_with("127.")
            || host.starts_with("10.") || host.starts_with("192.168.")
            || host.starts_with("169.254.") {
            return Err("Access to private/loopback addresses is not allowed".into());
        }
        // Check 172.16.0.0 - 172.31.255.255
        if host.starts_with("172.") {
            if let Some(second) = host.split('.').nth(1) {
                if let Ok(n) = second.parse::<u8>() {
                    if (16..=31).contains(&n) {
                        return Err("Access to private addresses is not allowed".into());
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// WebFetchTool — fetch a web page and return plain text
// ---------------------------------------------------------------------------

pub struct WebFetchTool;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }

    /// HTML → plain-text extractor using scraper for proper DOM parsing.
    /// Skips script/style/noscript/template subtrees; inserts newlines at
    /// block-level element boundaries for readable output.
    fn extract_text(html: &str) -> String {
        use scraper::Html;
        let doc = Html::parse_document(html);

        let mut out = String::new();
        walk_for_text(doc.root_element(), &mut out);
        return collapse_whitespace(&out);

        fn walk_for_text(node: scraper::ElementRef, out: &mut String) {
            for child in node.children() {
                if let Some(text) = child.value().as_text() {
                    out.push_str(text);
                    continue;
                }
                if let Some(elem) = scraper::ElementRef::wrap(child) {
                    let tag = elem.value().name();
                    // Skip non-content elements entirely.
                    if matches!(tag, "script" | "style" | "noscript" | "template") {
                        continue;
                    }
                    // Block-level elements get a newline boundary on entry.
                    if matches!(
                        tag,
                        "br" | "p" | "div" | "h1" | "h2" | "h3" | "h4"
                        | "h5" | "h6" | "li" | "tr" | "section"
                        | "article" | "header" | "footer"
                    ) {
                        out.push('\n');
                    }
                    walk_for_text(elem, out);
                    // And on exit (except for void elements like <br>).
                    if matches!(
                        tag,
                        "p" | "div" | "h1" | "h2" | "h3" | "h4"
                        | "h5" | "h6" | "li" | "tr" | "section"
                        | "article"
                    ) {
                        out.push('\n');
                    }
                }
            }
        }

        fn collapse_whitespace(s: &str) -> String {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    /// Heuristic SPA detection. Returns true when the page looks like a
    /// JavaScript-rendered single-page app whose initial HTML carries
    /// minimal human content — the scraper-extracted text will undercount.
    ///
    /// All three conditions required:
    ///   1. > 5 `<script>` tags
    ///   2. Extracted visible text < 500 characters
    ///   3. At least one recognized framework mount marker
    fn detect_spa(html: &str, extracted_text: &str) -> bool {
        use scraper::{Html, Selector};
        let doc = Html::parse_document(html);

        // (1) script tag count > 5
        let script_sel = Selector::parse("script").unwrap();
        if doc.select(&script_sel).count() <= 5 {
            return false;
        }

        // (2) visible body text under 500 chars
        if extracted_text.chars().count() >= 500 {
            return false;
        }

        // (3) at least one obvious framework mount marker
        let markers = [
            "#root", "#app", "#__next", "#__nuxt",
            "[data-reactroot]", "[ng-app]",
        ];
        for m in &markers {
            if let Ok(sel) = Selector::parse(m) {
                if doc.select(&sel).next().is_some() {
                    return true;
                }
            }
        }
        false
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a web page and return its text content. \
         HTML is automatically converted to plain text (scripts and styles are removed)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Request timeout in milliseconds (default 15000)"
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum characters to return (default 50000)"
                }
            },
            "required": ["url"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::UnlessAutoApproved
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let url = params["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("url is required".into()))?;
        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(DEFAULT_TIMEOUT_MS);
        let max_length = params["max_length"].as_u64().unwrap_or(50_000) as usize;

        // Validate URL to prevent SSRF
        if let Err(reason) = validate_url(url) {
            warn!(url, reason = %reason, "web_fetch: URL rejected");
            return Err(ToolError::kinded(
                ToolErrorKind::PermissionDenied,
                format!("URL blocked: {}", reason),
            ));
        }

        info!(url, timeout_ms, "Fetching web page");

        // Browser-like request headers. CDN bot managers score on header
        // presence; a bare UA with no Accept headers reads as a bot. Cheap signal.
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
        );
        headers.insert(
            reqwest::header::ACCEPT_LANGUAGE,
            reqwest::header::HeaderValue::from_static("en-US,en;q=0.9"),
        );

        let total = std::time::Duration::from_millis(timeout_ms);
        let client = reqwest::Client::builder()
            .user_agent(BROWSER_USER_AGENT)
            .default_headers(headers)
            // Bound the connect phase independently so a transient DNS/connect
            // stall fails fast enough to retry *within* the caller's budget,
            // instead of one stalled attempt eating the whole window.
            .connect_timeout(total.min(std::time::Duration::from_secs(8)))
            .timeout(total)
            .build()
            .map_err(|e| ToolError::kinded_with_source(
                ToolErrorKind::Other,
                "Failed to create HTTP client",
                e.to_string(),
            ))?;

        // One bounded retry on a *transient connection* failure (DNS hiccup,
        // connect reset) — never on timeouts or HTTP status errors. This is the
        // exact failure class seen under tool-concurrency bursts: the OS
        // resolver's ~5s default timeout fires and reqwest returns a non-timeout
        // NetworkError well inside our own (15–30s) budget. GET is idempotent, so
        // retrying is safe and turns a spurious hard failure into a success.
        let resp = {
            let mut attempt: u32 = 0;
            loop {
                match client.get(url).send().await {
                    Ok(resp) => break resp,
                    Err(e) => {
                        let transient = e.is_connect() && !e.is_timeout();
                        if transient && attempt == 0 {
                            attempt += 1;
                            warn!(url, cause = %e, "web_fetch: transient connect error — retrying once");
                            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                            continue;
                        }
                        let kind = if e.is_timeout() {
                            ToolErrorKind::Timeout
                        } else {
                            ToolErrorKind::NetworkError
                        };
                        return Err(ToolError::kinded_with_source(
                            kind,
                            format!("Failed to fetch {}", url),
                            e.to_string(),
                        ));
                    }
                }
            }
        };

        let status = resp.status();
        if !status.is_success() {
            let code = status.as_u16();
            let kind = match code {
                400..=403 => ToolErrorKind::PermissionDenied,
                404 => ToolErrorKind::ResourceNotFound,
                408 | 504 => ToolErrorKind::Timeout,
                429 => ToolErrorKind::RateLimited,
                500..=599 => ToolErrorKind::UpstreamError,
                _ => ToolErrorKind::Other,
            };
            warn!(url, %status, "Non-success HTTP status");
            return Err(ToolError::kinded(
                kind,
                format!("Page returned {} ({})", code, url),
            ));
        }

        // Read body. Real-world pages often exceed our cap; truncate at a
        // UTF-8 char boundary instead of rejecting, so the agent gets the
        // top of the page (head/title/intro) rather than nothing.
        let raw_body = resp
            .text()
            .await
            .map_err(|e| ToolError::kinded_with_source(
                ToolErrorKind::ParseError,
                "Failed to decode response body",
                e.to_string(),
            ))?;
        let body = if raw_body.len() > MAX_RESPONSE_SIZE {
            warn!(
                url,
                size = raw_body.len(),
                cap = MAX_RESPONSE_SIZE,
                "Response exceeded cap — truncating"
            );
            truncate_at_byte_boundary(&raw_body, MAX_RESPONSE_SIZE).to_owned()
        } else {
            raw_body
        };

        let text = Self::extract_text(&body);
        let is_spa = Self::detect_spa(&body, &text);

        // max_length is documented as "characters" in the tool schema.
        let total_chars = text.chars().count();
        let truncated = if total_chars > max_length {
            let prefix: String = text.chars().take(max_length).collect();
            format!(
                "{}...\n[Truncated: showing {}/{} characters]",
                prefix, max_length, total_chars
            )
        } else {
            text
        };

        let final_output = if is_spa {
            format!(
                "{}\n\n⚠️ This page appears to be a JavaScript-rendered single-page app \
                 (heuristic: many <script> tags, sparse body text, framework mount point \
                 detected). The text above may be missing dynamic content. For full \
                 content, use the browser tool instead.",
                truncated,
            )
        } else {
            truncated
        };

        debug!(url, chars = final_output.len(), is_spa, "Web page fetched");
        Ok(ToolOutput::success(
            &final_output,
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ---------------------------------------------------------------------------
// HttpRequestTool — generic HTTP request
// ---------------------------------------------------------------------------

pub struct HttpRequestTool;

impl HttpRequestTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for HttpRequestTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Make an HTTP request. Supports GET, POST, PUT, DELETE, PATCH methods \
         with custom headers and JSON body."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"],
                    "description": "HTTP method"
                },
                "url": {
                    "type": "string",
                    "description": "The URL to send the request to"
                },
                "headers": {
                    "type": "object",
                    "description": "Optional HTTP headers as key-value pairs"
                },
                "body": {
                    "type": "string",
                    "description": "Optional request body (JSON string)"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Request timeout in milliseconds (default 15000)"
                }
            },
            "required": ["method", "url"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::UnlessAutoApproved
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let method_str = params["method"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("method is required".into()))?;
        let url = params["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("url is required".into()))?;
        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(DEFAULT_TIMEOUT_MS);

        // Validate URL to prevent SSRF
        if let Err(reason) = validate_url(url) {
            warn!(method = method_str, url, reason = %reason, "http_request: URL rejected");
            return Err(ToolError::kinded(
                ToolErrorKind::PermissionDenied,
                format!("URL blocked: {}", reason),
            ));
        }

        info!(method = method_str, url, "Making HTTP request");

        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| ToolError::kinded_with_source(
                ToolErrorKind::Other,
                "Failed to create HTTP client",
                e.to_string(),
            ))?;

        let method = match method_str.to_uppercase().as_str() {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "DELETE" => reqwest::Method::DELETE,
            "PATCH" => reqwest::Method::PATCH,
            other => {
                return Err(ToolError::InvalidParams(format!(
                    "Unsupported HTTP method: {}",
                    other
                )))
            }
        };

        let mut request = client.request(method, url);

        // Apply custom headers
        if let Some(headers) = params["headers"].as_object() {
            for (key, value) in headers {
                if let Some(val) = value.as_str() {
                    request = request.header(key.as_str(), val);
                }
            }
        }

        // Apply body
        if let Some(body) = params["body"].as_str() {
            if !body.is_empty() {
                request = request
                    .header("Content-Type", "application/json")
                    .body(body.to_string());
            }
        }

        let resp = request
            .send()
            .await
            .map_err(|e| {
                let kind = if e.is_timeout() {
                    ToolErrorKind::Timeout
                } else {
                    ToolErrorKind::NetworkError
                };
                ToolError::kinded_with_source(
                    kind,
                    format!("HTTP request failed: {}", url),
                    e.to_string(),
                )
            })?;

        let status = resp.status().as_u16();
        let resp_headers: HashMap<String, String> = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<binary>").to_string()))
            .collect();

        let body = resp
            .text()
            .await
            .map_err(|e| ToolError::kinded_with_source(
                ToolErrorKind::ParseError,
                "Failed to decode response body",
                e.to_string(),
            ))?;

        // Truncate very large bodies. The MAX_RESPONSE_SIZE cap is for
        // memory safety so bytes are the right unit; round down to a
        // char boundary so the slice doesn't panic on multi-byte
        // codepoints landing across the boundary.
        let body_display = if body.len() > MAX_RESPONSE_SIZE {
            format!(
                "{}...\n[Truncated: {}/{} bytes]",
                truncate_at_byte_boundary(&body, MAX_RESPONSE_SIZE),
                MAX_RESPONSE_SIZE,
                body.len()
            )
        } else {
            body
        };

        let result = serde_json::json!({
            "ok": true,
            "status": status,
            "headers": resp_headers,
            "body": body_display
        });

        debug!(method = method_str, url, status, "HTTP request completed");
        Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_at_byte_boundary_passes_through_short_input() {
        assert_eq!(truncate_at_byte_boundary("hello", 10), "hello");
    }

    #[test]
    fn truncate_at_byte_boundary_caps_at_exact_ascii_boundary() {
        assert_eq!(truncate_at_byte_boundary("hello world", 5), "hello");
    }

    /// Regression for the original panic: `&body[..MAX_RESPONSE_SIZE]`
    /// fell inside a multi-byte codepoint. The helper backs off to the
    /// nearest valid boundary instead of panicking.
    #[test]
    fn truncate_at_byte_boundary_rounds_down_on_multibyte() {
        // "中" is 3 bytes in UTF-8. Asking for max_bytes=2 should give
        // an empty string (no full codepoint fits), not panic.
        assert_eq!(truncate_at_byte_boundary("中", 2), "");
        // 4-byte cap on "中中" (6 bytes total) should keep the first 中
        // (3 bytes) and stop, not slice mid-character.
        assert_eq!(truncate_at_byte_boundary("中中", 4), "中");
    }

    #[test]
    fn truncate_at_byte_boundary_zero_cap() {
        assert_eq!(truncate_at_byte_boundary("中", 0), "");
        assert_eq!(truncate_at_byte_boundary("abc", 0), "");
    }

    #[test]
    fn extract_text_strips_script_and_style() {
        let html = r#"
            <html><head><style>body { color: red }</style></head>
            <body>
                <h1>Title</h1>
                <p>Para 1.</p>
                <script>alert('x')</script>
                <p>Para 2.</p>
            </body></html>"#;
        let text = WebFetchTool::extract_text(html);
        assert!(text.contains("Title"), "got: {:?}", text);
        assert!(text.contains("Para 1"), "got: {:?}", text);
        assert!(text.contains("Para 2"), "got: {:?}", text);
        assert!(!text.contains("alert"), "got: {:?}", text);
        assert!(!text.contains("color: red"), "got: {:?}", text);
    }

    #[test]
    fn detect_spa_recognizes_react_root_with_few_text() {
        let html = r#"
            <html><body>
            <div id="root"></div>
            <script src="bundle.js"></script>
            <script src="vendor.js"></script>
            <script src="runtime.js"></script>
            <script src="polyfills.js"></script>
            <script src="main.js"></script>
            <script src="chunk.js"></script>
            </body></html>"#;
        let text = WebFetchTool::extract_text(html);
        assert!(
            WebFetchTool::detect_spa(html, &text),
            "expected SPA detection; extracted text len = {}",
            text.chars().count()
        );
    }

    #[test]
    fn detect_spa_returns_false_for_content_heavy_page() {
        let html = r#"
            <html><body>
            <article>
                <h1>An Article</h1>
                <p>This is a real content-heavy page. It has many paragraphs
                of text. Real content here, not a SPA wrapper. We expect the
                heuristic to recognize this as NOT a SPA because the visible
                text is substantial. Lorem ipsum dolor sit amet, consectetur
                adipiscing elit. Many many words to push past the 500-char
                threshold so that this fixture clearly disambiguates from a
                sparse SPA shell. Additional padding text to ensure we cross
                the threshold comfortably and the test is not flaky on small
                margins. More content. More content. More content.</p>
            </article>
            </body></html>"#;
        let text = WebFetchTool::extract_text(html);
        assert!(
            !WebFetchTool::detect_spa(html, &text),
            "expected NOT SPA; extracted text len = {}",
            text.chars().count()
        );
    }
}
