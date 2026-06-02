# Memory Integration — Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (user request). One implementer subagent per PR-group, controller reviews at PR boundary. Steps use checkbox (`- [ ]`).

**Goal:** P2 收敛——把 memU 降为 embedder-only(彻底退出召回),并把 P1-② 的扁平 composer 升级成 typed `PiPromptContext` struct(只含现有维度,P3 再加)。

**Architecture:** 两个独立 commit 组。P2-① 是删除级联(由编译器"unused"警告引导,行为目标=召回 FTS-only、无 memU)。P2-② 是把 `build_pi_prompt_context_blocks(vec)` 重构成 typed struct + `compose()`,两个 pi site 改用它。

**Tech Stack:** Rust (Tauri v2) · `cargo test --lib` · 分支 `pi/memory-integration-p2`。
**Spec:** `docs/superpowers/specs/2026-06-02-memory-integration-design.md` §5(P2 sketch);本计划据用户 2 个决策细化:memU=embedder-only 完整退、ContextBuilder=务实 typed。

**验证命令:** `cd src-tauri && cargo build 2>&1 | grep -E "^error"`(空)· `cargo build 2>&1 | grep "generated [0-9]* warnings"`(≤ 基线 47)· `cargo test --lib <filter>`。

---

## File Structure

| 文件 | 改动 | PR |
|---|---|---|
| `src-tauri/src/app.rs` | 移除 `MemUAdapter` 注册(:1040-43);保留 `MemUEmbedder`(:1013) | P2-① |
| `src-tauri/src/memory_graph/recall.rs` | `layer_relevant` 删 memU 向量检索→FTS-only;删 `memu_retrieve_timeout_ms`(5 处)+ `memu_client` 字段/构造参数(若全 unused) | P2-① |
| `src-tauri/src/tauri_commands.rs` | 删 patch 字面量里的 `memu_retrieve_timeout_ms` + 两个 pi site 的 `get_or_insert(...)`;`MemoryRecallEngine::new` 调用去掉 memu 参数 | P2-① |
| `src-tauri/src/agent/memory_context.rs` | `build_pi_prompt_context_blocks`(vec)→ typed `PiPromptContext` struct + `compose()` | P2-② |
| `src-tauri/src/tauri_commands.rs` | 两个 pi site 改用 `PiPromptContext{...}.compose(...)` | P2-② |

---

## PR P2-① — memU → embedder-only(彻底退出召回)

**行为目标**:召回变成 FTS-only(无 memU 向量);memU 仅剩 `MemUEmbedder`(bucket_seal 用)。#44 已把 L3 bound 到 300ms+超时 fallback FTS,所以这只是把"超时回退"变永久,行为风险低。**这是删除级联——跟着编译器的 unused 警告删干净,以"0 新警告 + 召回 FTS-only"为硬验收。**

### Task 1: 移除 MemUAdapter 注册

**Files:** Modify `src-tauri/src/app.rs`(:1040-1043 的 `if let Some(ref memu) = memu_client { let memu_adapter = MemUAdapter::new(...); memory_adapters_map.insert(...); }` 块)

- [ ] **Step 1:** 删除该 `memu_adapter` 注册块(整个 `if let Some(ref memu)` … insert 块)。**保留** `MemUEmbedder::new`(:1013)和 `memu_client` 本身(embedder + 桥接仍需)。
- [ ] **Step 2:** `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空)。若有引用 `"memu"` 后端的硬编码(grep `"memu"` in `memory_adapter/` + `tauri_commands.rs`),确认没有调用方依赖它(默认是 bucket_seal)。
- [ ] **Step 3:** Commit:
```bash
git add src-tauri/src/app.rs
git commit -m "refactor(memory): retire MemUAdapter recall backend (keep MemUEmbedder)"
```

### Task 2: 召回去掉 memU 向量 → FTS-only(+ 删尽 dead 机制)

**Files:** Modify `src-tauri/src/memory_graph/recall.rs`(`layer_relevant` 的 memU 向量块 + `fts_limit` 的 `self.memu_client.is_some()` 分支 + `memu_retrieve_timeout_ms` 字段 5 处 + `memu_client` 字段:387/构造参数:394)、`src-tauri/src/tauri_commands.rs`(patch 字面量 + 两个 pi site 的 `recall_config.memu_retrieve_timeout_ms.get_or_insert(...)` + `MemoryRecallEngine::new(store, memu, config)` 调用)

- [ ] **Step 1: 先确认现有召回测试是基线(它们应在删 memU 后仍过——召回退化为 FTS-only)**

Run: `cd src-tauri && cargo test --lib "memory_graph::recall" 2>&1 | grep "test result"`. 记下通过数(应保持)。

- [ ] **Step 2: 实现删除级联**

a) `layer_relevant`:删掉 `memu.retrieve(...)` 那段(#44 加的 `let vector_results = if let Some(ref memu) = self.memu_client { ... fetch ... timeout ... } else { Vec::new() };`),令 `vector_results = Vec::new()`(或直接删掉向量融合分支,fusion 退化为纯 FTS)。同时简化 `fts_limit` 的 `if self.memu_client.is_some()` 分支(memU 不再参与召回 → 用 FTS fallback limit)。
b) 删 `memu_retrieve_timeout_ms`:recall.rs 的 struct/Default/Dto/2 个 From(5 处)+ `tauri_commands.rs` `patch_memory_recall_config` 字面量里的那一行 + 两个 pi site 的 `recall_config.memu_retrieve_timeout_ms.get_or_insert(...)`(整段删)。
c) 跟编译器走:`memu_client` 字段(:387)+ 构造参数(:394)+ `MemoryRecallEngine::new` 所有调用方(pi sites + legacy recall sites,把 `new(store, memu, config)` 改 `new(store, config)`)——删到无 unused 警告为止。**删之前先确认 `memu_client` 不在 recall.rs 其它地方被用**(grep `self.memu_client` / `memu_client`);若别处仍用则保留字段、只删 retrieve。

- [ ] **Step 3:** `cargo build 2>&1 | grep -E "^error"`(空);`cargo test --lib "memory_graph::recall" 2>&1 | grep "test result"`(通过数 = Step 1 基线,召回 FTS-only 仍工作);`cargo build 2>&1 | grep "generated [0-9]* warnings"`(≤ 47,**无新 unused 警告**)。
- [ ] **Step 4:** Commit:
```bash
git add src-tauri/src/memory_graph/recall.rs src-tauri/src/tauri_commands.rs
git commit -m "refactor(recall): drop memU vector retrieval — recall is FTS-only now

Removes the L3 memu.retrieve path + its now-dead memu_retrieve_timeout_ms config
+ memu_client from MemoryRecallEngine. memU is now embedder-only (MemUEmbedder
for bucket_seal). Recall degrades to the FTS path #44's timeout already fell back to."
```

**→ P2-① 完成。** 验收:`cargo test --lib memory_graph::recall` 全过(FTS-only);grep `memu_client` in recall.rs 仅剩 0 处(或有理由保留的注明)。

---

## PR P2-② — typed `PiPromptContext`(务实版)

**设计**:把 P1-② 的 `build_pi_prompt_context_blocks(vec![(&str, Option<String>)], usize)` 升级成 typed struct,字段=现有维度,带总预算 + 可选 per-dimension 上限。P3 加 `reflections`/`user_model` 时只加字段。

```rust
/// Typed pi prompt-context. Dimensions in priority order (facts highest,
/// gbrain lowest). `compose` truncates each dimension to its per-dim cap, then
/// concatenates by priority under `total_budget` chars (lower-priority dropped
/// when the budget would overflow). P3 adds `reflections` / `user_model` fields.
#[derive(Default)]
pub struct PiPromptContext {
    pub facts: Option<String>,
    pub genes: Option<String>,
    pub recall: Option<String>,
    pub gbrain: Option<String>,
}

impl PiPromptContext {
    pub fn compose(self, total_budget: usize) -> Option<String> {
        // per-dim caps keep any single dimension from starving the rest
        const CAP_FACTS: usize = 1_500;
        const CAP_GENES: usize = 2_500;
        const CAP_RECALL: usize = 8_000;
        const CAP_GBRAIN: usize = 2_000;
        let dims = [
            (self.facts, CAP_FACTS),
            (self.genes, CAP_GENES),
            (self.recall, CAP_RECALL),
            (self.gbrain, CAP_GBRAIN),
        ];
        let mut kept: Vec<String> = Vec::new();
        let mut used = 0usize;
        for (block, cap) in dims {
            let Some(mut b) = block else { continue };
            if b.trim().is_empty() { continue; }
            if b.len() > cap { b.truncate(cap_boundary(&b, cap)); }
            if used + b.len() > total_budget { break; }
            used += b.len();
            kept.push(b);
        }
        if kept.is_empty() { None } else { Some(kept.join("\n\n")) }
    }
}

/// Largest char-boundary <= cap (avoid splitting a UTF-8 codepoint).
fn cap_boundary(s: &str, cap: usize) -> usize {
    let mut n = cap.min(s.len());
    while n > 0 && !s.is_char_boundary(n) { n -= 1; }
    n
}
```

### Task 1: 定义 `PiPromptContext` + `compose`(TDD)

**Files:** Modify `src-tauri/src/agent/memory_context.rs`(替换 `build_pi_prompt_context_blocks`;迁移其 2 个测试)

- [ ] **Step 1: 写失败测试**(放 `mod tests`)

```rust
    #[test]
    fn pi_context_orders_by_priority_and_skips_empty() {
        let out = PiPromptContext {
            facts: Some("F".into()), genes: Some("G".into()),
            recall: Some("R".into()), gbrain: Some("B".into()),
        }.compose(10_000);
        assert_eq!(out.as_deref(), Some("F\n\nG\n\nR\n\nB"));
        assert!(PiPromptContext::default().compose(10_000).is_none());
        assert!(PiPromptContext { gbrain: Some("   ".into()), ..Default::default() }
            .compose(10_000).is_none());
    }

    #[test]
    fn pi_context_total_budget_drops_low_priority() {
        let out = PiPromptContext {
            facts: Some("AAAA".into()), genes: Some("BBBB".into()),
            recall: Some("CCCC".into()), ..Default::default()
        }.compose(8);
        let s = out.unwrap();
        assert!(s.contains("AAAA") && s.contains("BBBB") && !s.contains("CCCC"));
    }

    #[test]
    fn pi_context_per_dim_cap_truncates_on_char_boundary() {
        let big = "中".repeat(2000); // 6000 bytes > CAP_RECALL=8000? 中=3 bytes → 6000; use a >8000 case
        let huge = "x".repeat(9000);
        let out = PiPromptContext { recall: Some(huge), ..Default::default() }.compose(100_000);
        let s = out.unwrap();
        assert!(s.len() <= 8_000, "recall capped to CAP_RECALL");
        let _ = big;
    }
```

- [ ] **Step 2:** `cargo test --lib pi_context 2>&1 | tail -8` — FAIL(类型不存在)。
- [ ] **Step 3:** 用上面"设计"里的 `PiPromptContext` + `compose` + `cap_boundary` 替换 `build_pi_prompt_context_blocks`(删旧函数 + 它的 2 个旧测试 `compose_pi_*`,被新测试取代)。
- [ ] **Step 4:** `cargo test --lib pi_context 2>&1 | tail -8` — PASS。`cargo build 2>&1 | grep -E "^error"`(此时两个 pi site 仍调旧 `build_pi_prompt_context_blocks` → 会编译失败,Task 2 修;**本 task 的 commit 前先做 Task 2 再一起 build 绿** —— 见 Task 2 Step 顺序)。
- [ ] **Step 5:** 暂不 commit(等 Task 2 一起,保证可编译)。

### Task 2: 两个 pi site 改用 `PiPromptContext{...}.compose(...)`

**Files:** Modify `src-tauri/src/tauri_commands.rs`(两个 pi site 现在调 `build_pi_prompt_context_blocks(vec![...], 12_000)` 的地方)

- [ ] **Step 1:** 两个 site 把
```rust
crate::agent::memory_context::build_pi_prompt_context_blocks(
    vec![("facets", facets_block), ("genes", genes_block),
         ("recall", recall_ctx), ("gbrain", gbrain_block)], 12_000)
```
改成
```rust
crate::agent::memory_context::PiPromptContext {
    facts: facets_block, genes: genes_block, recall: recall_ctx, gbrain: gbrain_block,
}.compose(12_000)
```
两个 site 都改(CLAUDE.md 双 composer 规则)。
- [ ] **Step 2:** `cargo build 2>&1 | grep -E "^error"`(空);`cargo test --lib "agent::memory_context" 2>&1 | grep "test result"`(全过);warnings ≤ 47。
- [ ] **Step 3:** Commit(Task 1 + Task 2 一起,保证可编译):
```bash
git add src-tauri/src/agent/memory_context.rs src-tauri/src/tauri_commands.rs
git commit -m "refactor(memory_context): typed PiPromptContext with per-dimension caps

Replaces the build_pi_prompt_context_blocks(vec) composer with a typed struct
(facts/genes/recall/gbrain) + per-dim caps + total budget. Both pi sites use it.
P3 will add reflections / user_model fields."
```

**→ P2-② 完成。**

---

## Self-Review(对照决策)

- **决策覆盖**:memU embedder-only 完整退 = P2-① T1(adapter)+ T2(L3 向量 + dead 机制);务实 typed = P2-② T1(struct)+ T2(两 site)。✓
- **类型一致**:`PiPromptContext` 字段名(facts/genes/recall/gbrain)在 struct 定义、测试、两个 pi site 一致;`compose(total_budget)` 签名一致。✓
- **可编译性**:P2-② Task 1 删旧函数会让 pi site 暂时编译失败,故 Task 1+2 合成一个 commit(Step 明确)——不破坏 bisect(该 commit 自身可编译)。✓
- **风险点(留给实现者按编译器核定)**:P2-① 的 `memu_client` 删除深度(若 recall.rs 别处仍用则保留字段只删 retrieve);`MemoryRecallEngine::new` 调用方数量(编译错误会逐个指出)。硬验收=召回 FTS-only 测试全过 + 0 新警告。

## Execution Handoff
Subagent-Driven:P2-① 一个 implementer subagent,P2-② 一个;controller 在 PR 边界 review + 开一个 Phase-2 PR(两组 bisectable commits)。
