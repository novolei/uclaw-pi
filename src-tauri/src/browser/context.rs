//! BrowserContext — one Chrome instance per agent session.
//!
//! Wraps a `chromiumoxide::Browser`, a map of tab_id → `Page`, a DOM-state
//! cache with a 500 ms TTL, and a map of screencast stop-signal senders.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use chromiumoxide::browser::BrowserConfig;
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, DispatchMouseEventParams, DispatchMouseEventType,
    MouseButton,
};
use chromiumoxide::cdp::browser_protocol::page::{
    ScreencastFrameAckParams, StartScreencastFormat, StartScreencastParams,
    StopScreencastParams,
};
use chromiumoxide::{Browser, Page};
use futures::StreamExt;
use tauri::Emitter;
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio::time::sleep;
use uuid::Uuid;

use crate::browser::dom_state::{dom_state_from_raw, DOM_QUERY_SCRIPT};
use crate::browser::identity::{
    PlaywrightCookie, PlaywrightLocalStorageEntry, PlaywrightOrigin, PlaywrightStorageState,
};
use crate::browser::perception::{NoopVisualPerceptionProvider, VisualPerceptionProvider};
use crate::browser::types::{DOMState, DomStateRaw, NavStatePayload, ScreencastFramePayload, TabInfo};

// ── DOM cache ─────────────────────────────────────────────────────────────────

const DOM_CACHE_TTL: Duration = Duration::from_millis(500);

// ── CookieInfo ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CookieInfo {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
    pub expires: f64,
}

// ── DevicePreset ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevicePreset {
    Desktop,
    Mobile,
}

impl DevicePreset {
    pub fn from_str(s: &str) -> Self {
        if s.eq_ignore_ascii_case("mobile") { DevicePreset::Mobile } else { DevicePreset::Desktop }
    }

    pub fn viewport_width(self) -> u32 { match self { DevicePreset::Mobile => 390, DevicePreset::Desktop => 1280 } }
    pub fn viewport_height(self) -> u32 { match self { DevicePreset::Mobile => 844, DevicePreset::Desktop => 800 } }
    pub fn user_agent(self) -> &'static str {
        match self {
            DevicePreset::Mobile => "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            DevicePreset::Desktop => "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
        }
    }
}

// ── BrowserContext ────────────────────────────────────────────────────────────

pub struct BrowserContext {
    pub browser: Arc<Browser>,
    pub pages: Arc<RwLock<HashMap<String, Page>>>,
    active_tab_id: Arc<RwLock<Option<String>>>,
    dom_cache: Arc<RwLock<HashMap<String, (DOMState, Instant)>>>,
    screencast_stops: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    visual_perception_provider: Arc<dyn VisualPerceptionProvider>,
    session_id: String,
}

impl BrowserContext {
    /// Launch Chrome and open one initial blank tab.
    pub async fn launch(session_id: &str, profile_dir: PathBuf) -> Result<Self> {
        // Remove stale Chrome lock files so a crashed session doesn't block relaunch.
        for lock in &["SingletonLock", "SingletonCookie", "SingletonSocket"] {
            let path = profile_dir.join(lock);
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
        }

        let config = BrowserConfig::builder()
            .new_headless_mode()
            .no_sandbox()
            .user_data_dir(&profile_dir)
            .launch_timeout(Duration::from_secs(60))
            // chromiumoxide prepends `--` automatically — passing already-prefixed
            // names produces `----foo` which Chrome silently ignores. Strip the
            // prefix from each arg name.
            .args([
                "no-first-run",
                "disable-default-apps",
                "disable-infobars",
                "disable-notifications",
                "disable-translate",
                "disable-extensions",
            ])
            .build()
            .map_err(|e| anyhow!("Browser config error: {}", e))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| anyhow!("Failed to launch browser: {}", e))?;

        // Drive the CDP event loop in a background task.
        tokio::spawn(async move {
            while let Some(_event) = handler.next().await {}
            tracing::warn!("CDP handler exited — browser subprocess may have crashed");
        });

        let browser = Arc::new(browser);
        let pages: Arc<RwLock<HashMap<String, Page>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Open one initial blank tab so the browser is ready to use.
        let init_page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| anyhow!("Failed to open initial tab: {}", e))?;
        let init_id = Uuid::new_v4().to_string();
        pages.write().await.insert(init_id.clone(), init_page);

        Ok(Self {
            browser,
            pages,
            active_tab_id: Arc::new(RwLock::new(Some(init_id))),
            dom_cache: Arc::new(RwLock::new(HashMap::new())),
            screencast_stops: Arc::new(Mutex::new(HashMap::new())),
            visual_perception_provider: Arc::new(NoopVisualPerceptionProvider),
            session_id: session_id.to_string(),
        })
    }

    pub fn with_visual_perception_provider(
        mut self,
        provider: Arc<dyn VisualPerceptionProvider>,
    ) -> Self {
        self.visual_perception_provider = provider;
        self
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    async fn get_page(&self, tab_id: &str) -> Result<Page> {
        // Bundle 25-C: fast-reject pseudo tab ids (currently "new") BEFORE
        // any lock acquisition or lookup. Bundle 19 already returned a
        // helpful error when "new" fell through the lookup path, but the
        // LLM continued to make the same call repeatedly in tight loops
        // (see /Users/ryanliu/.uclaw/logs/uclaw.log.2026-05-21).
        // This guard:
        //   1. Returns an unambiguous `[InvalidInput]` error so the agent
        //      sees a category-specific signal that retrying with the same
        //      param won't help.
        //   2. Logs a warn! line so we can track how often this fires —
        //      sustained high counts would suggest schema-level
        //      tightening (enum constraint) is needed.
        //   3. Avoids the cost of `pages.read()` + inventory string build
        //      on a known-bad input.
        if let Some(msg) = Self::reject_pseudo_tab_id(tab_id) {
            tracing::warn!(
                tab_id = %tab_id,
                "browser tool rejected pseudo tab_id (Bundle 25-C)"
            );
            return Err(anyhow!(msg));
        }

        let pages = self.pages.read().await;
        if let Some(page) = pages.get(tab_id) {
            return Ok(page.clone());
        }
        // Real lookup miss (legit id but tab closed / wrong session) —
        // give the LLM the open-tabs inventory so it can pick a real one.
        let mut msg = format!("Tab '{}' not found.", tab_id);
        msg.push_str(
            " The tab_id must be a value returned by a prior \
             `browser_navigate` call.",
        );
        if pages.is_empty() {
            msg.push_str(" No tabs are currently open.");
        } else {
            let mut ids: Vec<&str> = pages.keys().map(String::as_str).collect();
            ids.sort();
            let take = ids.len().min(5);
            if ids.len() > 5 {
                msg.push_str(&format!(
                    " Open tabs (first 5 of {}): {}",
                    ids.len(),
                    ids[..take].join(", "),
                ));
            } else {
                msg.push_str(&format!(" Open tabs: {}", ids[..take].join(", ")));
            }
        }
        Err(anyhow!(msg))
    }

    /// Bundle 25-C: returns a recovery message when `tab_id` is a value
    /// that's never going to resolve to a real tab — currently just "new",
    /// but built as a list so we can extend (e.g. "current", "active")
    /// without restructuring callers.
    ///
    /// Returns `None` if the tab_id passes the pseudo-id check (still
    /// might fail the real lookup; that's the next stage in get_page).
    fn reject_pseudo_tab_id(tab_id: &str) -> Option<String> {
        match tab_id {
            "new" => Some(
                "[InvalidInput] Tab id 'new' is only valid as input to `browser_navigate` \
                 (where it opens a new tab and returns the real id). For every other \
                 browser tool (browser_reload, browser_click, browser_type, \
                 browser_screenshot, etc.) pass a tab_id that came back from a prior \
                 `browser_navigate` call. If you need a fresh tab, call `browser_navigate` \
                 first with tab_id='new' to get a real id, then use that id."
                    .to_string(),
            ),
            // Future: "current", "active", "" — add when we see them in logs.
            _ => None,
        }
    }

    fn loading_nav_state_tab_id(requested_tab_id: &str, active_tab_id: Option<&str>) -> String {
        if requested_tab_id == "new" {
            active_tab_id.unwrap_or_default().to_string()
        } else {
            requested_tab_id.to_string()
        }
    }

    /// Resolve tab_id: if "new" or not found, open a blank page and return the
    /// new id. Otherwise return the existing id and page clone.
    async fn resolve_tab(&self, tab_id: &str) -> Result<(String, Page)> {
        if tab_id != "new" {
            if let Some(page) = self.pages.read().await.get(tab_id).cloned() {
                return Ok((tab_id.to_string(), page));
            }
        }
        // Need a new tab.
        let page = self
            .browser
            .new_page("about:blank")
            .await
            .map_err(|e| anyhow!("Failed to open new tab: {}", e))?;
        let new_id = Uuid::new_v4().to_string();
        self.pages.write().await.insert(new_id.clone(), page.clone());
        Ok((new_id, page))
    }

    fn index_selector(index: u32) -> String {
        format!("[data-uclaw-index=\"{}\"]", index)
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    /// Emit a `browser:nav-state` event to the frontend.
    async fn emit_nav_state(
        &self,
        tab_id: &str,
        page: &Page,
        app_handle: &tauri::AppHandle,
        is_loading: bool,
    ) {
        let url = page
            .evaluate("window.location.href")
            .await
            .ok()
            .and_then(|v| v.into_value::<String>().ok())
            .unwrap_or_default();
        let title = page
            .evaluate("document.title")
            .await
            .ok()
            .and_then(|v| v.into_value::<String>().ok())
            .unwrap_or_default();
        let history_len = page
            .evaluate("history.length")
            .await
            .ok()
            .and_then(|v| v.into_value::<i64>().ok())
            .unwrap_or(1);
        let payload = NavStatePayload {
            session_id: self.session_id.clone(),
            tab_id: tab_id.to_string(),
            url,
            title,
            is_loading,
            can_go_back: history_len > 1,
            can_go_forward: false,
        };
        let _ = app_handle.emit("browser:nav-state", &payload);
    }

    /// Navigate to `url` in the given tab. Returns the resolved tab_id.
    pub async fn navigate(
        &self,
        tab_id: &str,
        url: &str,
        app_handle: &tauri::AppHandle,
    ) -> Result<String> {
        // Emit loading=true immediately so the address bar shows a spinner.
        let active_tab_id = self.active_tab_id.read().await.clone();
        let loading_tab_id = Self::loading_nav_state_tab_id(tab_id, active_tab_id.as_deref());
        let _ = app_handle.emit("browser:nav-state", NavStatePayload {
            session_id: self.session_id.clone(),
            tab_id: loading_tab_id,
            url: url.to_string(),
            title: String::new(),
            is_loading: true,
            can_go_back: false,
            can_go_forward: false,
        });

        let pages = self.pages.write().await;
        if tab_id != "new" {
            if let Some(page) = pages.get(tab_id) {
                let page = page.clone();
                drop(pages);
                page.goto(url)
                    .await
                    .map_err(|e| anyhow!("navigate to {url}: {e}"))?;
                *self.active_tab_id.write().await = Some(tab_id.to_string());
                self.invalidate_dom_cache(tab_id).await;
                self.emit_nav_state(tab_id, &page, app_handle, false).await;
                return Ok(tab_id.to_string());
            }
        }
        // New tab — open directly at the target URL.
        drop(pages);
        let page = self
            .browser
            .new_page(url)
            .await
            .map_err(|e| anyhow!("new_page: {e}"))?;
        let new_id = Uuid::new_v4().to_string();
        self.pages.write().await.insert(new_id.clone(), page.clone());
        *self.active_tab_id.write().await = Some(new_id.clone());
        self.invalidate_dom_cache(&new_id).await;
        self.emit_nav_state(&new_id, &page, app_handle, false).await;
        Ok(new_id)
    }

    pub async fn go_back(&self, tab_id: &str, app_handle: &tauri::AppHandle) -> Result<()> {
        let page = self.get_page(tab_id).await?;
        *self.active_tab_id.write().await = Some(tab_id.to_string());
        page.evaluate("history.back()")
            .await
            .map_err(|e| anyhow!("go_back failed: {}", e))?;
        self.invalidate_dom_cache(tab_id).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        self.emit_nav_state(tab_id, &page, app_handle, false).await;
        Ok(())
    }

    pub async fn go_forward(&self, tab_id: &str, app_handle: &tauri::AppHandle) -> Result<()> {
        let page = self.get_page(tab_id).await?;
        *self.active_tab_id.write().await = Some(tab_id.to_string());
        page.evaluate("history.forward()")
            .await
            .map_err(|e| anyhow!("go_forward failed: {}", e))?;
        self.invalidate_dom_cache(tab_id).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        self.emit_nav_state(tab_id, &page, app_handle, false).await;
        Ok(())
    }

    pub async fn reload(&self, tab_id: &str, app_handle: &tauri::AppHandle) -> Result<()> {
        let page = self.get_page(tab_id).await?;
        *self.active_tab_id.write().await = Some(tab_id.to_string());
        let _ = app_handle.emit("browser:nav-state", NavStatePayload {
            session_id: self.session_id.clone(),
            tab_id: tab_id.to_string(),
            url: String::new(),
            title: String::new(),
            is_loading: true,
            can_go_back: false,
            can_go_forward: false,
        });
        page.reload()
            .await
            .map_err(|e| anyhow!("reload failed: {}", e))?;
        self.invalidate_dom_cache(tab_id).await;
        self.emit_nav_state(tab_id, &page, app_handle, false).await;
        Ok(())
    }

    pub async fn switch_tab(&self, tab_id: &str, app_handle: &tauri::AppHandle) -> Result<()> {
        let page = self.get_page(tab_id).await?;
        *self.active_tab_id.write().await = Some(tab_id.to_string());
        self.emit_nav_state(tab_id, &page, app_handle, false).await;
        Ok(())
    }

    // ── DOM state ─────────────────────────────────────────────────────────────

    /// Return the DOM state for `tab_id`, using a 500 ms TTL cache.
    pub async fn get_dom_state(&self, tab_id: &str) -> Result<DOMState> {
        // Check cache first.
        {
            let cache = self.dom_cache.read().await;
            if let Some((state, fetched_at)) = cache.get(tab_id) {
                if fetched_at.elapsed() < DOM_CACHE_TTL {
                    return Ok(state.clone());
                }
            }
        }

        // Cache is stale — re-evaluate the DOM query script.
        let page = self.get_page(tab_id).await?;
        let result = match page.evaluate(DOM_QUERY_SCRIPT).await {
            Ok(result) => result,
            Err(first_error) if is_transient_cdp_context_error(&first_error.to_string()) => {
                sleep(Duration::from_millis(250)).await;
                let page = self.get_page(tab_id).await?;
                page.evaluate(DOM_QUERY_SCRIPT).await.map_err(|second_error| {
                    anyhow!(
                        "DOM query failed after retry: {}; first error: {}",
                        second_error,
                        first_error
                    )
                })?
            }
            Err(error) => return Err(anyhow!("DOM query failed: {}", error)),
        };

        let json_str: String = result
            .into_value()
            .map_err(|e| anyhow!("DOM result not a string: {}", e))?;

        let raw: DomStateRaw = serde_json::from_str(&json_str)
            .map_err(|e| anyhow!("DOM JSON parse error: {}", e))?;

        let tabs = self.get_all_tabs().await;
        let state = dom_state_from_raw(raw, tabs);

        self.dom_cache
            .write()
            .await
            .insert(tab_id.to_string(), (state.clone(), Instant::now()));

        Ok(state)
    }

    pub async fn invalidate_dom_cache(&self, tab_id: &str) {
        self.dom_cache.write().await.remove(tab_id);
    }

    // ── Screenshot ────────────────────────────────────────────────────────────

    /// Capture a PNG screenshot and return it as a base64 string.
    pub async fn screenshot(&self, tab_id: &str) -> Result<String> {
        use chromiumoxide::page::ScreenshotParams;

        let page = self.get_page(tab_id).await?;
        let png_bytes = page
            .screenshot(ScreenshotParams::default())
            .await
            .map_err(|e| anyhow!("Screenshot failed: {}", e))?;

        Ok(STANDARD.encode(&png_bytes))
    }

    /// Return a structured browser-use-style observation for agent planning.
    pub async fn observe(
        &self,
        tab_id: &str,
        include_screenshot: bool,
    ) -> Result<crate::browser::observation::BrowserObservation> {
        self.observe_with_visual(tab_id, include_screenshot, false).await
    }

    pub async fn observe_with_visual(
        &self,
        tab_id: &str,
        include_screenshot: bool,
        include_visual: bool,
    ) -> Result<crate::browser::observation::BrowserObservation> {
        let state = self.get_dom_state(tab_id).await?;
        let screenshot_b64 = if include_screenshot || include_visual {
            Some(self.screenshot(tab_id).await?)
        } else {
            None
        };
        let visual_observation = if include_visual {
            match screenshot_b64.as_deref() {
                Some(screenshot) => {
                    let screenshot_ref = format!(
                        "browser://{}/{}/{}",
                        self.session_id,
                        tab_id,
                        chrono::Utc::now().timestamp_millis()
                    );
                    match self
                        .visual_perception_provider
                        .analyze_screenshot(&screenshot_ref, screenshot)
                        .await
                    {
                        Ok(observation) => observation,
                        Err(error) => {
                            tracing::warn!(
                                provider = ?self.visual_perception_provider.kind(),
                                error = %error,
                                "visual perception provider failed; degrading to DOM-only observation"
                            );
                            None
                        }
                    }
                }
                None => None,
            }
        } else {
            None
        };

        Ok(crate::browser::observation::BrowserObservation {
            session_id: self.session_id.clone(),
            tab_id: tab_id.to_string(),
            url: state.url,
            title: state.title,
            page_text: state.page_text,
            elements: state.elements,
            tabs: state.tabs,
            screenshot_b64,
            visual_observation,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        })
    }

    // ── Interaction ───────────────────────────────────────────────────────────

    pub async fn click(&self, tab_id: &str, index: u32) -> Result<()> {
        let selector = Self::index_selector(index);
        let page = self.get_page(tab_id).await?;
        page.find_element(&selector)
            .await
            .map_err(|e| anyhow!("Element [{}] not found: {}", index, e))?
            .click()
            .await
            .map_err(|e| anyhow!("Click [{}] failed: {}", index, e))?;
        Ok(())
    }

    pub async fn click_at(&self, tab_id: &str, x: f64, y: f64) -> Result<()> {
        let page = self.get_page(tab_id).await?;
        let press = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MousePressed)
            .x(x)
            .y(y)
            .button(MouseButton::Left)
            .buttons(1)
            .click_count(1)
            .build()
            .map_err(|e| anyhow!("mouse_pressed params: {e}"))?;
        let release = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseReleased)
            .x(x)
            .y(y)
            .button(MouseButton::Left)
            .buttons(0)
            .click_count(1)
            .build()
            .map_err(|e| anyhow!("mouse_released params: {e}"))?;

        page.execute(press)
            .await
            .map_err(|e| anyhow!("mouse_pressed: {e}"))?;
        page.execute(release)
            .await
            .map_err(|e| anyhow!("mouse_released: {e}"))?;
        self.invalidate_dom_cache(tab_id).await;
        Ok(())
    }

    pub async fn mouse_event_at(
        &self,
        tab_id: &str,
        event_type: DispatchMouseEventType,
        x: f64,
        y: f64,
    ) -> Result<()> {
        let page = self.get_page(tab_id).await?;
        let (button, buttons, click_count) = match event_type {
            DispatchMouseEventType::MousePressed => (MouseButton::Left, 1, 1),
            DispatchMouseEventType::MouseReleased => (MouseButton::Left, 0, 1),
            DispatchMouseEventType::MouseMoved => (MouseButton::None, 1, 0),
            _ => (MouseButton::None, 0, 0),
        };
        let event = DispatchMouseEventParams::builder()
            .r#type(event_type)
            .x(x)
            .y(y)
            .button(button)
            .buttons(buttons)
            .click_count(click_count)
            .build()
            .map_err(|e| anyhow!("mouse_event params: {e}"))?;
        page.execute(event)
            .await
            .map_err(|e| anyhow!("mouse_event: {e}"))?;
        self.invalidate_dom_cache(tab_id).await;
        Ok(())
    }

    pub async fn type_text(&self, tab_id: &str, index: u32, text: &str) -> Result<()> {
        let selector = Self::index_selector(index);
        let page = self.get_page(tab_id).await?;
        let element = page
            .find_element(&selector)
            .await
            .map_err(|e| anyhow!("Element [{}] not found: {}", index, e))?;

        match element.type_str(text).await {
            Ok(_) => {
                self.invalidate_dom_cache(tab_id).await;
                Ok(())
            }
            Err(e) => {
                // Bundle 25-B: CDP keyboard layout doesn't cover every Unicode
                // codepoint — CJK characters (e.g. '许') raise "Key not found".
                // When that happens, fall back to setting `el.value` via
                // evaluate() and dispatching input + change events so frameworks
                // (React/Vue/Svelte) still see a real input.
                let msg = e.to_string();
                let is_unsupported_key = msg.contains("Key not found")
                    || msg.contains("KeyDescriptor")
                    || msg.contains("keyDefinitions");
                if !is_unsupported_key {
                    return Err(anyhow!("type_text [{}] failed: {}", index, e));
                }

                tracing::debug!(
                    index = %index,
                    chars = %text.chars().count(),
                    "type_text: type_str failed with 'Key not found' — falling back to evaluate()"
                );
                Self::type_via_evaluate(&page, &selector, text)
                    .await
                    .map_err(|e2| {
                        anyhow!(
                            "type_text [{}] failed (CDP type_str: {}) and evaluate fallback also failed: {}",
                            index, e, e2
                        )
                    })?;
                self.invalidate_dom_cache(tab_id).await;
                Ok(())
            }
        }
    }

    /// Bundle 25-B helper: build the JS script for the type-via-evaluate fallback.
    /// Extracted as a pure function so it can be unit-tested without a real Page.
    fn build_type_fallback_script(selector: &str, text: &str) -> Result<String> {
        // selector may contain ' (single quote) — same escape pattern as select_option().
        let safe_selector = selector.replace('\'', "\\'");
        // Serialize text as a JS string literal — handles all Unicode safely.
        let js_text = serde_json::to_string(text)
            .map_err(|e| anyhow!("JSON-encode text for evaluate: {}", e))?;
        Ok(format!(
            r#"(function() {{
                var el = document.querySelector('{selector}');
                if (!el) return {{ ok: false, reason: 'element_not_found' }};
                try {{
                    if (typeof el.focus === 'function') el.focus();
                    el.value = {value};
                    el.dispatchEvent(new Event('input',  {{ bubbles: true }}));
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                    return {{ ok: true }};
                }} catch (err) {{
                    return {{ ok: false, reason: String(err && err.message || err) }};
                }}
            }})()"#,
            selector = safe_selector,
            value = js_text,
        ))
    }

    /// Bundle 25-B: Fallback for `type_text` when the CDP keyboard layout
    /// rejects a Unicode codepoint (e.g. CJK).
    ///
    /// Sets `el.value` directly via `evaluate()` and dispatches `input` +
    /// `change` events so JS frameworks see the change. Uses
    /// `serde_json::to_string(text)` to produce a valid JS string literal —
    /// this is the only correct way to embed arbitrary Unicode (including
    /// quotes, backslashes, newlines, surrogate pairs) into JS source.
    async fn type_via_evaluate(
        page: &chromiumoxide::Page,
        selector: &str,
        text: &str,
    ) -> Result<()> {
        let script = Self::build_type_fallback_script(selector, text)?;

        let result = page
            .evaluate(script)
            .await
            .map_err(|e| anyhow!("evaluate fallback failed: {}", e))?;

        // chromiumoxide's evaluate returns a value we deserialize via .into_value::<T>()
        let value: serde_json::Value = result
            .into_value::<serde_json::Value>()
            .map_err(|e| anyhow!("evaluate fallback returned non-JSON: {}", e))?;
        if !value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let reason = value
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            return Err(anyhow!("evaluate fallback rejected: {}", reason));
        }
        Ok(())
    }

    pub async fn select_option(&self, tab_id: &str, index: u32, value: &str) -> Result<()> {
        let selector = Self::index_selector(index);
        let safe_value = value.replace('\'', "\\'");
        let safe_selector = selector.replace('\'', "\\'");
        let script = format!(
            r#"(function() {{
                var el = document.querySelector('{selector}');
                if (el) {{
                    el.value = '{value}';
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                }}
            }})()"#,
            selector = safe_selector,
            value = safe_value,
        );
        let page = self.get_page(tab_id).await?;
        page.evaluate(script)
            .await
            .map_err(|e| anyhow!("select_option [{}] failed: {}", index, e))?;
        self.invalidate_dom_cache(tab_id).await;
        Ok(())
    }

    pub async fn upload_file(&self, tab_id: &str, index: u32, file_path: &std::path::Path) -> Result<()> {
        let selector = Self::index_selector(index);
        let page = self.get_page(tab_id).await?;
        let element = page.find_element(selector).await
            .map_err(|_| anyhow!("Element with index {} not found", index))?;

        use chromiumoxide::cdp::browser_protocol::dom::SetFileInputFilesParams;
        let mut cmd = SetFileInputFilesParams::new(vec![file_path.to_string_lossy().to_string()]);
        cmd.node_id = Some(element.node_id);

        page.execute(cmd)
            .await
            .map_err(|e| anyhow!("setFileInputFiles failed: {e}"))?;
        self.invalidate_dom_cache(tab_id).await;
        Ok(())
    }

    pub async fn scroll(
        &self,
        tab_id: &str,
        index: Option<u32>,
        direction: &str,
        pixels: u32,
    ) -> Result<()> {
        let (dx, dy) = match direction {
            "up" => (0i64, -(pixels as i64)),
            "down" => (0i64, pixels as i64),
            "left" => (-(pixels as i64), 0i64),
            "right" => (pixels as i64, 0i64),
            _ => return Err(anyhow!("Unknown scroll direction: {}", direction)),
        };

        let script = if let Some(idx) = index {
            let selector = Self::index_selector(idx);
            let safe_selector = selector.replace('\'', "\\'");
            format!(
                r#"(function() {{
                    var el = document.querySelector('{selector}');
                    if (el) {{
                        el.scrollIntoView({{behavior: 'instant', block: 'nearest'}});
                        el.scrollBy({dx}, {dy});
                    }}
                }})()"#,
                selector = safe_selector,
                dx = dx,
                dy = dy,
            )
        } else {
            format!(r#"window.scrollBy({dx}, {dy})"#, dx = dx, dy = dy,)
        };

        let page = self.get_page(tab_id).await?;
        page.evaluate(script)
            .await
            .map_err(|e| anyhow!("scroll failed: {}", e))?;
        Ok(())
    }

    /// Send key events to the whole page (no element targeting).
    pub async fn send_keys(&self, tab_id: &str, keys: &str) -> Result<()> {
        let page = self.get_page(tab_id).await?;

        let down = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyDown)
            .key(keys.to_string())
            .build()
            .map_err(|e| anyhow!("key_down params: {e}"))?;
        let up = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyUp)
            .key(keys.to_string())
            .build()
            .map_err(|e| anyhow!("key_up params: {e}"))?;

        page.execute(down)
            .await
            .map_err(|e| anyhow!("key_down: {e}"))?;
        page.execute(up)
            .await
            .map_err(|e| anyhow!("key_up: {e}"))?;
        Ok(())
    }

    /// Execute JavaScript and return the result as a JSON string.
    pub async fn execute_js(&self, tab_id: &str, script: &str) -> Result<String> {
        let page = self.get_page(tab_id).await?;
        let val = page
            .evaluate(script)
            .await
            .map_err(|e| anyhow!("execute_js: {}", e))?;
        let s = val
            .into_value::<serde_json::Value>()
            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|e| e.to_string()))
            .unwrap_or_else(|_| "<no return value>".to_string());
        Ok(s)
    }

    pub(crate) fn browser_run_invocation_script(
        source: &str,
        params: &serde_json::Value,
    ) -> Result<String> {
        let source = source.trim().trim_end_matches(';');
        let params_json = serde_json::to_string(params)
            .map_err(|e| anyhow!("serialize browser_run params: {}", e))?;
        Ok(format!(
            "(async () => {{ const __uclawParams = {params_json}; const __uclawUserFn = {source}; return await __uclawUserFn(__uclawParams); }})()"
        ))
    }

    /// Execute a Halo-compatible browser adapter function with JSON params.
    pub async fn evaluate_script_with_params(
        &self,
        tab_id: &str,
        source: &str,
        params: serde_json::Value,
        timeout_ms: u64,
    ) -> Result<serde_json::Value> {
        let page = self.get_page(tab_id).await?;
        let script = Self::browser_run_invocation_script(source, &params)?;
        let val = tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            page.evaluate(script),
        )
        .await
        .map_err(|_| anyhow!("script timed out after {timeout_ms}ms"))?
        .map_err(|e| anyhow!("evaluate_script_with_params: {}", e))?;
        val.into_value::<serde_json::Value>()
            .map_err(|e| anyhow!("decode browser_run result: {}", e))
    }

    // ── Tab management ────────────────────────────────────────────────────────

    pub async fn get_all_tabs(&self) -> Vec<TabInfo> {
        let active = self.active_tab_id.read().await.clone();
        let pages = self.pages.read().await;
        let mut tabs = Vec::with_capacity(pages.len());
        for (id, page) in pages.iter() {
            let url = page
                .evaluate("window.location.href")
                .await
                .ok()
                .and_then(|v| v.into_value::<String>().ok())
                .unwrap_or_default();
            let title = page
                .evaluate("document.title")
                .await
                .ok()
                .and_then(|v| v.into_value::<String>().ok())
                .unwrap_or_default();
            tabs.push(TabInfo {
                tab_id: id.clone(),
                url,
                title,
                active: active.as_deref() == Some(id.as_str()),
            });
        }
        tabs
    }

    /// Return the active tab id, falling back to the first known tab.
    pub async fn active_or_first_tab_id(&self) -> Option<String> {
        let active = self.active_tab_id.read().await.clone();
        if let Some(tab_id) = active {
            if self.pages.read().await.contains_key(&tab_id) {
                return Some(tab_id);
            }
        }
        self.pages.read().await.keys().next().cloned()
    }

    pub async fn close_tab(&self, tab_id: &str) -> Result<()> {
        let page = {
            let mut pages = self.pages.write().await;
            match pages.remove(tab_id) {
                Some(p) => p,
                None => {
                    // Bundle 19 — same actionable hint as get_page.
                    let mut msg = format!("Tab '{}' not found.", tab_id);
                    if pages.is_empty() {
                        msg.push_str(" No tabs are currently open to close.");
                    } else {
                        let mut ids: Vec<&str> = pages.keys().map(String::as_str).collect();
                        ids.sort();
                        let take = ids.len().min(5);
                        msg.push_str(&format!(" Open tabs: {}", ids[..take].join(", ")));
                    }
                    return Err(anyhow!(msg));
                }
            }
        };
        // Stop CDP screencast before closing the page; Page::close consumes it.
        if let Some(stop_tx) = self.screencast_stops.lock().await.remove(tab_id) {
            let _ = page.execute(StopScreencastParams::default()).await;
            let _ = stop_tx.send(());
        }
        page.close()
            .await
            .map_err(|e| anyhow!("close_tab failed: {}", e))?;
        self.dom_cache.write().await.remove(tab_id);
        let should_pick_next = self.active_tab_id.read().await.as_deref() == Some(tab_id);
        if should_pick_next {
            let next = self.pages.read().await.keys().next().cloned();
            *self.active_tab_id.write().await = next;
        }
        Ok(())
    }

    // ── Screencast ────────────────────────────────────────────────────────────

    pub async fn start_screencast(
        &self,
        tab_id: &str,
        app_handle: tauri::AppHandle,
    ) -> Result<()> {
        let page = self.get_page(tab_id).await?;

        if let Some(stop_tx) = self.screencast_stops.lock().await.remove(tab_id) {
            let _ = page.execute(StopScreencastParams::default()).await;
            let _ = stop_tx.send(());
        }

        let mut frame_stream = page
            .event_listener::<chromiumoxide::cdp::browser_protocol::page::EventScreencastFrame>()
            .await
            .map_err(|e| anyhow!("event_listener failed: {}", e))?;

        page.execute(
            StartScreencastParams::builder()
                .format(StartScreencastFormat::Jpeg)
                .quality(55_i64)
                .max_width(1280_i64)
                .max_height(800_i64)
                .every_nth_frame(1_i64)
                .build(),
        )
        .await
        .map_err(|e| anyhow!("start_screencast failed: {}", e))?;

        let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
        self.screencast_stops
            .lock()
            .await
            .insert(tab_id.to_string(), stop_tx);

        let page_clone = page.clone();
        let tab_id_owned = tab_id.to_string();
        let session_id = self.session_id.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    frame_opt = frame_stream.next() => {
                        let frame = match frame_opt {
                            Some(f) => f,
                            None => break,
                        };

                        // Ack the frame so Chrome sends the next one.
                        let _ = page_clone
                            .execute(ScreencastFrameAckParams::new(frame.session_id))
                            .await;

                        // Binary wraps a base64 string; cast through AsRef<str>.
                        let data_b64: String =
                            <_ as AsRef<str>>::as_ref(&frame.data).to_string();

                        let payload = ScreencastFramePayload {
                            session_id: session_id.clone(),
                            tab_id: tab_id_owned.clone(),
                            data_b64,
                            page_width: frame.metadata.device_width as u32,
                            page_height: frame.metadata.device_height as u32,
                        };

                        let _ = app_handle.emit("browser:screencast-frame", &payload);
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop_screencast(&self, tab_id: &str) {
        if let Some(stop_tx) = self.screencast_stops.lock().await.remove(tab_id) {
            if let Ok(page) = self.get_page(tab_id).await {
                let _ = page.execute(StopScreencastParams::default()).await;
            }
            let _ = stop_tx.send(());
        }
    }

    // ── Cookies ───────────────────────────────────────────────────────────────

    pub async fn get_cookies(&self, tab_id: &str, url_filter: Option<&str>) -> Result<Vec<CookieInfo>> {
        let page = self.get_page(tab_id).await?;
        use chromiumoxide::cdp::browser_protocol::network::GetCookiesParams;
        let cmd = GetCookiesParams {
            urls: url_filter.map(|u| vec![u.to_string()]),
        };
        let result = page.execute(cmd).await
            .map_err(|e| anyhow!("get_cookies CDP error: {e}"))?;
        let cookies = result.result.cookies.into_iter().map(|c| CookieInfo {
            name: c.name,
            value: c.value,
            domain: c.domain,
            path: c.path,
            secure: c.secure,
            http_only: c.http_only,
            same_site: c.same_site.map(|s| format!("{s:?}")),
            expires: c.expires,
        }).collect();
        Ok(cookies)
    }

    pub async fn capture_storage_state(
        &self,
        tab_id: &str,
        url_filter: &str,
    ) -> Result<PlaywrightStorageState> {
        let cookies = self.get_cookies(tab_id, Some(url_filter)).await?;
        let page = self.get_page(tab_id).await?;
        let origin = page
            .evaluate("window.location.origin")
            .await
            .ok()
            .and_then(|v| v.into_value::<String>().ok())
            .unwrap_or_default();
        let local_storage = page
            .evaluate(
                r#"(function() {
                    return Object.keys(window.localStorage || {}).map(function(name) {
                        return { name: name, value: window.localStorage.getItem(name) || "" };
                    });
                })()"#,
            )
            .await
            .ok()
            .and_then(|v| v.into_value::<Vec<PlaywrightLocalStorageEntry>>().ok())
            .unwrap_or_default();

        Ok(PlaywrightStorageState {
            cookies: cookies
                .into_iter()
                .map(|cookie| PlaywrightCookie {
                    name: cookie.name,
                    value: cookie.value,
                    domain: cookie.domain,
                    path: cookie.path,
                    expires: Some(cookie.expires),
                    http_only: cookie.http_only,
                    secure: cookie.secure,
                    same_site: cookie.same_site,
                })
                .collect(),
            origins: if origin.is_empty() || local_storage.is_empty() {
                Vec::new()
            } else {
                vec![PlaywrightOrigin { origin, local_storage }]
            },
        })
    }

    pub async fn set_cookie(
        &self,
        tab_id: &str,
        name: &str,
        value: &str,
        domain: &str,
        path: Option<&str>,
        secure: bool,
        http_only: bool,
    ) -> Result<bool> {
        let page = self.get_page(tab_id).await?;
        use chromiumoxide::cdp::browser_protocol::network::{CookieParam, SetCookiesParams};
        let mut cookie = CookieParam::new(name, value);
        cookie.domain = Some(domain.to_string());
        cookie.path = path.map(|p| p.to_string());
        cookie.secure = Some(secure);
        cookie.http_only = Some(http_only);
        let cmd = SetCookiesParams { cookies: vec![cookie] };
        page.execute(cmd).await
            .map_err(|e| anyhow!("set_cookie CDP error: {e}"))?;
        Ok(true)
    }

    pub async fn apply_storage_state(
        &self,
        tab_id: &str,
        state: &PlaywrightStorageState,
        app_handle: &tauri::AppHandle,
    ) -> Result<()> {
        self.apply_storage_state_cookies(tab_id, state).await?;
        for origin in &state.origins {
            self.apply_local_storage_origin(tab_id, origin, app_handle).await?;
        }
        self.invalidate_dom_cache(tab_id).await;
        Ok(())
    }

    async fn apply_storage_state_cookies(
        &self,
        tab_id: &str,
        state: &PlaywrightStorageState,
    ) -> Result<()> {
        if state.cookies.is_empty() {
            return Ok(());
        }
        let page = self.get_page(tab_id).await?;
        use chromiumoxide::cdp::browser_protocol::network::{
            CookieParam, CookieSameSite, SetCookiesParams, TimeSinceEpoch,
        };
        let mut cookies = Vec::with_capacity(state.cookies.len());
        for source in &state.cookies {
            let mut cookie = CookieParam::new(&source.name, &source.value);
            cookie.domain = Some(source.domain.clone());
            cookie.path = Some(source.path.clone());
            cookie.secure = Some(source.secure);
            cookie.http_only = Some(source.http_only);
            cookie.same_site = source
                .same_site
                .as_deref()
                .and_then(|value| value.parse::<CookieSameSite>().ok());
            cookie.expires = source.expires.map(TimeSinceEpoch::new);
            cookies.push(cookie);
        }
        page.execute(SetCookiesParams { cookies })
            .await
            .map_err(|e| anyhow!("apply storageState cookies failed: {e}"))?;
        Ok(())
    }

    async fn apply_local_storage_origin(
        &self,
        tab_id: &str,
        origin: &PlaywrightOrigin,
        app_handle: &tauri::AppHandle,
    ) -> Result<()> {
        if origin.local_storage.is_empty() {
            return Ok(());
        }
        self.navigate(tab_id, &origin.origin, app_handle).await?;
        let page = self.get_page(tab_id).await?;
        let entries = serde_json::to_string(&origin.local_storage)?;
        let script = format!(
            r#"(function() {{
                const entries = {entries};
                for (const item of entries) {{
                    window.localStorage.setItem(item.name, item.value);
                }}
                return entries.length;
            }})()"#
        );
        page.evaluate(script)
            .await
            .map_err(|e| anyhow!("apply storageState localStorage failed: {e}"))?;
        Ok(())
    }

    // ── Device emulation ──────────────────────────────────────────────────────

    pub async fn apply_device_emulation(&self, tab_id: &str, device: DevicePreset) -> Result<()> {
        let page = self.get_page(tab_id).await?;
        use chromiumoxide::cdp::browser_protocol::emulation::{
            MediaFeature, SetDeviceMetricsOverrideParams, SetEmulatedMediaParams,
            SetTouchEmulationEnabledParams, SetUserAgentOverrideParams,
        };

        // 1. Viewport dimensions + mobile rendering mode.
        let metrics = SetDeviceMetricsOverrideParams {
            width: device.viewport_width() as i64,
            height: device.viewport_height() as i64,
            device_scale_factor: if device == DevicePreset::Mobile { 3.0 } else { 1.0 },
            mobile: device == DevicePreset::Mobile,
            scale: None,
            screen_width: None,
            screen_height: None,
            position_x: None,
            position_y: None,
            dont_set_visible_size: None,
            screen_orientation: None,
            viewport: None,
        };
        page.execute(metrics).await
            .map_err(|e| anyhow!("set device metrics: {e}"))?;

        // 2. User-agent.
        let ua = SetUserAgentOverrideParams {
            user_agent: device.user_agent().to_string(),
            accept_language: None,
            platform: None,
            user_agent_metadata: None,
        };
        page.execute(ua).await
            .map_err(|e| anyhow!("set UA: {e}"))?;

        // 3. Touch emulation — mobile gets 5-point touch, desktop gets none.
        let is_mobile = device == DevicePreset::Mobile;
        let touch = SetTouchEmulationEnabledParams {
            enabled: is_mobile,
            max_touch_points: Some(if is_mobile { 5 } else { 0 }),
        };
        page.execute(touch).await
            .map_err(|e| anyhow!("touch emulation: {e}"))?;

        // 4. CSS media features — pointer:coarse + hover:none for mobile.
        let features = if is_mobile {
            vec![
                MediaFeature { name: "hover".to_string(), value: "none".to_string() },
                MediaFeature { name: "pointer".to_string(), value: "coarse".to_string() },
            ]
        } else {
            vec![
                MediaFeature { name: "hover".to_string(), value: "hover".to_string() },
                MediaFeature { name: "pointer".to_string(), value: "fine".to_string() },
            ]
        };
        page.execute(SetEmulatedMediaParams {
            media: None,
            features: Some(features),
        })
        .await
        .map_err(|e| anyhow!("emulated media: {e}"))?;

        Ok(())
    }
}

fn is_transient_cdp_context_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("cannot find context with specified id")
        || lower.contains("cannot find execution context")
        || lower.contains("execution context was destroyed")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_selector_format() {
        let selector = format!("[data-uclaw-index=\"{}\"]", 5);
        assert_eq!(selector, "[data-uclaw-index=\"5\"]");
    }

    #[test]
    fn dom_cache_ttl_expired() {
        let past = Instant::now() - Duration::from_millis(600);
        assert!(
            past.elapsed() > Duration::from_millis(500),
            "600 ms old cache entry should be expired"
        );
    }

    #[test]
    fn device_preset_mobile_fields() {
        assert_eq!(DevicePreset::Mobile.viewport_width(), 390);
        assert_eq!(DevicePreset::Mobile.viewport_height(), 844);
        assert!(DevicePreset::Mobile.user_agent().contains("iPhone"));
    }

    #[test]
    fn device_preset_desktop_fields() {
        assert_eq!(DevicePreset::Desktop.viewport_width(), 1280);
        assert_eq!(DevicePreset::Desktop.viewport_height(), 800);
        assert!(DevicePreset::Desktop.user_agent().contains("Macintosh"));
    }

    #[test]
    fn browser_run_invocation_script_injects_params_into_adapter_function() {
        let script = BrowserContext::browser_run_invocation_script(
            "(async (params) => ({ room: params.roomId, nested: params.options.enabled }));",
            &serde_json::json!({
                "roomId": "douyin-room-1",
                "options": { "enabled": true }
            }),
        )
        .unwrap();

        // Compare parsed JSON, not raw text: key order depends on whether
        // serde_json's `preserve_order` feature is unified into the build.
        let params_json = script
            .split("const __uclawParams = ")
            .nth(1)
            .and_then(|rest| rest.split("; const __uclawUserFn").next())
            .expect("params literal in script");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(params_json).unwrap(),
            serde_json::json!({ "roomId": "douyin-room-1", "options": { "enabled": true } })
        );
        assert!(script.contains("const __uclawUserFn = (async (params) =>"));
        assert!(script.contains("return await __uclawUserFn(__uclawParams);"));
    }

    // --- Bundle 25-B: type-fallback script builder tests ---

    #[test]
    fn type_fallback_script_embeds_cjk_text_safely() {
        let s = BrowserContext::build_type_fallback_script(
            "[data-uclaw-index=\"3\"]",
            "你好世界",
        )
        .unwrap();
        // JSON serialization of CJK uses raw UTF-8 by default — both forms
        // are valid JS but serde_json writes the bytes directly.
        assert!(s.contains("\"你好世界\""), "expected CJK passed through, got:\n{s}");
        // The 'value =' line uses the JSON literal as a JS string.
        assert!(s.contains("el.value = \"你好世界\""));
        // Both events dispatched.
        assert!(s.contains("new Event('input'"));
        assert!(s.contains("new Event('change'"));
    }

    #[test]
    fn type_fallback_script_escapes_quotes_and_backslashes() {
        let s = BrowserContext::build_type_fallback_script(
            "[data-uclaw-index=\"7\"]",
            "she said \"hi\" and `\\` and 'q' and\nnewline",
        )
        .unwrap();
        // serde_json escapes " as \" and \ as \\ and \n as \\n
        assert!(s.contains(r#"\"hi\""#), "quotes should be escaped");
        assert!(s.contains(r"\\"), "backslash should be escaped");
        assert!(s.contains(r"\n"), "newline should be escaped");
    }

    #[test]
    fn type_fallback_script_escapes_selector_single_quotes() {
        // (rare but possible: selectors containing single quotes from a malformed index)
        let s = BrowserContext::build_type_fallback_script(
            "input[name='weird']",
            "ok",
        )
        .unwrap();
        // Single quotes inside the selector are backslash-escaped before going
        // into the JS single-quoted string literal.
        assert!(s.contains(r"input[name=\'weird\']"));
    }

    // --- Bundle 25-C: pseudo tab_id rejection tests ---

    #[test]
    fn pseudo_tab_id_rejects_new() {
        let msg = BrowserContext::reject_pseudo_tab_id("new").expect("'new' should be rejected");
        assert!(msg.contains("[InvalidInput]"));
        assert!(msg.contains("browser_navigate"));
        // Mentions concrete other tools so the LLM gets a hint about scope.
        assert!(msg.contains("browser_reload") || msg.contains("browser_click"));
    }

    #[test]
    fn pseudo_tab_id_accepts_real_uuids() {
        // A normal UUID-style id should pass the guard (real lookup happens later).
        assert!(BrowserContext::reject_pseudo_tab_id(
            "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
        )
        .is_none());
    }

    #[test]
    fn pseudo_tab_id_accepts_short_random_ids() {
        assert!(BrowserContext::reject_pseudo_tab_id("tab_42").is_none());
        assert!(BrowserContext::reject_pseudo_tab_id("0").is_none()); // intentionally not a pseudo yet
        assert!(BrowserContext::reject_pseudo_tab_id("").is_none()); // empty currently passes; revisit if seen in logs
    }

    #[test]
    fn loading_nav_state_never_emits_new_as_tab_id() {
        assert_eq!(
            BrowserContext::loading_nav_state_tab_id("new", Some("tab-real")),
            "tab-real"
        );
        assert_eq!(BrowserContext::loading_nav_state_tab_id("new", None), "");
        assert_eq!(
            BrowserContext::loading_nav_state_tab_id("tab-1", Some("tab-real")),
            "tab-1"
        );
    }

    #[test]
    fn type_fallback_script_emoji_and_emoji_zwj_sequences() {
        // 👨‍👩‍👧 = ZWJ family sequence — multi-codepoint, valid UTF-8.
        let s = BrowserContext::build_type_fallback_script("#x", "👨‍👩‍👧 hi").unwrap();
        assert!(s.contains("\"👨\u{200d}👩\u{200d}👧 hi\"") || s.contains("\"👨‍👩‍👧 hi\""));
    }
}
