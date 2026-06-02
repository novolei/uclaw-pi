# Memory Integration — Phase 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (user request). One implementer subagent per PR-group; controller reviews at PR boundary. Steps use checkbox (`- [ ]`).

**Goal:** P3 成长——建 mem.md 的头号理念:一个 turn-count 触发的统一 `ReflectionService`,周期蒸馏出 (a) Reflection{insight,confidence}(事件→洞见)和 (b) user_model(facts→人格模型),回注 pi prompt。补齐 Event→Fact→**Pattern→Model** 链的后两环。

**Architecture:** 一个新模块 `memory_graph/reflection_service.rs`,`run_once(app)` 跑两遍蒸馏(reflection + promotion),用 `state.learning_llm`(`MemoryOsLlm::complete_text`)+ 既有 daily-budget gate。**触发=turn-count**(事件驱动,非壁钟循环):`engine_sink::persist_assistant` 每 agent 回合给 `AppState` 的 `AtomicU64` 计数 +1,`% N == 0` 时 fire-and-forget spawn `run_once`。两张新表(V57)。注入走 P2-② 的 `PiPromptContext`(加 `reflections` / `user_model` 字段)。

**Tech Stack:** Rust (Tauri v2) · `cargo test --lib` · 分支 `pi/memory-integration-p3`。
**决策(已拍板):** 触发=按回合数(N=20);架构=统一 ReflectionService。
**复用模式:** LLM 调用照抄 `proactive/daily_summary.rs:142-196`(`provider.complete` 或 `state.learning_llm.complete_text`);budget gate 照抄 learning extractor(`cost_store::today_learning_tokens` vs `learning_llm_daily_token_budget`);迁移照抄 migrations.rs 现有 V-block(下一个空闲=**V57**)。

**验证:** `cargo build 2>&1 | grep -E "^error"`(空)· warnings ≤ 基线 47 · `cargo test --lib <filter>`。

---

## File Structure

| 文件 | 改动 | PR |
|---|---|---|
| `src-tauri/src/db/migrations.rs` | V57:`reflections` + `user_model` 两张表(additive) | P3-① |
| `src-tauri/src/memory_graph/reflection_service.rs` | **新**:ReflectionService(`run_once` = reflection pass;P3-② 加 promotion) | P3-①/② |
| `src-tauri/src/memory_graph/mod.rs` | `pub mod reflection_service;` | P3-① |
| `src-tauri/src/app.rs` | AppState 加 `reflection_turn_counter: Arc<AtomicU64>` | P3-① |
| `src-tauri/src/engine_sink.rs` | `persist_assistant` 末尾:计数 +1,`% N==0` spawn `run_once` | P3-① |
| `src-tauri/src/agent/memory_context.rs` | `PiPromptContext` 加 `reflections`(P3-①)/`user_model`(P3-②)字段 + cap | P3-①/② |
| `src-tauri/src/tauri_commands.rs` | 两个 pi site 读最近 reflections(P3-①)/user_model(P3-②)→ PiPromptContext | P3-①/② |

---

## PR P3-① — ReflectionService(reflection pass)+ 表 + turn-count 触发 + 注入

### Task 1: V57 迁移(reflections + user_model 两表)

**Files:** Modify `src-tauri/src/db/migrations.rs`

- [ ] **Step 1:** 照抄现有 V-block 模式,加 V57(additive,`IF NOT EXISTS`):
```sql
-- reflections: periodic insights distilled from recent events (mem.md Reflection Agent)
CREATE TABLE IF NOT EXISTS reflections (
    id                 TEXT PRIMARY KEY,
    insight            TEXT NOT NULL,
    confidence         REAL NOT NULL DEFAULT 0.5,
    source_event_count INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_reflections_created ON reflections(created_at DESC);
-- user_model: single distilled persona/preference model (Pattern→Model layer)
CREATE TABLE IF NOT EXISTS user_model (
    id          TEXT PRIMARY KEY,   -- singleton: fixed id "default"
    summary     TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
```
注册进 migration runner(照 V56 的注册方式)。**先 grep `V56` 确认确切注册写法 + 没有别的 open PR 占用 V57**(CLAUDE.md migration registry 规则)。

- [ ] **Step 2:** `cd src-tauri && cargo build 2>&1 | grep -E "^error"`(空)。迁移单测(若有 migration test harness,加一个 V57 apply 测试;否则 build 绿即可)。
- [ ] **Step 3:** Commit:
```bash
git add src-tauri/src/db/migrations.rs
git commit -m "feat(db): V57 — reflections + user_model tables (Phase 3 growth)"
```

### Task 2: ReflectionService 模块 + reflection pass(读事件→LLM→insert)

**Files:** Create `src-tauri/src/memory_graph/reflection_service.rs`; Modify `src-tauri/src/memory_graph/mod.rs`(`pub mod reflection_service;`)

实现者:LLM 调用 + budget gate **照抄** `proactive/daily_summary.rs:142-196`(它演示了 `state.provider_service.get_active_llm_config()` → `provider.complete(messages, ...)`;或用更轻的 `state.learning_llm.as_ref()` → `MemoryOsLlm::complete_text(...)` —— 先 READ `memory_graph/memory_os_llm.rs:62` 的 `complete_text` 确切签名 + cost_tag 参数)。budget gate 照 learning extractor(`cost_store::today_learning_tokens(db)` vs `state.learning_llm_daily_token_budget`,超则 skip)。

- [ ] **Step 1: 写失败测试**(放模块 `#[cfg(test)]`):**只 TDD 可纯测的部分**——reflections 表 CRUD + 解析 LLM 输出 + turn-count 触发判定:
```rust
    #[test]
    fn parse_reflection_extracts_insight_and_confidence() {
        // LLM 被要求输出 JSON {"insight": "...", "confidence": 0.8}
        let (insight, conf) = parse_reflection_output(r#"{"insight":"user is building an agent framework","confidence":0.82}"#);
        assert_eq!(insight, "user is building an agent framework");
        assert!((conf - 0.82).abs() < 1e-6);
        // 不可解析 → confidence 默认 0.5、insight 取原文 trim
        let (i2, c2) = parse_reflection_output("just some prose");
        assert_eq!(i2, "just some prose");
        assert!((c2 - 0.5).abs() < 1e-6);
    }
    #[test]
    fn reflections_store_inserts_and_reads_recent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        apply_reflections_schema(&conn); // test helper applying the V57 reflections DDL
        insert_reflection(&conn, "id1", "insight A", 0.7, 50).unwrap();
        insert_reflection(&conn, "id2", "insight B", 0.9, 60).unwrap();
        let recent = recent_reflections(&conn, 5).unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent.iter().any(|r| r.insight == "insight B"));
    }
    #[test]
    fn should_run_reflection_every_n_turns() {
        assert!(should_run_reflection(20, 20));   // count, n
        assert!(should_run_reflection(40, 20));
        assert!(!should_run_reflection(19, 20));
        assert!(!should_run_reflection(0, 20));    // 0 不触发
    }
```
- [ ] **Step 2:** Run `cd src-tauri && cargo test --lib reflection_service 2>&1 | tail -10` — FAIL(fns 不存在)。
- [ ] **Step 3: 实现** `reflection_service.rs`:
  - `fn parse_reflection_output(s: &str) -> (String, f64)` — serde_json 解析 {insight, confidence};失败回退 (s.trim(), 0.5)。
  - `fn apply_reflections_schema(conn)` / `fn insert_reflection(conn, id, insight, conf, n)` / `struct ReflectionRow { insight, confidence, created_at }` + `fn recent_reflections(conn, limit) -> Vec<ReflectionRow>`。
  - `fn should_run_reflection(count: u64, n: u64) -> bool { n > 0 && count > 0 && count % n == 0 }`。
  - `pub async fn run_once(app: tauri::AppHandle)`:try_state → budget gate(超则 return)→ `state.learning_llm` 取 LLM(None 则 return)→ 读最近 ~50 条 `agent_messages`(role+content,`ORDER BY created_at DESC LIMIT 50`)→ 拼 reflection prompt(要求 JSON {insight, confidence})→ `complete_text` → `parse_reflection_output` → `insert_reflection`(id=uuid)。全程 best-effort,任何错误 log+return,绝不 panic。
- [ ] **Step 4:** Run `cargo test --lib reflection_service 2>&1 | tail -10` — 3 测试 PASS。`cargo build 2>&1 | grep -E "^error"`(空)。
- [ ] **Step 5:** Commit:
```bash
git add src-tauri/src/memory_graph/reflection_service.rs src-tauri/src/memory_graph/mod.rs
git commit -m "feat(reflection): ReflectionService run_once — distill recent turns into reflections"
```

### Task 3: turn-count 触发(AppState 计数 + engine_sink hook)

**Files:** Modify `src-tauri/src/app.rs`(AppState 加字段 + 初始化);`src-tauri/src/engine_sink.rs`(`persist_assistant`)

- [ ] **Step 1:** AppState 加 `pub reflection_turn_counter: std::sync::Arc<std::sync::atomic::AtomicU64>`,构造里 `Arc::new(AtomicU64::new(0))`。
- [ ] **Step 2:** `engine_sink.rs::persist_assistant` 末尾(紧跟 `spawn_bucket_seal_ingest` 之后),仅 agent session 时计数 + 触发:
```rust
        // P3: turn-count trigger — every N agent turns, distill reflections.
        const REFLECTION_EVERY_N_TURNS: u64 = 20;
        if is_agent_session {
            let n = state.reflection_turn_counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if crate::memory_graph::reflection_service::should_run_reflection(n, REFLECTION_EVERY_N_TURNS) {
                let app = self.app.clone();
                tauri::async_runtime::spawn(async move {
                    crate::memory_graph::reflection_service::run_once(app).await;
                });
            }
        }
```
(`is_agent_session` 是 persist_assistant 里已算好的变量;`self.app` 是 AppHandle。)
- [ ] **Step 3:** `cargo build 2>&1 | grep -E "^error"`(空);warnings ≤ 47。
- [ ] **Step 4:** Commit:
```bash
git add src-tauri/src/app.rs src-tauri/src/engine_sink.rs
git commit -m "feat(reflection): turn-count trigger — spawn run_once every 20 agent turns"
```

### Task 4: 注入——pi prompt 加 reflections 维度

**Files:** Modify `src-tauri/src/agent/memory_context.rs`(`PiPromptContext`);`src-tauri/src/tauri_commands.rs`(两个 pi site)

- [ ] **Step 1: 改 PiPromptContext + cap 测试**:`PiPromptContext` 加 `pub reflections: Option<String>` 字段(放在 `recall` 之后、`gbrain` 之前——优先级:facts→genes→reflections→recall→gbrain?**实现者定**:reflections 应高于 recall,因为是蒸馏过的高价值);`compose` 的 `dims` 数组加 `(self.reflections, CAP_REFLECTIONS=2_000)`。更新 `pi_context_orders_by_priority_and_skips_empty` 测试断言含 reflections 顺序。
- [ ] **Step 2:** Run `cargo test --lib pi_context 2>&1 | tail` — 改后的测试 FAIL→实现→PASS。
- [ ] **Step 3: 两个 pi site 注入**:每个 site 在建 `PiPromptContext{...}` 处,加 `reflections: { ... read recent reflections ... }`:
```rust
            let reflections_block = {
                let recent = state.db.lock().ok()
                    .and_then(|c| crate::memory_graph::reflection_service::recent_reflections(&c, 3).ok())
                    .unwrap_or_default();
                if recent.is_empty() { None } else {
                    let mut s = String::from("## Recent Reflections\n");
                    for r in &recent { s.push_str(&format!("- ({:.2}) {}\n", r.confidence, r.insight)); }
                    Some(s)
                }
            };
```
然后 `PiPromptContext { facts, genes, reflections: reflections_block, recall, gbrain }.compose(12_000)`。两个 site 都改。
- [ ] **Step 4:** `cargo build 2>&1 | grep -E "^error"`(空);`cargo test --lib "agent::memory_context" 2>&1 | grep "test result"`(过);warnings ≤ 47。
- [ ] **Step 5:** Commit:
```bash
git add src-tauri/src/agent/memory_context.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(reflection): inject recent reflections into pi prompt (PiPromptContext.reflections)"
```

**→ P3-① 完成。** 验收:`cargo test --lib reflection_service` + `pi_context` 全过;手测:聊 20 轮后 `~/.uclaw-pi/uclaw.db` `reflections` 表出现行,之后 prompt 含 `## Recent Reflections`。

---

## PR P3-② — Promotion pass(facts→user_model)+ 注入

### Task 1: ReflectionService 加 run_promotion + run_once 调它

**Files:** Modify `src-tauri/src/memory_graph/reflection_service.rs`

- [ ] **Step 1: 写失败测试**:user_model 表 upsert/read:
```rust
    #[test]
    fn user_model_upserts_single_row() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        apply_user_model_schema(&conn);
        upsert_user_model(&conn, "Ryan, engineer, Rust").unwrap();
        upsert_user_model(&conn, "Ryan Liu, Apple PKG PD, Rust+SwiftUI").unwrap(); // 覆盖
        let m = get_user_model(&conn).unwrap();
        assert_eq!(m.as_deref(), Some("Ryan Liu, Apple PKG PD, Rust+SwiftUI"));
    }
```
- [ ] **Step 2:** FAIL → 实现:`apply_user_model_schema` / `upsert_user_model(conn, summary)`(id="default" 固定,`INSERT ... ON CONFLICT(id) DO UPDATE`)/ `get_user_model(conn) -> Option<String>`;`async fn run_promotion(state)`:budget gate → 读 `user_profile_facets`(class/name/value)+ `memory_nodes WHERE kind='user_profile'`(title)→ 拼 promotion prompt → `complete_text` → `upsert_user_model`。在 `run_once` 末尾调 `run_promotion`(reflection 之后)。
- [ ] **Step 3:** `cargo test --lib reflection_service 2>&1 | tail` — PASS;build 绿。
- [ ] **Step 4:** Commit:
```bash
git add src-tauri/src/memory_graph/reflection_service.rs
git commit -m "feat(reflection): run_promotion — distill facts + profile nodes into user_model"
```

### Task 2: 注入——pi prompt 加 user_model 维度

**Files:** Modify `src-tauri/src/agent/memory_context.rs`;`src-tauri/src/tauri_commands.rs`(两 site)

- [ ] **Step 1:** `PiPromptContext` 加 `pub user_model: Option<String>`(优先级:facts→user_model→genes→reflections→recall→gbrain?**实现者定**:user_model 应很靠前——它是"你是谁"的核心,小而高价值);`compose` dims 加 `(self.user_model, CAP_USER_MODEL=1_200)`;更新顺序测试。
- [ ] **Step 2:** test FAIL→实现→PASS。
- [ ] **Step 3:** 两个 pi site:`user_model: { state.db.lock().ok().and_then(|c| get_user_model(&c).ok()).flatten().map(|m| format!("## User Model\n{m}")) }`,塞进 `PiPromptContext{...}`。两 site 都改。
- [ ] **Step 4:** build 绿;`cargo test --lib "agent::memory_context"` 过;warnings ≤ 47。
- [ ] **Step 5:** Commit:
```bash
git add src-tauri/src/agent/memory_context.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(reflection): inject user_model into pi prompt (PiPromptContext.user_model)"
```

**→ P3-② 完成。**

---

## Self-Review
- **决策覆盖**:turn-count 触发=P3-① T3(`should_run_reflection` + engine_sink hook);统一 ReflectionService=P3-① 建模块、P3-② 加 run_promotion 进同一 `run_once`。Event→Fact(P1-③ extractor)→Pattern/Model(P3-② promotion→user_model)+ Reflection(P3-①)。✓
- **类型一致**:`run_once`/`recent_reflections`/`should_run_reflection`/`get_user_model`/`upsert_user_model` 跨 task 引用名一致;`PiPromptContext` 新字段 `reflections`(P3-①)/`user_model`(P3-②)。✓
- **可测性**:纯逻辑(parse、表 CRUD、触发判定、compose 字段)走 TDD;LLM orchestration(run_once/run_promotion 的 LLM 调用)build-green + 手测(LLM 难单测)。
- **留给实现者核定**:`MemoryOsLlm::complete_text` 确切签名(memory_os_llm.rs:62)+ cost_tag;daily_summary 的 LLM 取法二选一;V57 注册写法(照 V56);budget 字段名(`learning_llm_daily_token_budget`)。这些是模式复用点,实现时 READ 真实签名。

## Execution Handoff
Subagent-Driven:P3-① 一个 implementer subagent(4 task),P3-② 一个(2 task);controller PR 边界 review + 开一个 Phase-3 PR(两组 bisectable commits)。
