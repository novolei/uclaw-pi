# Memory Integration — Phase 4 (daydream) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (user request). One implementer subagent; controller reviews at PR boundary. Steps use checkbox (`- [ ]`).

**Goal:** P4 收尾——给 `ReflectionService` 加一个 **daydream** pass:每 ~100 agent 回合(reflection 的 1/5 频率),对**随机记忆**发散自由联想 → 一个新假设/连接 → 存 `daydreams` 表 + emit `agent:daydream` UI 事件(**不注入 prompt**)。这是最后一项 north-star。

**Architecture:** 复用 ReflectionService 的 turn-count 调度 + LLM/budget 模式。`run_once` 加一个 turn_count 参数,`should_run_reflection(turn_count, 100)` 为真时跑 `run_daydream`。daydream = 随机 memory_nodes → 发散 prompt → `daydreams` 表 + `app.emit("agent:daydream", ...)`。决策:**UI 浮现**(存 + emit,不注入)。

**Tech Stack:** Rust (Tauri v2) · `cargo test --lib` · 分支 `pi/memory-integration-p4`。
**复用:** LLM/budget/borrow-safe 模式照抄 `reflection_service.rs` 现有 `run_once`/`run_promotion`;emit 照抄 `tauri::Emitter` 的 `app.emit(...)`(load_context 里 `agent:memory-recall` 是例子);迁移下一个空闲=**V58**。

**验证:** `cargo build 2>&1 | grep -E "^error"`(空)· warnings ≤ 47 · `cargo test --lib reflection_service`。

---

## File Structure

| 文件 | 改动 |
|---|---|
| `src-tauri/src/db/migrations.rs` | V58:`daydreams` 表 |
| `src-tauri/src/memory_graph/reflection_service.rs` | daydreams 存助手 + `run_daydream` pass;`run_once(app, turn_count)` 加 daydream gate |
| `src-tauri/src/engine_sink.rs` | trigger 改调 `run_once(app, n)`(把已算好的回合数传进去) |

**不改 PiPromptContext / pi sites**(daydream 不注入 prompt)。前端渲染 `agent:daydream` 事件是后续 UI 任务,不在本 PR。

---

## PR P4 — daydream pass

### Task 1: V58 daydreams 表 + 存助手(TDD)

**Files:** Modify `src-tauri/src/db/migrations.rs`、`src-tauri/src/memory_graph/reflection_service.rs`

- [ ] **Step 1: V58 迁移**(照 V57 模式,additive,`IF NOT EXISTS`;先 grep `V57` 确认注册写法 + 没 open PR 占 V58):
```sql
-- daydreams: divergent free-association hypotheses (mem.md daydream)
CREATE TABLE IF NOT EXISTS daydreams (
    id         TEXT PRIMARY KEY,
    content    TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_daydreams_created ON daydreams(created_at DESC);
```

- [ ] **Step 2: 写失败测试**(reflection_service.rs `#[cfg(test)]`):
```rust
    #[test]
    fn daydreams_store_inserts_and_reads_recent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        apply_daydreams_schema(&conn);
        insert_daydream(&conn, "id1", "what if agents dream in graphs?").unwrap();
        insert_daydream(&conn, "id2", "rust borrow-checker as a memory model").unwrap();
        let recent = recent_daydreams(&conn, 5).unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent.iter().any(|d| d.content.contains("borrow-checker")));
    }
```

- [ ] **Step 3:** Run `cd src-tauri && cargo test --lib daydreams_store 2>&1 | tail -8` — FAIL。

- [ ] **Step 4: 实现存助手**(reflection_service.rs):`fn apply_daydreams_schema(conn)`(V58 daydreams DDL,for tests)+ `fn insert_daydream(conn, id, content) -> Result<()>` + `struct DaydreamRow { content: String, created_at: String }` + `fn recent_daydreams(conn, limit) -> Result<Vec<DaydreamRow>>`(ORDER BY created_at DESC)。

- [ ] **Step 5:** Run `cargo test --lib daydreams_store 2>&1 | tail -8` — PASS。`cargo build 2>&1 | grep -E "^error"`(空)。

- [ ] **Step 6: Commit**:
```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/memory_graph/reflection_service.rs
git commit -m "feat(db): V58 daydreams table + store helpers"
```

### Task 2: `run_daydream` pass + run_once 集成 + trigger 传 count

**Files:** Modify `src-tauri/src/memory_graph/reflection_service.rs`、`src-tauri/src/engine_sink.rs`

实现者:`run_daydream` 的 LLM/budget/borrow-safe 结构**照抄同文件的 `run_promotion`**(complete_text、budget gate、Mutex-before-await),只换:数据源=随机记忆、prompt=发散、产出=存 daydreams + emit。

- [ ] **Step 1: 实现 `run_daydream(app: tauri::AppHandle)`**:
  - `try_state` → budget gate(同 run_promotion)→ `state.learning_llm` 取 LLM(None 则 return)。
  - 读随机记忆(borrow-safe 块,drop guard 再 await):`SELECT title FROM memory_nodes WHERE title IS NOT NULL AND title != '' ORDER BY RANDOM() LIMIT 6`(+ 可选随机 facts);空则 skip。
  - 发散 prompt(system 引导"free-associate / be speculative / one novel hypothesis or connection";user = 这几条随机记忆),`complete_text(cost_tag="memory_daydream", system, user, max_tokens=300)` → `output.text`。
  - `insert_daydream(conn, uuid, &text)`。
  - emit:`use tauri::Emitter;` 然后 `let _ = app.emit("agent:daydream", serde_json::json!({"content": text, "created_at": chrono::Utc::now().to_rfc3339()}));`(确认 `Emitter` 的 emit 签名;load_context 里 `app_handle.emit("agent:memory-recall", ev)` 是同款例子)。
  - 全程 best-effort:任何失败 log + return。

- [ ] **Step 2: run_once 加 daydream gate**:把 `pub async fn run_once(app: tauri::AppHandle)` 改成 `pub async fn run_once(app: tauri::AppHandle, turn_count: u64)`;在 reflection + promotion **之后**加:
```rust
    const DAYDREAM_EVERY_N_TURNS: u64 = 100;
    if should_run_reflection(turn_count, DAYDREAM_EVERY_N_TURNS) {
        run_daydream(app.clone()).await;
    }
```
(复用 `should_run_reflection` 的 modulo;run_once 内部对 app 的用法若已 move,注意 `app.clone()`。)

- [ ] **Step 3: trigger 传 count**:`engine_sink.rs` 把 `run_once(app).await` 改成 `run_once(app, n).await`(`n` 是该处已算好的 `fetch_add(...)+1` 回合数)。

- [ ] **Step 4:** `cargo build 2>&1 | grep -E "^error"`(空);`cargo test --lib reflection_service 2>&1 | grep "test result"`(含 daydreams 测试 + P3 既有测试全过);warnings ≤ 47。

- [ ] **Step 5: Commit**:
```bash
git add src-tauri/src/memory_graph/reflection_service.rs src-tauri/src/engine_sink.rs
git commit -m "feat(daydream): run_daydream — divergent free-association over random memory + agent:daydream event

Every ~100 agent turns (reuses the reflection scheduler at 1/5 cadence). Stores to
daydreams + emits agent:daydream for the UI. Not injected into the prompt."
```

**→ P4 完成。** 验收:`cargo test --lib reflection_service` 全过;手测:聊 ~100 轮后 `daydreams` 表出现行 + 前端收到 `agent:daydream` 事件。

---

## Self-Review
- **决策覆盖**:UI 浮现 = insert daydreams 表 + emit `agent:daydream`,无 PiPromptContext 改动(不注入)。✓
- **类型一致**:`run_once(app, turn_count)` 新签名 → engine_sink 调用同步改;`should_run_reflection` 复用(daydream gate);`insert_daydream`/`recent_daydreams`/`apply_daydreams_schema` 跨 task 一致。✓
- **可测性**:daydreams 存 CRUD 走 TDD;run_daydream 的 LLM+emit build-green + 手测。
- **留给实现者核定**:`tauri::Emitter::emit` 确切签名(照 load_context 的 `agent:memory-recall`);V58 注册写法(照 V57)。

## Execution Handoff
Subagent-Driven:一个 implementer subagent(2 task);controller review + 开 Phase-4 PR。
