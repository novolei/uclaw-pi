# 记忆 / 学习 / 自进化系统整合落地 — 设计 (Design Spec)

> **Status:** Design approved (brainstorming). Next: `writing-plans`.
> **Date:** 2026-06-02 · **Owner:** Ryan Liu
> **Reference:** `docs/ref/mem.md` (Agent-Native Memory Architecture)

**Goal:** 让 uClaw 已建好但 stranded 的整套 Agent-Native 记忆/学习/自进化架构,在用户实际运行的 **pi 引擎路径** 上真正落地运行,从"声称实现"变成"在跑"。

**Architecture:** 一切进出 pi 的记忆收敛到两个 chokepoint seam——**读 seam**(`EngineCmd::Prompt.context`)与**写 seam**(`engine_sink`)。整合 = 把现有子系统路由穿过这两个 seam,而非重写。

**Tech Stack:** Rust (Tauri v2) · `uclaw_pi_engine::PiEngine` · SQLite (`~/.uclaw-pi/`) · 既有模块 `learning/` `agent/gep/` `memory_graph/` `memory_bucket_seal/` `proactive/`。

---

## 1. 背景与核心发现

5 路并行代码审计(2026-06-02)的统一结论:**整套架构 ~80% 是真建好的**(非 stub),但**全部挂在 legacy `ChatDelegate` 上,被 pi 早返回甩在墙后**。用户运行 pi(`pi_engine_enabled=true`,持久化于 `~/.uclaw-pi/config.json`),所以这些子系统对用户**一次都没运行过**。

**根因(单一结构性事实):** `send_message`(`tauri_commands.rs:1481`→return `1632`)和 `send_agent_message`(`:4925`→return `5120`)在 pi 分支早早 `return`。所有 learning/facet/gene/reflection/proactive 注入都在 return 之后的 legacy ChatDelegate 块里。pi 路径只携带 4 样东西(其中 3 样是 #43–#46 刚接的):persist + bucket_seal ingest(`engine_sink.rs`)+ load_context 召回 + gbrain 指令。

### mem.md 概念 × 现实对照(三层:有代码 / 接进 pi / 在跑)

| mem.md 概念 | 有代码 | 接进 pi | 在跑 | 证据 |
|---|---|---|---|---|
| Semantic = 知识图谱 | memory_graph(**SQLite,非 petgraph**) | 只读 | 写枯竭 | `memory_nodes=660`(1/天),`memory_edges=4` |
| Event→Fact→Pattern→Model | **半截**(只到 Fact) | legacy-only | 死 | 无 Pattern/Model 类型;`user_profile_facets=3`(全陈) |
| Procedural Rules(自动注入) | `learning/prompt_section`(最全,有 Veto) | legacy-only | 死 | `UserProfileSection::render` 只在 3 个 legacy 站点调 |
| Reflection Agent(周期洞察) | **无周期引擎** | legacy-only | 死 | `ReflectionOrchestrator` 是 per-turn+legacy;"spawned"=0;`daily_summaries=0` |
| Context Builder | load_context(**字符串拼接**) | live | 跑 | "Prompt+TopK",6 维 0 个真维度 |
| Importance Score | `importance_decay`(真算法) | 算了 | **recall 读取=0** | `memory_importance_scores=8/660`;`recall.rs` 0 次引用 |
| GEP/Gene 自进化 | **完整真实环** | legacy-only | 死 | 16 种子基因,**0 运行时诞生**;pi 分支无 gene 字段 |
| daydream | **零代码** | — | — | 全仓 0 命中 |
| bucket_seal(openhuman) | 真 | **live(#45)** | **跑** | `chunks.db:mem_tree_chunks=12`,2026-06-02 05:21 |

**结论:不是烂代码,是迁移没接完。** 修复主要是**接线**,不是重建。#43–#46 + bucket_seal 通水已证明:把子系统路由穿过两个 seam 的 fire-and-forget 模式可行(已成功 4 次)。

## 2. 已定决策(brainstorming)

1. **北极星 = 先接线见效(wire-first):** 用最便宜的改动把 stranded 的 80% 在 pi 上激活,先不大规模退役/重构。
2. **退役范围 = 只 memU→embedder-only:** 退 memU 的 MemoryAdapter store/recall 角色,仅保留为 bucket_seal 的 `MemUEmbedder`。`legacy_steward` / `route_store` / `memory_graph` / `gbrain-backend` **本期不动**(用户拍板保留)。
3. **spec 范围 = north-star + phase-1 详设。**
4. **方案 = B(双 seam),拆 1a 读 → 1b 写两个 PR。**

## 3. North-star 架构(两个 seam + 4 阶段)

```
        ┌─ 读 seam: EngineCmd::Prompt.context ───────────────┐
pi turn │  应进 prompt: rules/facets · genes ·               │ → pi agent
        │  importance-ranked recall · (later) reflections     │
        │  · user_model                                       │
        └─────────────────────────────────────────────────────┘
        ┌─ 写 seam: engine_sink (persist / RealToolRequestSink)┐
pi turn │  应从 turn 学: fact extractor · ToolExecuted 事件 ·   │ → stores
        │  bucket_seal ingest(已接 #45)                       │
        └──────────────────────────────────────────────────────┘
```

- **P1 接线(本 spec 详写):** 1a 读 seam → 1b 写 seam。
- **P2 收敛(sketch):** memU→embedder-only;扁平组装器 → typed `ContextBuilder`。
- **P3 成长(sketch):** 周期 `ReflectionService`;Event→Fact→Pattern→Model 补 Pattern/UserModel。
- **P4 daydream(sketch):** 同调度器,发散 prompt。

设计原则:每个 seam 是**唯一 chokepoint**;穿过 seam 的注入/喂料一律 **fire-and-forget、best-effort、不阻塞回合**;碰共用代码处一律 **flag gate + 可回滚**。

## 4. Phase 1 详设

### Phase 1a — 读 seam(让已建好的进 prompt)

**组件 1 — `PiPromptContext` 组装器**(新,轻量,放 `agent/memory_context.rs`)
- 接口:`build_pi_prompt_context(state, query, recall_ctx: Option<String>, gbrain_block: Option<String>) -> Option<String>`
- 职责:按**固定优先级 + 总 token 预算**拼块,低优先块先截断。顺序:`rules/facets(高,小) → genes(相关策略) → recall(RAG) → gbrain 指令(静态)`。
- 是 P2 typed `ContextBuilder` 的前身——先扁平 composer,**不动 load_context 内部**。
- 两个 pi site(`send_message` `:1615`、`send_agent_message` `:5108`)都改为调它(CLAUDE.md 双 composer 规则)。

**组件 2 — facets/rules 注入**
- 复用 `learning::prompt_section::UserProfileSection::render(&state.facet_cache)`(已存在,产 `## User Profile (Learned)` + Veto 规则块)。两个 pi site 调它喂给组装器。

**组件 3 — genes 注入**
- 复用 `build_gene_retriever`(`tauri_commands.rs:57`)+ `match_genes(query)` + `format_gene_injection`(`agent/gep/retrieval.rs:288`)。产 `<active_genes>` 块喂给组装器。
- 16 个种子基因立刻开始指导 live agent(无需引擎 API 改动——基因以 prompt context 形式搭车)。

**组件 4 — importance-aware 召回**(`memory_graph/recall.rs`)
- 每个 L1-L5 layer 查询 `LEFT JOIN memory_importance_scores ORDER BY importance DESC`,过滤 `archive_pending` 节点。**激活已算好却零读取的子系统**,零新 schema。
- ⚠ `recall.rs` legacy+pi 共用 → gate `importance_recall_enabled`(默认 on)+ 加法式排序,可回滚。

**数据流(1a):**
```
pi send site → load_context() → recall_ctx(按 importance 排)
            → UserProfileSection::render → facets_block
            → gene_retriever.match(query) → genes_block
            → build_pi_prompt_context([gbrain,facets,genes,recall], budget) → context
            → EngineCmd::Prompt { context }
```

**测试:** 组装器单测(优先级 + 预算截断);importance-JOIN 召回单测(按分排序 + archive 过滤)。手测:prompt 内出现 `<active_genes>` / `## User Profile`。
**风险:** 组装器纯加法(只 pi)→ 低;importance-JOIN 动共用 recall.rs(flag 兜底)→ 低-中。

### Phase 1b — 写 seam(让 agent 从 turn 自我刷新)

**组件 5 — Fact extractor spawn**
- pi turn 上 fire-and-forget spawn `learning::extractor`(以 user 消息为信号)→ `LearningCandidate` → 既有 `Buffer` → 既有 `LearningScheduler`(30 分钟 tick)折叠成 facets。
- 落点:**send site**(user 文本 + state 现成;`engine_sink` 缺 user_text)。
- 效果:facets 在 pi 上**刷新 + 累积 `recurrence`/`evidence_count`**——mem.md"说 50 次→规则"前置条件第一次被满足。

**组件 6 — `ToolExecuted` infra 事件**
- pi 工具执行器(`engine_sink::RealToolRequestSink::request`)现对 InfraService 零发布;而 GeneCandidate 池订阅 `InfraEventType::ToolExecuted`。
- 改:pi 工具跑完后向 `state.infra_service` 发 `ToolExecuted{tool_name, success/error}`(engine_sink 经 AppState 拿 handle)。
- 效果:重新喂饱基因候选池 → 既有 `GeneEvolutionScenario` 蒸馏能**真诞生新基因** + capsule 适应度环恢复。

**数据流(1b):**
```
pi user turn → spawn learning::extractor(user_text) → Candidate → Buffer → Scheduler → facets 刷新
pi tool exec → publish InfraEvent::ToolExecuted → GeneCandidate 池 → 蒸馏 → 新基因 + capsule
```

**测试:** extractor 产候选(多轮后 facets 增长);infra 事件发布单测。手测:多轮后 facets/genes 变化。
**风险:** 中——动写路径 + engine_sink 拿 InfraService handle;extractor/蒸馏走 LLM(已有预算 gate)。fire-and-forget 保证不阻塞回合。

## 5. Phases 2–4(sketch)

- **P2 收敛:** ① memU 退 MemoryAdapter store/recall 注册(`app.rs` 不再 insert `memu` adapter;保留 `MemUEmbedder`)。② 1a 扁平组装器升级为 typed `ContextBuilder`:`load_context` 返回 `{task, facts, rules, user_model, recent, reflections, genes}` + 统一 8k/16k/32k 预算,替代盲拼 `push_str`。
- **P3 成长:** ① 周期 `ReflectionService`(`main.rs` Stage 5,抄 `daily_summary.rs` 脚手架,但**读 `agent_messages`/`agent_turns` 活表**,非空的 fragment 表)→ 写新 `reflections(id,insight,confidence,source_event_count,created_at)` 表 → 经读 seam 回注。② Event→Fact→Pattern→Model:补 Pattern/UserModel 晋升 job(读 facets + `memory_nodes(kind='user_profile')` → LLM 蒸馏 → `user_model` → 注入 `## User Model`)。
- **P4 daydream:** 同 ReflectionService 调度器,phase-2 加一个发散"随机记忆自由联想"prompt → 假设 → 写回 gbrain/reflections。

## 6. 横切关注

**Bisectable PR 计划(P1 ≈ 3 PR,跟 #43–#46 节奏):**
| PR | 内容 | 触碰 |
|---|---|---|
| P1-① | importance-aware 召回(独立,因动共用 recall.rs) | `memory_graph/recall.rs`(flag gated) |
| P1-② | 读 seam:`PiPromptContext` 组装器 + facets + genes 注入 | `memory_context.rs`、`tauri_commands.rs`×2 site |
| P1-③ | 写 seam:extractor spawn + `ToolExecuted` infra 事件 | `tauri_commands.rs`(send site)、`engine_sink.rs` |

**测试策略:** 每 PR 单测(组装器预算/排序、importance JOIN、infra publish);`cargo build` 绿、0 new warning;用户在 pi 上手测(prompt 出现 genes/facets,多轮后 facets/genes 增长)。
**风险/回滚:** 碰共用代码处 flag gate(`importance_recall_enabled`);所有写 seam fire-and-forget→不破回合;读 seam 对 pi 纯加法。
**明确不做(YAGNI,留 north-star):** petgraph 迁移、Event Sourcing 统一事件流、真 Memory Router(name-switch 保留)、Hot/Warm/Cold 分层、退役 `legacy_steward`/`route_store`/`gbrain-backend`(用户保留)。

## 7. 证据附录(关键引用)

- pi 早返回:`tauri_commands.rs:1481→1632`、`4925→5120`。
- pi 现有读注入:`prompt_context = gbrain_block + recall_ctx`(`:1615`/`:5108`)→ `EngineCmd::Prompt.context` → `compose_prompt_input`(`crates/uclaw-pi-engine/src/engine.rs:137`)。
- pi 现有写:`engine_sink.rs:91-100`(persist + `spawn_bucket_seal_ingest`)。
- 复用件:`UserProfileSection::render`(`learning/prompt_section.rs:79`)、`build_gene_retriever`(`tauri_commands.rs:57`)、`format_gene_injection`(`agent/gep/retrieval.rs:288`)、`importance_decay::compute_importance`(`memory_graph/importance_decay.rs:165`)。
- legacy-only 现状:learning 接线 `tauri_commands.rs:1986-2007`/`6269-6289`;gene 注入 `:1958-1971`/`6229-6259`;`InfraService` 注入 `:1854`。
- 运行时数据(2026-06-02):`user_profile_facets=3`、`memory_importance_scores=8/660`、gep 16 seed genes/0 runtime-born、`bucket_seal chunks=12`、`daily_summaries=0`、no `reflections` table、daydream 0 命中。
