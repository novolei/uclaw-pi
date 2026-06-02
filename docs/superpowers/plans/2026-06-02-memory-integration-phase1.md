# Memory Integration — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task (the user explicitly requested subagent execution). Fresh subagent per task + two-stage review (spec-compliance then code-quality). Steps use checkbox (`- [ ]`) syntax.

**Goal:** 把 stranded 的 facets/rules + genes + importance 召回接进 pi 的读 seam,把 extractor + ToolExecuted 事件接进写 seam,让已建好的 learning/GEP 在 pi 路径上活起来。

**Architecture:** 三个独立可合 PR,全部 fire-and-forget / best-effort / 加法式,碰共用代码处 flag gate。复用既有件(`UserProfileSection::render`、`build_gene_retriever`/`match_genes`/`format_gene_injection`、`extract_from_chat_turn`、`InfraService::publish_tool_executed`),不重写。

**Tech Stack:** Rust (Tauri v2) · `cargo test --lib` (inline `#[cfg(test)]`) · 分支 `pi/memory-integration`(已有 spec commit `39a995ff`)。

**Spec:** `docs/superpowers/specs/2026-06-02-memory-integration-design.md`

**验证命令(每任务后):**
- `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` — 编译,只看错误
- `cd src-tauri && cargo test --lib <filter> 2>&1 | tail -15` — 单测
- 基线 47 warnings(`uclaw` lib),判据"零新增"

---

## File Structure(改动地图)

| 文件 | 职责 | PR |
|---|---|---|
| `src-tauri/src/memory_graph/recall.rs` | 加 `importance_recall_enabled` flag + 召回按 importance 排序/过滤 archive | P1-① |
| `src-tauri/src/agent/memory_context.rs` | 新 `build_pi_prompt_context` 组装器(纯函数,预算+优先级) | P1-② |
| `src-tauri/src/tauri_commands.rs` | 两个 pi site:调组装器、注入 facets/genes;send site spawn extractor | P1-②/③ |
| `src-tauri/src/engine_sink.rs` | pi 工具执行器发 `publish_tool_executed` | P1-③ |

每个 PR 自成一个 commit 组,独立可合、独立可回滚。

---

## PR P1-① — importance-aware 召回

**前提认知**:`memory_importance_scores` 表已有真算法填充(`importance_decay::compute_importance`),但 `recall.rs` 从不读它。本 PR 让召回按它排序 + 过滤 `archive_pending`,gate 在新 flag(默认 on,可回滚)。`MemoryRecallConfig` 已有 `memu_retrieve_timeout_ms` 字段可照抄其模式。

### Task 1: 加 `importance_recall_enabled` flag(照抄 memu_retrieve_timeout_ms 模式)

**Files:**
- Modify: `src-tauri/src/memory_graph/recall.rs`(`MemoryRecallConfig` struct + `Default` + `MemoryRecallConfigDto` + 两个 `From` impl,全部紧挨 `memu_retrieve_timeout_ms` 旁边)
- Test: 同文件 `mod phase5_boost_tests`

- [ ] **Step 1: 写失败测试**(放进 `mod phase5_boost_tests`)

```rust
    #[test]
    fn dto_round_trip_preserves_importance_recall_enabled() {
        let mut cfg = MemoryRecallConfig::default();
        assert_eq!(cfg.importance_recall_enabled, true); // 默认 on
        cfg.importance_recall_enabled = false;
        let dto: MemoryRecallConfigDto = cfg.clone().into();
        assert_eq!(dto.importance_recall_enabled, Some(false));
        let restored: MemoryRecallConfig = dto.into();
        assert_eq!(restored.importance_recall_enabled, false);
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test --lib dto_round_trip_preserves_importance_recall_enabled 2>&1 | tail -8`
Expected: 编译失败 `no field importance_recall_enabled`。

- [ ] **Step 3: 加字段(5 处,照抄 memu_retrieve_timeout_ms)**

在 `MemoryRecallConfig` struct 加:
```rust
    /// When true (default), recall layers rank by `memory_importance_scores`
    /// and drop `archive_pending` nodes. Off = legacy order. Gate for rollback.
    pub importance_recall_enabled: bool,
```
在 `impl Default` 加:`importance_recall_enabled: true,`
在 `MemoryRecallConfigDto` 加:`#[serde(default)] pub importance_recall_enabled: Option<bool>,`
在 `From<Dto> for Config` 加:`importance_recall_enabled: dto.importance_recall_enabled.unwrap_or(default.importance_recall_enabled),`
在 `From<Config> for Dto` 加:`importance_recall_enabled: Some(cfg.importance_recall_enabled),`

- [ ] **Step 4: 跑测试确认通过**

Run: `cd src-tauri && cargo test --lib dto_round_trip_preserves_importance_recall_enabled 2>&1 | tail -8`
Expected: PASS。还要确认 `patch_memory_recall_config`(`tauri_commands.rs`)的 `MemoryRecallConfigDto { ... }` 字面量加上 `importance_recall_enabled: input.importance_recall_enabled.or(existing.importance_recall_enabled),`(否则 E0063);`cargo build 2>&1 | grep -E "^error"` 须空。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_graph/recall.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(recall): add importance_recall_enabled flag (default on)"
```

### Task 2: 召回按 importance 排序 + 过滤 archive_pending

**Files:**
- Modify: `src-tauri/src/memory_graph/recall.rs`(在组装最终候选的地方——`build_recall_plan_with_time` 收尾,或各 layer 返回后——按 importance 重排 + 过滤;实现者读 `MemoryRecallCandidate` 结构确定 node_id 字段)
- Test: 同文件 `mod phase5_boost_tests`

- [ ] **Step 1: 写失败测试**

先在测试里建一个 store + engine(照抄 `phase5_boost_tests` 里已有的 store 构造 helper),插入两个 candidate 对应节点,给其中"高分"节点写 `memory_importance_scores.importance`,断言 `importance_recall_enabled=true` 时高分排前、`archive_pending` 节点被过滤。具体断言:
```rust
    #[tokio::test]
    async fn recall_orders_by_importance_and_drops_archive_pending() {
        // 用 phase5_boost_tests 既有的 store 构造方式(参考 fetch_boost_signals_* 测试)
        // 1) 插入节点 A(importance=0.9)、B(importance=0.1)、C(archive_pending set)
        // 2) 跑 recall(importance_recall_enabled=true)
        // 3) 断言:结果顺序 A 在 B 前;C 不在结果里
        // (实现者:复用同模块已有的 store/engine 构造 helper,不要新造)
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test --lib recall_orders_by_importance_and_drops_archive_pending 2>&1 | tail -10`
Expected: FAIL(当前 recall 不读 importance,顺序/过滤不符)。

- [ ] **Step 3: 实现 importance 重排 + 过滤**

在 `build_recall_plan_with_time` 收尾处,`if self.config.importance_recall_enabled` 时:对各 layer 的候选 `LEFT JOIN memory_importance_scores`(按 node_id),`ORDER BY importance DESC`,并过滤 `archive_pending_since IS NOT NULL` 的节点。复用 `self.store.lock_conn()`(同 `recall_vector` 的 sync 块模式),一次批量取 importance,在内存里重排/过滤,不逐节点查。flag off 时保持原行为。

- [ ] **Step 4: 跑测试确认通过 + 不回归**

Run: `cd src-tauri && cargo test --lib "memory_graph::recall::phase5_boost_tests" 2>&1 | tail -12`
Expected: 新测试 + 既有 recall 测试全 PASS;`cargo build 2>&1 | grep -E "^error"` 空;warnings 不超基线。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_graph/recall.rs
git commit -m "feat(recall): rank by memory_importance_scores + drop archive_pending (gated)"
```

**→ 开 PR P1-①**(标题 `feat(recall): importance-aware recall on the L1-L5 plan`)。

---

## PR P1-② — 读 seam(facets/rules + genes 进 prompt)

**前提认知**:pi 两个 site 现在 `prompt_context = gbrain_block + recall_ctx`(我在 #46 加的)。本 PR 加一个带预算的组装器,再塞进 facets(`UserProfileSection::render`)+ genes(`format_gene_injection`),按优先级 `rules/facets → genes → recall → gbrain` 排、超预算低优先先截。

### Task 1: `build_pi_prompt_context` 组装器(纯函数,TDD 友好)

**Files:**
- Modify: `src-tauri/src/agent/memory_context.rs`(加 pub fn + `#[cfg(test)]` 测试)

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn compose_pi_orders_by_priority_and_truncates_to_budget() {
        // 顺序:facets → genes → recall → gbrain;空块跳过;全空→None
        let out = build_pi_prompt_context_blocks(
            vec![
                ("facets", Some("F".to_string())),
                ("genes", Some("G".to_string())),
                ("recall", Some("R".to_string())),
                ("gbrain", Some("B".to_string())),
            ],
            10_000,
        );
        assert_eq!(out.as_deref(), Some("F\n\nG\n\nR\n\nB"));
        assert!(build_pi_prompt_context_blocks(vec![("x", None)], 10_000).is_none());
    }

    #[test]
    fn compose_pi_drops_low_priority_blocks_over_budget() {
        // 预算只够前两块;低优先(recall/gbrain)被丢
        let out = build_pi_prompt_context_blocks(
            vec![
                ("facets", Some("AAAA".to_string())),
                ("genes", Some("BBBB".to_string())),
                ("recall", Some("CCCC".to_string())),
            ],
            8, // ~ 容得下前两块
        );
        let s = out.unwrap();
        assert!(s.contains("AAAA") && s.contains("BBBB"));
        assert!(!s.contains("CCCC"), "lowest-priority block must be dropped");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test --lib compose_pi 2>&1 | tail -8`
Expected: 编译失败 `cannot find function build_pi_prompt_context_blocks`。

- [ ] **Step 3: 实现纯函数组装器**

```rust
/// Compose the pi prompt-context from priority-ordered blocks under a char
/// budget. Blocks are given highest-priority-first; when the running total would
/// exceed `budget_chars`, the remaining (lower-priority) blocks are dropped.
/// Empty/None blocks are skipped. Returns None when nothing fits.
pub fn build_pi_prompt_context_blocks(
    blocks: Vec<(&'static str, Option<String>)>,
    budget_chars: usize,
) -> Option<String> {
    let mut kept: Vec<String> = Vec::new();
    let mut used = 0usize;
    for (_label, block) in blocks {
        let Some(b) = block else { continue };
        if b.trim().is_empty() { continue; }
        let cost = b.len() + if kept.is_empty() { 0 } else { 2 }; // "\n\n"
        if used + cost > budget_chars { break; }
        used += cost;
        kept.push(b);
    }
    if kept.is_empty() { None } else { Some(kept.join("\n\n")) }
}
```
(用 char/len 预算近似 token;P2 的 typed ContextBuilder 再精确化。)

- [ ] **Step 4: 跑测试确认通过**

Run: `cd src-tauri && cargo test --lib compose_pi 2>&1 | tail -8`
Expected: 两个测试 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent/memory_context.rs
git commit -m "feat(memory_context): build_pi_prompt_context_blocks composer (budget + priority)"
```

### Task 2: 两个 pi site 注入 facets + genes,改用组装器

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`(`send_message` pi 分支 + `send_agent_message` pi 分支——两处都有的 `let prompt_context = { ... gbrain_block ... recall_ctx ... }` 块)

- [ ] **Step 1: 实现(无独立单测——集成点;靠组装器单测 + build + 手测)**

在两个 pi site,把现有 `prompt_context` 组装替换为:
```rust
        let prompt_context = {
            // facets/rules(已存在;legacy 一直用,pi 现在补上)
            let facets_block =
                crate::learning::prompt_section::UserProfileSection::render(&state.facet_cache);
            // genes(复用 build_gene_retriever + match_genes + format_gene_injection)
            let genes_block = {
                let (active, repo) = {
                    let guard = state.proactive_service.read().await;
                    match guard.as_ref() {
                        Some(svc) => {
                            let repo = svc.gene_repository();
                            let active = repo.lock().ok()
                                .and_then(|r| r.list_active_genes().ok())
                                .unwrap_or_default();
                            (active, Some(repo.clone()))
                        }
                        None => (Vec::new(), None),
                    }
                };
                match build_gene_retriever(active, repo.as_ref()) {
                    Some(retr) => {
                        let matches = retr.match_genes(&QUERY, &[], 5).await; // QUERY=input.content / input.user_message
                        let block = crate::agent::gep::retrieval::format_gene_injection(&matches, 5);
                        (!block.trim().is_empty()).then_some(block)
                    }
                    None => None,
                }
            };
            // gbrain_block + recall_ctx 同现状
            crate::agent::memory_context::build_pi_prompt_context_blocks(
                vec![
                    ("facets", facets_block),
                    ("genes", genes_block),
                    ("recall", recall_ctx),
                    ("gbrain", gbrain_block),
                ],
                12_000, // ~3k token 预算近似
            )
        };
```
chat site 的 `QUERY` = `&input.content`;agent site = `&input.user_message`。`gbrain_block` 沿用各 site 现有变量。两个 site 都要改(CLAUDE.md 双 composer 规则)——在 commit body 注明。

- [ ] **Step 2: build 绿 + warnings 不超基线**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` (空) 然后 `cargo build 2>&1 | grep "generated [0-9]* warnings"`(≤ 基线)。
注意借用:`state.proactive_service.read().await` 是 tokio RwLock,在 async 段;`gene_repository()` 返回 `Arc<Mutex<GeneRepository>>`(`list_active_genes` 在 sync `lock()` 内,不跨 await)。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "feat(pi_engine): inject facets/rules + genes into pi prompt via composer

Both pi send sites (send_message + send_agent_message). Reuses
UserProfileSection::render + build_gene_retriever/match_genes/format_gene_injection."
```

**→ 开 PR P1-②**(标题 `feat(pi_engine): read seam — facets/rules + genes into the pi prompt`)。手测验收:pi 会话 prompt 内出现 `## User Profile (Learned)` + `<active_genes>`(可临时 log `prompt_context` 长度/前缀确认)。

---

## PR P1-③ — 写 seam(extractor + ToolExecuted 事件)

**前提认知**:写 seam 让 pi 从 turn 自我刷新。两件,都 fire-and-forget:① send site spawn `extract_from_chat_turn`(喂 `state.learning_buffer` → 既有 scheduler 折成 facets);② engine_sink 的 pi 工具执行器发 `publish_tool_executed`(喂 GeneCandidate 池)。

### Task 1: send site spawn learning extractor

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`(两个 pi site,user 消息持久化之后)

- [ ] **Step 1: 实现(集成点;复用 legacy 的 extract 调用形态)**

在两个 pi site(紧跟 `spawn_bucket_seal_ingest` 之后)加:
```rust
        // 写 seam:fire-and-forget 学习抽取(user 消息 → LearningCandidate → Buffer
        // → 既有 LearningScheduler 折成 facets)。复用 legacy 的 extract 形态。
        {
            let buffer = std::sync::Arc::clone(&state.learning_buffer);
            let text = QUERY.to_string();              // chat: input.content / agent: input.user_message
            let session = conv_id.clone();             // 各 site 现有 conv/session id
            let turn_id = user_msg_id.clone();          // 各 site 现有 user 消息 id
            let llm = state.learning_llm_handle();      // 若无该 helper,传 None + false(实现者按现有 learning llm 接法)
            tauri::async_runtime::spawn(async move {
                let _ = crate::learning::extractor::extract_from_chat_turn(
                    &text, &session, &turn_id, &buffer, false, None,
                ).await;
            });
        }
```
MVP 先 `llm_enabled=false`(纯 regex 抽取,零 LLM 成本、零预算门);LLM 抽取留 P3。`turn_id` 用各 site 已有的 user 消息 id 变量。

- [ ] **Step 2: build 绿**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空)。确认 `Buffer` 是 `Arc`(AppState `learning_buffer: Arc<Buffer>`,`Arc::clone` OK);`tauri::async_runtime::spawn` 任意线程安全。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "feat(pi_engine): write seam — spawn learning extractor on pi turns (regex, fire-and-forget)"
```

### Task 2: engine_sink pi 工具执行器发 ToolExecuted 事件

**Files:**
- Modify: `src-tauri/src/engine_sink.rs`(`RealToolRequestSink::request` 的 spawn 内,工具跑完拿到 `(text, is_error)` 之后)

- [ ] **Step 1: 实现**

在 `request()` 的 `tauri::async_runtime::spawn` 内,`run_mcp_tool`/`run_skill_tool` 返回 `(text, is_error)` 之后、`engine.send(ToolResult)` 旁边,加:
```rust
            // 写 seam:把 pi 工具执行喂给 InfraService,GeneCandidate 池订阅
            // InfraEventType::ToolExecuted —— 重新喂饱基因蒸馏 + capsule 适应度。
            if let Some(state) = app.try_state::<AppState>() {
                let infra = std::sync::Arc::clone(&state.infra_service);
                let tn = tool_name.clone();
                let err = is_error;
                tauri::async_runtime::spawn(async move {
                    infra.publish_tool_executed(&tn, !err /* success */, None).await;
                });
            }
```
(`publish_tool_executed` 的确切参数由实现者对照 `infra/service.rs:172` 核定——签名可能是 `(tool_name, success, extra)` 或带 error 文本;按实际签名填。)

- [ ] **Step 2: build 绿**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空)。`app`/`tool_name`/`is_error` 在该 spawn 作用域内均可见(`request` 已 clone 进去)。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/engine_sink.rs
git commit -m "feat(pi_engine): write seam — publish ToolExecuted infra event from pi tool executor

Re-feeds the GeneCandidate pool so gene distillation + capsule fitness run on pi."
```

**→ 开 PR P1-③**(标题 `feat(pi_engine): write seam — learning extractor + ToolExecuted events`)。手测验收:pi 上聊几轮 + 用几个工具后,`user_profile_facets` 行数/`evidence_count` 开始增长,`gep/` 候选/基因开始变化。

---

## Self-Review(写完计划对照 spec)

- **Spec 覆盖**:§4 1a 组件 1(组装器=P1-② T1)、组件 2 facets(P1-② T2)、组件 3 genes(P1-② T2)、组件 4 importance 召回(P1-① T1+T2);§4 1b 组件 5 extractor(P1-③ T1)、组件 6 ToolExecuted(P1-③ T2)。§6 PR 表三 PR ↔ 本计划三 PR。✓ 全覆盖。
- **类型一致**:`build_pi_prompt_context_blocks`(T 定义)在 P1-② T2 调用名一致;`importance_recall_enabled` 字段名跨 5 处一致;`extract_from_chat_turn` 参数顺序对齐 legacy 调用(`text, session_id, turn_id, &buffer, llm_enabled, llm`)。✓
- **已知留给实现者核定的点(非占位,是"对照实际签名填"):** (a) P1-① T2 recall 重排的确切 SQL/字段(`MemoryRecallCandidate.node_id`、`archive_pending_since`)——实现者读 recall.rs + V44 schema;(b) P1-③ T2 `publish_tool_executed` 确切签名(`infra/service.rs:172`);(c) P1-③ T1 `learning_llm_handle` 不存在则传 `None/false`。这些是集成点,subagent 实现时读码核定——故 P1-① 用真 TDD,P1-②/③ 的纯逻辑(组装器)用 TDD、wiring 用 build-green + 手测。

---

## Execution Handoff

按用户要求:**Subagent-Driven**(superpowers:subagent-driven-development)——每 task 一个全新 subagent,两段式 review(先 spec 合规、后代码质量),controller(主会话)在 PR 边界开 PR + review。三个 PR 顺序执行(P1-① → P1-② → P1-③),各自独立可合。
