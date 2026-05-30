# R1 接线蓝图：从「引擎已建」到「app 跑通」

> ✅ **已实现（2026-05-30）—— 本蓝图的核心阻塞前提已被推翻**。「接线**必须**在 rusqlite 移除之后」是**错的**：`libsqlite3-sys` 冲突仅因 pi 默认 `sqlite-sessions` feature；**pi 跑 stateless 即 0 个 libsqlite3-sys，与 uClaw rusqlite 共存，无需任何迁移**。接线已完成（`TauriEventSink` + `PiEngine::spawn` + `send_message`/`stop_agent`→`cmd_tx`），见 `MIGRATION_GOALS.md`「## 突破」+ v1.15。本文档保留作历史；§3 的 `TauriEventSink` 代码仍准确。
>
> 2026-05-30 · 配套 [`MIGRATION_GOALS.md`](./MIGRATION_GOALS.md) §P0/§R1
>
> 后端 ACL 骨架（`crates/uclaw-pi-engine`，12 测试绿）+ 前端 §2A bridge 地基（`ui/src/lib/bridge/`）已就绪。本文是把它们接进 uClaw 主体、让 app 端到端跑通的**可执行蓝图 + 决策点**。~~**关键阻塞**：src-tauri 依赖 engine→pi 会触发 `libsqlite3-sys` native-link 冲突（与 uClaw `rusqlite`）。故接线**必须**在 rusqlite 移除之后。~~（**已推翻——见顶部 ✅ 横幅**）

---

## 1. 现实：rusqlite 迁移 ≈ R5 旧后端删除（应合并）

`rusqlite` 用于 **101 个 src-tauri 文件**。实测分布：**大半在 R5 待删的旧后端**——`symphony_graph/`、`learning/`、`memorization/`、`memory_bucket_seal/`、`agent/`（agentic_loop/dispatcher/...）、`runtime/`。对这些「将删」模块做 rusqlite→sqlmodel 迁移 = 无用功。

**结论：把「rusqlite 移除」与「R5 旧后端删除」合并执行，顺序为**

1. **删旧后端执行层**（分析报告 §7.2 / 复刻计划 R5）——一次性消掉绝大多数 rusqlite 用点。
2. **迁剩余 rusqlite**（保留清单 §7.3 里真正要留的：`db/`(会话索引/cost)、`settings`、`permissions` 等）到 pi 的 `sqlmodel-sqlite`——或按 F2 直接让 pi 原生 session store 接管会话，cost/settings 迁 sqlmodel。
3. **校验**：`grep -rl 'rusqlite' src-tauri/src` 归零；`cargo build` 在含 `crates/pi` member 的 workspace 下绿。

> 这意味着 **R1 的「接线」依赖 R5 先行**。建议把 R1/R5 边界重排：R1 = 「ACL 骨架 + 前端 §2A」（引擎已达成 + 前端复刻待续）；接线并入「R5 旧后端删除 + 数据层迁移」这一合并阶段。

---

## 2. 接线：把 `crates/pi` + `crates/uclaw-pi-engine` 转正式 member

rusqlite 归零后：

1. 删 `crates/pi/Cargo.toml` 顶部的 `[workspace]`（独立 sub-workspace 标记）；同 `crates/uclaw-pi-engine/Cargo.toml`。
2. 根 `Cargo.toml` `[workspace] members` 加 `"crates/pi"`、`"crates/uclaw-pi-engine"`。
3. `src-tauri/Cargo.toml` 依赖：`uclaw-pi-engine = { path = "../crates/uclaw-pi-engine" }`（engine 已 re-export pi 所需）。
4. `cargo build --release` 全 workspace 应绿（无 libsqlite3-sys 冲突）。

---

## 3. Tauri `EventSink` 适配器（落 `src-tauri/src/`，rusqlite 后即可编译）

engine 经 `EventSink` trait emit；src-tauri 提供把它接到 `AppHandle::emit` 的适配器：

```rust
// src-tauri/src/engine_sink.rs
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use uclaw_pi_engine::EventSink;

/// 把 engine 的 EventSink emit 转成 Tauri 全局事件（前端 useGlobalAgentListeners 落地）。
pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    pub fn new(app: AppHandle) -> Arc<dyn EventSink> {
        Arc::new(Self { app })
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        // app.emit 是线程安全的（可从 engine 线程调用）。失败仅记日志，不 panic。
        if let Err(e) = self.app.emit(event, payload) {
            log::warn!("EventSink emit {event} failed: {e}");
        }
    }
}
```

启动时（`main.rs` setup）：构造 `EngineConfig`（含 **F7 if2pi**：先 `std::env::set_var("PI_CODING_AGENT_DIR", ~/.uclaw/if2pi/agent)` 等，再 `session_dir = ~/.uclaw/if2pi/agent/sessions`），`PiEngine::spawn(TauriEventSink::new(app.clone()), cfg)`，`app.manage(engine)`。

---

## 4. 命令路由：前端命令 → `EngineCmd`（契约名不变）

`src-tauri/src/commands/agent.rs`（薄命令体，§2A.2 纪律）把既有命令转成 `engine.send(EngineCmd::…)`：

| 前端命令（名不变） | 路由 |
|---|---|
| `send_agent_message` / `send_message` | `EngineCmd::Prompt { conv_id, input }` |
| `stop_agent` / `stop_generation` / `interrupt_current_agent_run` | `EngineCmd::Stop { conv_id }` |
| `agent_follow_up` | `EngineCmd::FollowUp { conv_id }` |
| `set_active_model` / `set_role_model` | `EngineCmd::SetModel { conv_id, provider, model }` |
| `get_messages` / `get_agent_session_messages` | 读 pi `handle.messages()` → `dto::message_to_chat_message`（F2：pi 拥有会话） |
| `agent_steer` | pi 无 handle.steer；二期经 session_mut / RPC（先 stub 或 FollowUp 近似） |
| 审批/ask_user/plan 回填 | per-request oneshot + pending 表（engine next slice：`tool_approval` 装配） |

> 注：engine 当前命令集 = Prompt/FollowUp/SetModel/Stop/Drop。审批/ask_user/steer/compact 的回填是 engine 的**下一 slice**（`AgentConfig.tool_approval` 手动装配 + ask_user 工具回填 + per-request oneshot）。

---

## 5. 前端整树复刻（§2A，独立机械线）

与接线解耦，可并行推进（量大，**适合 workflow 多 agent 并行**，需用户开 workflow）：
1. `lib/bridge/` 已起步——补齐其余域 facade（session/skills/memory/mcp/files/preview/...）。
2. `components/agent/` 60+ 组件 + ai-elements/composer 复刻进 `features/<domain>/`（视觉/交互 1:1），调用点改 `lib/bridge/<domain>`。
3. `useGlobalAgentListeners` 的裸 `listen()` 收敛到 `lib/bridge/events.ts` 工厂（事件名不变）。

---

## 6. 待用户拍板的决策点

1. **R1/R5 边界**：是否把「接线」并入「R5 旧后端删除 + rusqlite 迁移」合并阶段（推荐），即引擎+前端复刻算 R1，接线随合并阶段。
2. **会话持久化落点**（F2 已定 pi 拥有）：cost/settings 是迁 sqlmodel-sqlite，还是也并入 pi 存储？
3. **前端复刻是否开 workflow** 并行推进（60+ 组件机械复刻，单线程会很慢）。
