# R5 旧后端删除 + rusqlite 移除：执行计划

> 2026-05-30 · 配套 [`R1-wiring-plan.md`](./R1-wiring-plan.md) · 目标：src-tauri 的 `rusqlite` 归零 → pi(`sqlmodel-sqlite`) 可入 workspace → 接线。
>
> **基线已建**（`cargo check -p uclaw` 绿，无 pi 依赖）。`intent_classifier` 已删（0 引用，proven 流程）。

## 现实校正（实测）

- **`rusqlite` 用于 101 文件**。删旧后端只消掉 ~38（agent 20 / symphony_graph 8 / memory_bucket_seal 5 / learning 2 / memorization 2 / runtime 1）；**另 ~63 在保留代码里**（`db/`、`cost_store`、`mcp`、`skills_manifest`、`tauri_commands.rs` 命令体等）需 **rusqlite API → sqlmodel-sqlite API 迁移**（不是删）。
- **耦合现实**：每个旧后端模块的命令**交织在 18,516 行 `tauri_commands.rs`** 里（eval/symphony/learning/agent 各有多条命令），删模块 = 在该巨文件 + `main.rs`(generate_handler) + `app.rs`(AppState) 协调删除。**不是干净的删目录**。
- **无版本捷径**：`sqlmodel-sqlite` 用 `libsqlite3-sys 0.37`（疑为 pi 作者 fork；rusqlite 主线 ~0.30/0.31），无 rusqlite 版本满足 0.37 → 必须真删/真迁。

## 模块分类（DELETE vs KEEP）

### 🗑 DELETE（旧后端执行/认知层，§7.2）
`agent/`（agentic_loop/dispatcher/ChatDelegate/tools/turn/tool_dispatch/llm_stream/compaction/...，**核心，最大**）、`llm/`、`providers/`(运行时部分)、`symphony_graph/`(F4 stub symphony)、`learning/`(F4 stub proactive)、`eval/`、`runtime/`(rollout)、`intent_classifier/`✅、以及若存在的 `gep/teams/persona/recovery/heartbeat/regular_task/headless`。

### ✅ KEEP — 但要把 rusqlite 迁到 sqlmodel-sqlite（§7.3 + F4）
`db/`(会话索引/cost，2 rusqlite)、`cost_store.rs`(1)、`mcp.rs`/`mcp_server/`、`skills*`、`settings.rs`/`config/`、`safety/`/`browser/`/`files_rail/`/`preview/`/`automation/`/`channels/`/`im_channels/`/`api/`/`local_api/`/`world/`/`plugins/`。
**F4 记忆/技能服务（驱动 v1 召回 chip）保留**：`memory.rs`(1 rusqlite)、`memory_bucket_seal/`(5 rusqlite，**近期活跃开发 PR7-10，确属保留**)、`memory_graph/` 非 agent-耦合部分。

### ⚠️ 待你确认（keep/delete 模糊）
- `memorization/`（自动记忆服务，2 rusqlite）：是 F4「记忆服务」要保留，还是旧认知层删除？**默认倾向保留+迁移**（属记忆面），但请确认。

## 执行顺序（每步 `cargo check -p uclaw` 验证）

1. **删低耦合旧后端**（命令少）：`intent_classifier`✅ → `eval` → `symphony_graph` → `learning`。每个：删 `tauri_commands.rs` 里相关命令体 + `main.rs` 注册 + `app.rs` 字段 + `lib.rs` mod decl + 目录。
2. **删核心 `agent/` + `llm/` + `providers/` + `runtime/`**（协调大改：这是 18k 行命令文件 + main + app 的最大一刀，建议**单独一轮专注做**或拆多 PR）。
3. **迁保留区 rusqlite → sqlmodel-sqlite**（db/cost/memory*/mcp/skills/tauri_commands 剩余命令）：rusqlite `Connection/Statement/params!` → sqlmodel `SqliteConnection/execute/query`。**~63 文件，API 翻译量大**。
4. **校验** `grep -rl rusqlite src-tauri/src` 归零 → 删 `rusqlite` 依赖。
5. **接线**（[`R1-wiring-plan.md`](./R1-wiring-plan.md) §2-4）：crates/pi+engine 转 member、`TauriEventSink`、命令路由。

## rusqlite → sqlmodel-sqlite API 映射（迁移参考）

> 来源：pi 自身的 `crates/pi/src/session_sqlite.rs`（sqlmodel-sqlite 的规范用法）。迁移保留区的 63 文件时照此翻译。

| rusqlite | sqlmodel-sqlite |
|---|---|
| `Connection::open(path)` | `SqliteConnection::open(&SqliteConfig::file(path.to_string_lossy()).flags(OpenFlags::create_read_write()))` |
| 只读 | `…flags(OpenFlags::read_only())` |
| `conn.execute_batch(ddl)` / 无参 DDL | `conn.execute_raw(sql)` |
| `conn.execute(sql, params![a,b])` | `conn.execute_sync(sql, &[val_a, val_b])` |
| `conn.prepare(sql)?.query_map(p, |r| …)?` | `conn.query_sync(sql, &params)?`（返回 `Vec<Row>`），再 `for row in rows { row.get_named::<T>("col")? }` |
| `row.get::<_,String>(0)` / `row.get("c")` | `row.get_named::<String>("c")`（按列名） |
| `params![…]`(ToSql) | `&[sqlmodel_core::Value::…]`（需把 rusqlite 参数转 sqlmodel `Value`） |
| 事务 | `conn.execute_raw("BEGIN IMMEDIATE")` … `"COMMIT"` / `"ROLLBACK"` |
| 错误 | `map_err(|e| Error::…(format!("…: {e}")))`（自定义包装，仿 `map_sqlite_result`） |

**难点（迁移时注意）**：① rusqlite 常用 `Arc<Mutex<Connection>>` 共享连接（uClaw `db/` 即如此）→ sqlmodel `SqliteConnection` 的共享/Send 模型需确认（可能每次开或池化）；② `params!` 宏 → sqlmodel `Value` 数组需逐参转换；③ `query_map`/`prepare_cached` 等 rusqlite 习语无直接对应，改 `query_sync` + 手动行迭代。

## 给你的决策点

1. `memorization` keep（迁移）还是 delete？
2. 第 2 步（核心 agent/ 大删）+ 第 3 步（63 文件 rusqlite 迁移）体量巨大——**是否开 workflow 多 agent 并行**（删除可并行、迁移可按文件并行），还是我单线程逐步（很慢，多轮）？
3. 是否接受「先把整个旧后端删干净（app 暂时少功能）→ 再补」的中间态，以换取更快归零 rusqlite？
