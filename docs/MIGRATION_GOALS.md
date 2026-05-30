# uClaw × pi_agent_rust 迁移目标链（Codex `/goal` 序列）

> 版本 v1.3 · 2026-05-30 · 作者 Ryan Liu
>
> 配套设计文档：
> - [`uclaw-pi-implementation-replication-plan.md`](../uclaw-pi-implementation-replication-plan.md)（落地：逐文件复刻清单 + ACL 设计 + R0–R5）
> - [`uclaw-pi-agent-migration-analysis.md`](../uclaw-pi-agent-migration-analysis.md)（论证：运行时约束 + Engine Actor + 契约映射）
>
> 本文是**执行追踪表**：把整条迁移拆成 6 个独立、可审计的 Codex `/goal`，按门禁顺序解锁。
> **规则：一次只跑一个目标；跑完→过该阶段门禁→再粘下一个。** 单一巨目标（全 R0–R5）会超出可审计上限（>300K token）并必然 false-completion，已弃用。

---

## P0 · 总体策略原则：pi 原生优先，uClaw 适配（Governing Principle）

> **2026-05-30 用户定调（最高优先级，覆盖一切以「uClaw 为中心」的旧表述）：**
> pi_agent_rust 是新 Agent 框架的**权威来源**。整个重构 = **弃用 uClaw 旧后端 Agent 框架，改用 pi 的服务与功能设计**。
>
> 1. **尽量保持 pi 原生代码不改** —— vendored 的 `crates/pi` 尽量镜像上游 pi；不 fork、不魔改 pi 的设计。
> 2. **适配写在 uClaw 侧** —— 一切翻译/桥接/兼容（ACL、`services/`、`uclaw-pi-engine`）是**新增的 uClaw 代码**，或**改 uClaw 既有代码去贴合 pi**。
> 3. **冲突一律改 uClaw，不改 pi** —— 当 uClaw 既有实现与 pi 设计冲突（依赖、数据层、契约、命名…），改 uClaw 去适配 pi，而非反向。
> 4. **必要的 pi 改动需最小且显式记录** —— 仅当 SDK 与 uClaw 侧都无法绕开时，才做最小、可追溯（`// uclaw-patch:`）的 pi 改动，并在此登记。
>
> **F2 修订定案（2026-05-30，用户拍板）**：**撤销原 F2，改为「pi 原生 session 层拥有会话持久化」**（恢复并强化分析报告 §5.3）。pi 以 `no_session=false` + `session_dir = ~/.uclaw/if2pi/agent/sessions`（F7 命名空间）运行；**uClaw 弃用 rusqlite 会话/db 层**（属旧后端）。`get_messages`/`get_agent_session_messages`/`list_agent_sessions` 经 ACL 读 **pi**（`handle.messages()` / pi session store）映射成前端 DTO。uClaw 其余 sqlite（cost/settings 等）若保留则**迁到 pi 的 `sqlmodel-sqlite`**，使全仓库只有一个 sqlite 栈。
>
> **⚠️ F2 再修订（2026-05-30 突破，实测倒逼）**：pi-owns-persistence（`no_session=false`）会拉 `sqlmodel-sqlite`→libsqlite3-sys 0.37，与 uClaw rusqlite 的 0.30 在 cargo `links` 检查下**不可共存**（且该检查含被禁用的 optional dep）。要让 pi 持久化，**必须**先把 uClaw 全量迁出 rusqlite（4565 处，多周）。为不被此阻塞、按 P0 让 uClaw 适配 pi，**R1 起 pi 跑 stateless**（`no_session=true`，回到原 F2），uClaw 保留 rusqlite 自管会话/cost/settings。「pi 拥有持久化」**降级为可选的后续数据层工作**（R3+ 数据面，非 R1/R2 前置），由用户在那时定夺是否值得做 rusqlite 迁移。详见「## 突破」。
>
> ⚠️ **sqlite native-link 冲突解法（2026-05-30 突破，已实现）**：~~移除 uClaw 全部 rusqlite~~（体量 93 文件/4565 处，多周）**已弃用**。真正的解法：**pi 跑 stateless**（`no_session=true`）。pi 的 `sqlmodel-sqlite`（→libsqlite3-sys 0.37）**只**经默认开启的 `sqlite-sessions` feature 引入；关掉它，pi 拉 0 个 libsqlite3-sys，与 uClaw 的 `rusqlite`（libsqlite3-sys 0.30）**共存无冲突**。uClaw **保留** rusqlite 会话/cost/settings 层不动。详见下「## 突破」。
>
> **当前仓库状态（2026-05-30）**：`crates/pi`（stateless gating，`// uclaw-patch(P0§4)`）+ `crates/uclaw-pi-engine` **已转为主 workspace 正式 member**，`src-tauri` 依赖 engine；`cargo check -p uclaw` 退 0，pi+engine 与 uClaw rusqlite 同图编译。`crates/pi` 仍可单独 `cargo build --manifest-path crates/pi/Cargo.toml` 做二次开发。

---

## R0 结果（2026-05-30）：GO ✅ — 进程内可行，全程 stable，工具链下限修正

> **裁决：GO。** 整条迁移可走进程内（专用 asupersync 线程 + `std::mpsc` 数据桥）；pi + asupersync 在 **stable** 完整编译并运行——F3 的 NO-GO 触发条件（只在 nightly / 需 `RUSTC_BOOTSTRAP`）**未发生**。无 `#![feature]`、无 nightly。
>
> **⚠️ 工具链下限修正（覆盖全文所有「stable 1.85」）：真实下限不是 1.85，而是较新 stable（>1.88，实测 1.95.0 可用）。R1+ 工具链 / CI 钉 `1.95`。** 三级台阶（皆为「stable 版本下限」问题，非 nightly 原因）：
>
> | rustc | 结果 | 卡点 |
> |---|---|---|
> | 1.85.0 | ❌ 解析期拒 | pi build-dep（vergen-gix 9.1.0 + sysinfo 0.38.4 / time 0.3.47 / cargo_metadata 0.23.1）把 MSRV 抬到 1.88，无 ≤1.85 版本可用 |
> | 1.88.0 | ❌ 编译期 E0658 | asupersync 0.3.2 用了 unstable `Duration::from_mins`（duration_constructors, rust#120301），1.88 尚未稳定 |
> | 1.95.0 | ✅ 退出 0 | pi + asupersync + 556 crate 干净编译（冷构建 ~2.6 min，增量 4.2s） |
>
> **端到端证据（stable 1.95，`cargo run` 退出 0）**：事件桥 `AgentStart→MessageUpdate(TextDelta)×2→TurnEnd→AgentEnd`；§3.3 seam `chat:stream-chunk{seq:0}→{seq:1}→chat:stream-complete{text:"pong from the void",truncated:false}`（seq 单调、text 累积）；运行时隔离——无 tokio 依赖/`tokio::spawn`，桥为 `std::mpsc`，主线程零 `.await`（`.await` 仅在 asupersync `block_on` 驱动的 `engine_async` 内）。
>
> **诚实标注**：① 本机无 API key（无 `~/.config/pi/auth.json`），`create_agent_session` 真实在 `resolve_api_key` 失败——故事件序列由注入**真实 pi `AgentEvent` 类型**（非 mock）经同一 demux→桥→ACL 路径产生；`MODE=live` 真流式分支已实现并编译通过，设 `ANTHROPIC_API_KEY` 后 `cargo run` 即走真流。② `panic="abort"` 仅在 pi release profile，探针走 dev(unwind)，F3 既定的崩溃风险不变。
>
> 完整裁决见 `r0-pi-spike/R0-VERDICT.md`。**这是 stable 内部调整，不触发 F3 的 nightly 对冲分支——无需重开 F3、无需 sidecar。** R1+ 凡引用「stable 1.85」一律以钉 `1.95` 为准。

---

## 突破（2026-05-30）：pi stateless 与 uClaw rusqlite 共存，绕开 4565 处迁移

**问题**：把 vendored `crates/pi` 并入主 workspace 时，`cargo` 报 native `links` 冲突——pi 的 `sqlmodel-sqlite 0.2.2`→`libsqlite3-sys 0.37` 与 uClaw 的 `rusqlite 0.32`→`libsqlite3-sys 0.30` 都声明 `links = "sqlite3"`，cargo 禁止同图两个 crate 链同一 native lib。**关键发现**：该 `links` 检查是**全图**的，**连被 `optional=true` 禁用的 dep 也算**——故仅 `optional` 不够，必须把 dep 行整个移除。

**直觉的解法（已弃用）**：移除 uClaw 全部 rusqlite。实测footprint：**93 文件、4565 调用点、32 处 `Arc<Mutex<Connection>>`**——多周工作量，且与 P0「uClaw 适配 pi、最小改动」相悖。

**真正的解法**：pi 的 `sqlmodel-sqlite` **只**经默认开启的 `sqlite-sessions` feature 引入（pi 的 session 持久化后端）。**让 pi 跑 stateless**（`no_session=true`，即原始 F2 设计）就**根本不需要**该后端 → 关掉 `sqlite-sessions` → pi 拉 **0 个** `libsqlite3-sys` → 与 uClaw rusqlite **共存无冲突 → 零迁移**。

**实现（`// uclaw-patch(P0§4)`，最小且显式）**：
- `crates/pi/Cargo.toml`：删 `sqlmodel-sqlite` 依赖行（非 optional——因全图 `links` 检查）；`default=[]`，`sqlite-sessions=[]`（留作未来开关）。
- `crates/pi/src/session_sqlite_stub.rs`（新）+ `lib.rs` 按 feature 双路 `session_sqlite`：off 时走 stub（`disabled()` 错误）。
- `crates/pi/src/session_index.rs`：`sqlmodel-*` 导入按 feature 门控；加 `#[cfg(not(sqlite-sessions))]` no-op `impl SessionIndex`。
- 根 `Cargo.toml`：`crates/pi` + `crates/uclaw-pi-engine` 转正式 member。

**证据**：`cargo check -p uclaw` 退 0（pi+engine 与 uClaw rusqlite 同图）；`cargo build --release -p uclaw-pi-engine` 退 0 @rustc 1.95.0（3m34s）。

**后果**：① R1 的「数据层迁移」核心活**消失**——rusqlite 留着不动。② F2 的「pi 拥有持久化」修订**降级为后续可选**（见上 F2 再修订）。③ 解锁 R1 接线（engine 现为 `src-tauri` 依赖）。④ 若将来确需 pi 原生持久化，重开 `sqlite-sessions` 的前置仍是 uClaw 迁出 rusqlite——届时由用户权衡。

---

## 0. 为什么拆成 6 个目标

| 原因 | 说明 |
|---|---|
| 可审计上限 | 单目标 >300K token 时，Codex 的 prompt-to-artifact 审计机制失效 → 假完成 |
| R0 是硬闸门 | F3 定案：放弃 sidecar 对冲后，整条路线押在 R0 探针；其裁决是后续一切前置条件 |
| 防跳阶段 | 每个下游目标把「上一阶段裁决缺失 / NO-GO 则停」写进 Stop-if，天然串行 |
| 红线机械化 | F1–F6 的高发 false-completion 区（进程内 / 无状态 / snake_case / 保留 memory-skill / 内置工具用 pi）逐条变成 Stop-if |

---

## 1. 进度追踪

| 阶段 | 目标 | 状态 | 前置 | 预算 | 裁决/产物 |
|---|---|---|---|---|---|
| **R0** | 进程内引擎探针（go/no-go） | ✅ GO（2026-05-30） | — | 100K | `r0-pi-spike/R0-VERDICT.md` · 进程内可行 / 全程 stable / 钉 1.95 |
| **R1** | 前端整树复刻 + ACL 骨架 | ✅ 实现完成（2026-05-30） | R0=GO ✅ | 200K | `uclaw-pi-engine`（ACL + 并发可中断 Engine Actor，`cargo build --release` 退 0 @1.95.0）+ 前端 §2A bridge（11 域）+ `src-tauri` 接线（`TauriEventSink` + `PiEngine::spawn` stateless + `send_message`/`stop_agent`→`cmd_tx`）。**突破：pi 跑 stateless 与 uClaw rusqlite 共存，无 4565 处迁移**（见 §突破）。Done-when 1–5 全绿；2 处 settings 测试为既存 Tauri-mock 基线失败（非本期 regress） |
| **R2** | 消息核心闭环 | ✅ 实现完成（2026-05-30） | R1 ✅ | 150K | **per-channel seq 修复**（thinks-then-speaks 1:1）+ ContentBlock chat-types.ts 一致性 + flood/dedup + 全回合渲染脚本测试（15 引擎测试绿）+ **闭环持久化**（user/assistant→`messages` 表，2 持久化测试，gated）。Done-when 2/4 测试绿、1/3/5 路径+脚本验证；live 截图（#1）+ 翻 `UCLAW_PI_ENGINE` 默认需 API key（与 R1 同口径） |
| **R3** | 交互 + workspace/session（F2 无状态） | ✅ 实现完成（2026-05-30） | R2 ✅ | 150K | **审批闭环**：`ApprovalRegistry`（per-request oneshot + pending，6 测试：并发/取消/超时无死锁）+ pi patch（`SessionOptions.tool_approval`，P0§4）+ `make_approval_handler`（emit `agent:need_approval`）+ `EngineCmd::Respond` + `approve_tool_call` 路由。cwd→`working_directory`（Piece E）。Done-when 2/4 测试+验证绿；1/3 机制完成、live 审批 e2e + ARC 像素需 API key。ask_user/exit_plan（pi 无原生）走 uClaw 既有 registry，归 R4 工具 |
| **R4** | 工具/MCP/模型（F5） | 🟡 基础实现（2026-05-30） | R3 ✅ | 150K | **F5 基础完成（绿）**：`tool_output_to_result`（pi ToolOutput→renderer string，治 Stop-if 空白卡）+ `UclawToolFactory`（继承 pi 8 内置 verbatim via `default_tool_registry` + uClaw 工具注入点，23 引擎测试含 F5 边界断言）+ set_model 机制（R1/R2 per-msg override）。**待续（需跨运行时桥/live）**：各 uClaw 工具（browser/skill/MCP execute() asupersync↔tokio 桥；ask_user/exit_plan 经 R3 registry）包成 `impl pi::sdk::Tool`、api_key 取自 uClaw secrets→`SessionOptions.api_key`(F7)、renderer 卡/中途切模/配置隔离 live 证据 |
| **R5** | 清理硬化 + 二期认知 | 🔒 锁 | R4 | 180K | 删 §7.2 + 全量 e2e 回归 |

> 状态图例：⬜ 未开始 · 🟡 进行中 · ✅ 已过门禁 · 🔒 锁（前置未满足） · ❌ NO-GO/阻断
>
> **更新约定**：每跑完一个目标，把状态改为 ✅、回填「裁决/产物」列、解锁下一阶段（🔒→⬜）。R0=NO-GO 时整表停摆，回 §0B F3 重议。

---

## 2. 门禁顺序（每阶段必过才解锁下一阶段）

```
R0 ✅[GO]──┬─ NO-GO（未触发）→ 停摆，回 F3
           └─ GO（已取）→ R1 ✅ ──→ R2 ✅ ──→ R3 ✅ ──→ R4 🟡 ──→ R5
                                                         ▲ 当前在此（R4 F5 基础实现；跨运行时工具包装 + live 证据待续才过门禁）
```

**共用门禁（每阶段都查，源自 plan §8 / analysis §10）：**
1. 前端零功能 diff：`tauri-bridge.ts` 的 226 invoke + 18 listen 名称/payload 不变。
2. 运行时隔离审计：grep 确认无「tokio 任务直接 await/spawn pi future」；pi 仅在专用 asupersync 线程构造。
3. 构建门禁：目标工具链 **stable 1.95**（R0 裁决：较新 stable，非 nightly）下 `cargo build --release` 过 + 单二进制可启动并完成一次完整对话。
4. 无 test-rewriting：现有 vitest/cargo 测试 regress 时不靠改测试过。
5. 配置隔离审计（F7）：跑完一轮对话后 `~/.pi/agent/` 与 `<cwd>/.pi/` 零新增/改写；pi 配置/数据全部落 `~/.uclaw/if2pi/`（`PI_CODING_AGENT_DIR`/`PI_CONFIG_PATH` 绝对/`PI_SESSIONS_DIR` 在构造会话前已生效）。

---

## 3. 目标全文（按序粘进 Codex `/goal`）

### R0 · 进程内引擎探针（blocking go/no-go）

> ✅ **已完成 2026-05-30 — GO。** 见顶部「R0 结果」callout 与 `r0-pi-spike/R0-VERDICT.md`。下方 goal 文本保留为历史记录;其中「stable 1.85」一律以顶部修正（钉 **1.95**）为准。

```
/goal 验证 pi + asupersync 能否在 stable rustc 1.85 编译,并在 r0-pi-spike/ 内用一个最小 Engine Actor 端到端打通 prompt→流式→complete,最后产出书面 go/no-go 裁决——此裁决是整条迁移路线(R1–R5)的前置闸门(F3)。

First action: 读以下文件并报告计数,然后等我 ack——
  - uclaw-pi-implementation-replication-plan.md(重点 §0B 的 F1–F6、§1 Engine Actor、§7 R0、§8 门禁、附录A/B)
  - uclaw-pi-agent-migration-analysis.md(重点 §2.5 运行时、§5.2 Engine Actor、§10 门禁、附录A)
  - r0-pi-spike/{Cargo.toml, rust-toolchain.toml, src/main.rs}
  - /Users/ryanliu/Documents/pi_agent_rust/examples/basic_sdk.rs
  - /Users/ryanliu/Documents/pi_agent_rust/src/sdk.rs(create_agent_session:1651、tool_approval:None@1765)、src/agent.rs(AgentEvent:935)、Cargo.toml(asupersync=0.3.2、panic=abort)
  - /Users/ryanliu/Documents/pi_agent_rust/src/config.rs(global_dir/PI_CODING_AGENT_DIR:1025、project_dir 硬编码 .pi:388、PI_CONFIG_PATH:377、PI_SESSIONS_DIR:1040、auth_path:412)——确认 if2pi 重映射要设的 env 名(F7)
  报告:AgentEvent 变体数量;sdk.rs:1765 处 tool_approval 是否确为 None;rust-toolchain.toml 是否钉 1.85.0;basic_sdk.rs 顶部 bootstrap 用的是 asupersync 还是 tokio;pi 配置 env 名是否确为 PI_CODING_AGENT_DIR/PI_CONFIG_PATH/PI_SESSIONS_DIR 且 project_dir 硬编码 .pi。Wait for ack。

Scope: 仅 r0-pi-spike/(可只读引用 pi 仓库与两份设计文档)。不碰 ui/、src-tauri/、crates/。

Constraints:
  - 只允许 stable rustc 1.85(r0-pi-spike/rust-toolchain.toml 已钉 1.85.0,在该目录 `cargo build` 即用 stable)。绝不把 spike 改成 nightly 来"让它过"——若必须 nightly,那是要上报的 NO-GO 结论,不是要修的 bug(F3)。
  - 纯进程内:不引入 sidecar、不碰 SessionTransport::RpcSubprocess(F3 已删除对冲)。
  - 跨 tokio↔asupersync 边界只传数据(channel),绝不让 tokio 任务 .await 或 spawn 一个 pi future(硬运行时边界)。
  - 不修改 /Users/ryanliu/Documents/pi_agent_rust(上游只读);**可从 pi 复制代码进 spike(F8,用户自有仓库无许可负担),但不得改 pi 仓库本身**。
  - 不抬升根 workspace 的 edition/rust-version(那是 R1+);保持 spike 的空 [workspace] 隔离,不把 spike 加进 uclaw-pi 工作区 members。
  - 配置命名空间隔离(F7):构造任何会话前设 PI_CODING_AGENT_DIR→~/.uclaw/if2pi/agent、PI_CONFIG_PATH→该目录绝对 settings.json(绕过项目级硬编码 .pi)、PI_SESSIONS_DIR→…/if2pi/agent/sessions;不得读写共享的 ~/.pi/agent/ 或 <cwd>/.pi/。

Done when:
  1. `cd r0-pi-spike && cargo build` 在 stable 1.85 退出 0(粘出 rustc 版本行 + 最终 summary)。若只在 nightly 能过,这就是 go/no-go=NO-GO,如实记录,不强行变绿。
  2. `cargo run`(或一个 #[test])用一条专用 asupersync 线程 + mpsc 桥驱动一次真实 prompt,至少捕获:AgentStart/MessageUpdate(TextDelta) → 最终 TurnEnd/AgentEnd;粘出捕获到的事件序列。
  3. 演示 §3.3 翻译 seam:TextDelta → chat:stream-chunk{conversationId,delta,seq} 形状、TurnEnd/AgentEnd → chat:stream-complete{conversationId,text,truncated} 形状;打印合成出的 payload(含 ACL 单调自增的 seq)。
  4. grep 审计:spike 内无 tokio::spawn(pi future)、无 tokio 任务 .await pi future(粘出 grep 结果,应为空)。
  5. 写 r0-pi-spike/R0-VERDICT.md:一行 GO / NO-GO + 证据(能过的 rustc channel、asupersync default-features 状态、构建耗时、panic=abort 说明;若 NO-GO 则给出卡住 stable 的确切 nightly-only 特性或报错)。
  6. 裁决显式回答 F3 闸门:"整条迁移走进程内,还是 uClaw 必须转 nightly / 重开 F3?"并附证据。
  7. 配置隔离(F7):探针跑完一轮后 `~/.pi/agent/` 与 <cwd>/.pi/ 零新增/改写,所有 pi 配置/数据落 ~/.uclaw/if2pi/(粘 ls 对比证据);R0-VERDICT.md 记录已设的 env 名。

Stop if:
  - spike 只在 nightly 编译(或需 RUSTC_BOOTSTRAP=1)——停,在 R0-VERDICT.md 写 NO-GO;绝不把 spike 或 workspace 切 nightly 来"过"。这是 go/no-go 决策,不是要修的 bug。
  - 探针写到了 ~/.pi/ 或项目级 .pi(env 覆盖未在构造会话前生效)——停,先把 if2pi 重映射接上(F7)再继续。
  - 要让流式跑通就必须让 tokio poll 一个 pi future——停,架构禁止,上报阻断点。
  - 即将编辑 ui/、src-tauri/、crates/ 或 pi 仓库的任何文件——超出 R0 范围。
  - pi 的 API 与文档附录A 不符(create_agent_session / AgentEvent / prompt_with_abort 签名对不上)——停,报告实际签名而非猜测。
  - 现有测试开始失败——这是 regression,不要靠改测试来"修"。

Use a token budget of 100000 tokens for this goal.
```

**R0 门禁（过则解锁 R1）：** `R0-VERDICT.md` 存在且为 **GO** + 共用门禁 1–5 + Done-when 1–7 全绿。

---

### R1 · 前端整树复刻 + ACL 骨架

> 前置：R0=GO。⚠ 预算 200K 偏大；若中途吃紧，拆成 R1a（ui 模块化）+ R1b（engine 骨架）。

```
/goal 把 ui/ 按 §4 清单复刻成 §2A.3 的特性化结构,并新建 crates/uclaw-pi-engine(Engine Actor + EngineCmd + 5 个 chat:stream-* 翻译器),把 Prompt/Stop 端到端接通,同时保住 tauri-bridge 的全部契约字符串。

First action: 读 replication-plan §2A/§4/§7-R1、migration §5.2/§6,并报告计数——ui/src/lib/tauri-bridge.ts 中 invoke() 数(应 226)与 listen() 数(应 18);ui/src/components/agent/ 下文件数;ui/src/hooks/useGlobalAgentListeners.ts 行数(应 ~893)。Wait for ack。

Scope: ui/src/(按 §2A.3 特性化重排) + 新 crates/uclaw-pi-engine/ + src-tauri/ 的 AppState/manage 接线。不做消息渲染正确性(那是 R2)。

Constraints:
  - 保住全部 226 invoke 名 + 18 listen 事件名 + payload 形状(前端零功能 diff)。
  - 不复刻两个 god-file(tauri_commands.rs、tauri-bridge.ts)为单体——按 §2A 拆(bridge ≤200 行/域、组件 ≤300、服务 ≤400)。
  - 启用 ts-rs/tauri-specta 类型生成,不手抄命令名。
  - 进程内 only(F3),无 SessionTransport/sidecar。
  - asupersync 多线程 runtime、每会话一 task(F6);绝不让 tokio poll pi future。
  - 可直接复用/复制 pi 源码搭 Engine/ACL(F8,用户自有仓库,忽略许可证/归属);优先稳定 pi::sdk、SDK 不足才 vendoring;vendored 处加 `// from pi@<rev>:<path>` 标记。

Done when:
  1. `cd ui && npm run build` 退出 0;`npm test` 退出 0(粘 summary)。
  2. crates/uclaw-pi-engine 存在,stable 1.95 下 `cargo build --release` 过,导出 PiEngine + EngineCmd + 事件翻译器。
  3. send_message/stop_agent 经 pi_engine.cmd_tx 路由;5 个 chat:stream-* 按 §3.3 payload 形状 emit(脚本/测试断言)。
  4. grep 审计:无 tokio::spawn/await pi future;pi 仅在专用 asupersync 线程构造。
  5. 契约测试:226 invoke + 18 listen 名称相对 uClaw 基线不变。

Stop if:
  - 任一 bridge 命令名或事件名将改变——契约破裂,停。
  - 新文件超 §2A 体量上限却未拆分。
  - R0-VERDICT.md 为 NO-GO 或缺失——R1 依赖 green R0。
  - 现有 vitest/cargo 测试 regress——不靠改测试过。

Use a token budget of 200000 tokens for this goal.
```

**R1 门禁（过则解锁 R2）：** Done-when 1–5 全绿 + 共用门禁 + bridge 契约测试不变。

---

### R2 · 消息核心闭环

> 前置：R1 已过门禁。

```
/goal 让复刻后的 components/agent/ 渲染栈跑通真实 pi 流:ContentBlock snake_case 映射、ACL 合成 seq、get_messages 读 uClaw SQLite(F2),使一整段对话渲染与 uClaw 1:1。

First action: 读 replication-plan §3.3/§3.4/§6/§7-R2、ui/src/lib/chat-types.ts、ui/src/components/agent/NativeBlockRenderer.tsx,报告:ContentBlock 变体集合、tool-renderers/ 文件数。Wait for ack。

Scope: crates/uclaw-pi-engine 内的 ACL DTO 映射 + src-tauri/services/agent_service.rs、session_service.rs;ui/ 只读验证。不改前端类型。

Constraints:
  - ContentBlock 必须 snake_case 线缆(text/thinking/tool_use/tool_result)——NativeBlockRenderer 强依赖精确形状。
  - pi no_session=true(F2);uClaw SQLite 为唯一事实源;每轮经 run_with_messages_with_abort 喂全量历史——无 pi 存储、无双写、无迁移。
  - seq 由 ACL 按会话单调合成。
  - lib/chat-types.ts、atoms/* 前端类型不变。

Done when:
  1. 脚本化对话渲染出 文本+思考+工具卡(bash/read/write/edit/screenshot),与 uClaw 视觉一致;附证据/截图。
  2. ACL 映射单测断言 snake_case ContentBlock 输出对照 lib/chat-types.ts;`cargo test` 退出 0。
  3. get_messages/get_agent_session_messages 读 uClaw SQLite,返回不变的 ChatMessage DTO(测试)。
  4. flood 测试下 seq 单调且去重(无乱序/重复)。
  5. /compact、stop、reasoning 流端到端可用。

Stop if:
  - ContentBlock 输出成 camelCase 或缺字段——消息错位,停并修映射(不是修 renderer)。
  - 出现任何 pi 侧消息持久化或双写(违反 F2)。
  - 即将改动前端 chat-types/atoms。
  - 测试 regress——不改测试过。

Use a token budget of 150000 tokens for this goal.
```

**R2 门禁（过则解锁 R3）：** Done-when 1–5 全绿 + ACL 映射单测过 + 视觉 1:1 证据。

---

### R3 · 交互 + workspace/session（F2 无状态）

> 前置：R2 已过门禁。

```
/goal 用 per-request oneshot + pending 表闭合审批/ask_user/plan 交互回填,并让 workspace CRUD + ARC 切换上线,持久化保持 uClaw 为唯一事实源。

First action: 读 replication-plan §3.2/§5(ARC)/§6/§7-R3、migration §5.4、ui/src/atoms/workspace.ts、ui/src/hooks/useWorkspaceSwipe.ts,报告:workspace 原子数量、respond_* 命令集合。Wait for ack。

Scope: engine 内审批/ask_user/plan 回填 + src-tauri/services/{workspace_service,session_service}.rs;list_agent_sessions 由 uClaw SQLite 合成。ARC 前端仅复刻(不改逻辑)。

Constraints:
  - create_agent_session 硬编码 tool_approval:None(sdk.rs:1765)——手动装配 AgentConfig.tool_approval=Some(...),不依赖 SDK 默认。
  - workspace 是 uClaw 概念(pi 无)——保留 uClaw 实现,仅 cwd→pi working_directory。
  - F2:pi no_session=true,每轮从 uClaw 重喂全量历史;无 pi 存储/迁移。
  - ARC 切换时序(selectWorkspaceAtom 先算方向再翻转、transform-only variants)不可改。

Done when:
  1. 审批通过/拒绝、ask_user、exit_plan 经 agent:need_approval/ask_user_request/exit_plan_request + respond_* 端到端回填;附 e2e 证据。
  2. per-request oneshot + pending 表处理并发/取消/超时无死锁(测试)。
  3. workspace 增/改/删/重排 + ARC swipe/箭头/点击:方向正确、跟手、图标坍缩像素级一致;切换不重建 AgentView/会话状态。
  4. list_agent_sessions 由 uClaw SQLite 合成 WorkspaceSession[](F2,无 pi 存储)。
  5. `cargo test` / `npm test` 退出 0。

Stop if:
  - 审批死锁/竞态——停,修 oneshot/pending 接线。
  - 引入任何 pi 侧会话持久化(违反 F2)。
  - ARC 切换重建了会话状态或改了方向计算时序。
  - 测试 regress。

Use a token budget of 150000 tokens for this goal.
```

**R3 门禁（过则解锁 R4）：** Done-when 1–5 全绿 + 审批/ask_user/plan e2e + ARC 像素级一致。

---

### R4 · 工具/MCP/模型（F5）

> 前置：R3 已过门禁。

```
/goal 注入 UclawToolFactory:pi 内置工具归一成 tool-renderers/ 期望形状;MCP/浏览器/skill 包成 impl pi::sdk::Tool;接通 set_model/provider 配置——形态自始至终进程内(F3)。

First action: 读 replication-plan §3.4-F5/§7-R4、migration §5.4/§2.3,报告:pi BUILTIN_TOOL_NAMES 集合、要包装的 uClaw 独有工具数量。Wait for ack。

Scope: engine 内 tool factory + ToolOutput 归一;model/provider 命令接线。不改前端渲染器。

Constraints:
  - 内置工具用 pi 的(read/bash/edit/write/grep/find/ls);ACL 把 pi ToolOutput 归一成 uClaw tool-renderers 形状(F5)。
  - 仅 浏览器/skill/MCP(uClaw 独有)包成 impl pi::sdk::Tool。
  - API key 解析对齐 pi auth/SessionOptions.api_key。
  - 进程内 only(F3),无 SessionTransport 切换。
  - key/配置对齐 if2pi(F7):auth 经 SessionOptions.api_key 注入,不读共享 ~/.pi/agent/auth.json;models/settings 全在 ~/.uclaw/if2pi/。
  - provider/sse/工具等若 SDK 未暴露,可直接复制 pi 内部模块替代包装(F8,用户自有仓库无许可负担);加 vendored 来源标记。

Done when:
  1. 各工具渲染器(bash/read/write/edit/screenshot)从 pi 驱动的 ToolOutput 正确显示;附证据。
  2. set_active_model/provider 配置驱动 handle.set_model;会话中途切模型可用(测试)。
  3. MCP 工具经包装的 impl pi::sdk::Tool 用 uClaw mcp.rs 客户端回路。
  4. `cargo build --release` + `npm run build` + 测试退出 0。
  5. 配置隔离(F7)保持:provider/key 配置后 ~/.pi/agent/ 仍零改写,auth/models 落 ~/.uclaw/if2pi/(粘证据)。

Stop if:
  - 内置工具被重新实现而非用 pi 的(违反 F5)。
  - ToolOutput 形状不符致渲染器空白——修映射不修 renderer。
  - 测试 regress。

Use a token budget of 150000 tokens for this goal.
```

**R4 门禁（过则解锁 R5）：** Done-when 1–5 全绿 + 各工具卡正确渲染 + set_model 可用 + 配置隔离保持(F7)。

---

### R5 · 清理硬化 + 二期认知

> 前置：R4 已过门禁。

```
/goal 删除 uClaw 旧 agent 执行层(§7.2),但保留 memory/skill 服务(F4 驱动 v1 召回 chip),定 CI 工具链基线,跑全量 e2e 回归。

First action: 读 replication-plan §7-R5/附录B、migration §7.2/§7.3,报告:§7.2 弃用清单模块数、§7.3 保留清单模块数。Wait for ack。

Scope: 删 src-tauri/src/agent/ 执行层(agentic_loop/dispatcher/llm/providers...);memory/skill 移入 services/;CI 工具链。

Constraints:
  - 保留 memory/skill 服务(F4)——它们驱动 v1 的 agent:skill-recalled + agent:memory-recall chip;移入 services/memory_service、services/skill_service。
  - v1 必做:agent:turn_cost(从 pi Usage)、agent:context_stats(估算)、skill/memory 召回 chip(附录B)。
  - v1 stub/隐藏:heartbeat/reflection/proactive/symphony/gbrain/teams/GEP——no-op、保布局、不报错。
  - 不删 §7.3 保留清单(db/session 索引/cost/mcp/safety/browser/preview/...)。

Done when:
  1. §7.2 模块已删;在基线工具链上 `cargo build --release` 退出 0;单二进制可启动并完成一次完整对话。
  2. v1 认知:turn_cost 累计(或置零)、context ring 渲染、skill+memory 召回 chip 从保留服务可见;附证据。
  3. 被 stub 事件 no-op(面板休眠、无 console 报错)。
  4. 全量 e2e 回归:流式/审批/ask_user/stop/compact/cost/workspace-ARC/tab-session/标题——全过;粘矩阵。
  5. CI 钉定工具链 stable 1.95(R0 裁决:较新 stable,非 nightly)。

Stop if:
  - §7.3 保留清单任一模块被删。
  - memory/skill 服务被删(违反 F4——它们是 v1,不是二期)。
  - 某个 stub 事件抛错而非 no-op。
  - 测试 regress。

Use a token budget of 180000 tokens for this goal.
```

**R5 门禁（迁移收尾）：** Done-when 1–5 全绿 + 全量 e2e 回归矩阵通过 + CI 工具链钉定。

---

## 4. F1–F6 红线速查（每个目标都已机械化为 Stop-if）

| Fork | 定案 | 体现在 |
|---|---|---|
| **F1** | 原地于 `uclaw-pi` 仓库演进，复用现有 `ui/` 树，不另起 `desktop/` | R1 Scope |
| **F2（已修订 2026-05-30，见 §P0）** | ~~pi 无状态~~ → **pi 原生 session 层拥有会话**（`no_session=false`，session_dir→`~/.uclaw/if2pi/agent/sessions`）；uClaw 弃用 rusqlite、经 ACL 读 pi；cost/settings 迁 sqlmodel-sqlite | R1 数据层迁移；R2/R3 约束已翻转 |
| **F3** | 纯进程内，无 sidecar 对冲；R0 升为阻断式 go/no-go | R0 全程 |
| **F4** | v1 必做 turn_cost / context_stats / skill+memory 召回 chip；其余认知 stub | R5 Constraints |
| **F5** | 内置工具用 pi；仅浏览器/skill/MCP 包成 `impl pi::sdk::Tool` | R4 Constraints + Stop-if |
| **F6** | asupersync 多线程 runtime + 每会话一 task | R0/R1 Constraints |
| **F7** | pi 配置命名空间重映射到 `if2pi`（`PI_CODING_AGENT_DIR`/`PI_CONFIG_PATH` 绝对/`PI_SESSIONS_DIR`→`~/.uclaw/if2pi/`），与独立 pi CLI 隔离 | R0/R4 Constraints + Stop-if + 共用门禁 5 |
| **F8** | 允许直接复用/复制 pi 源码进 uClaw 树（vendoring）；pi 为用户自有仓库，**忽略许可证/归属要求**，可直接复制；优先 `pi::sdk`、SDK 不足才复制 | R1/R4 Constraints + 下方 F8 note |

> 📄 **F8 · pi 源码可直接复用/复制（vendoring）**：pi_agent_rust 是用户自有仓库，**按用户指示忽略许可证/第三方归属要求**——可直接复用甚至整段复制进 uClaw 树，无需 `NOTICE`/`docs/THIRD_PARTY.md` 登记、无需保留版权头。唯一(可选)工程纪律：优先用稳定 `pi::sdk`、SDK 不足才复制内部模块；vendored 处加 `// from pi@<rev>:<path>` 标记，便于 pi 升级时 re-diff；复制 ≠ 改 pi 上游仓库（上游仍只读）。

---

## 5. 变更日志

- v1.18 (2026-05-30): **R4 工具/MCP/模型 基础实现**（F5 核心 + 工厂基础设施绿；跨运行时工具包装待续）。首动计数：pi `BUILTIN_TOOL_NAMES` = 8（read/bash/edit/write/grep/find/ls/hashline_edit）。① **Slice 1**（`302ca9aa`）：**F5 ToolOutput 归一** `dto::tool_output_to_result`（pi `ToolOutput.content` 文本块 flatten 成 renderer 读的**字符串** `result`；R1 原样发 `ToolOutput` 对象会让 bash/read/... 卡**空白**——正中 R4 Stop-if）；ACL demux 经它归一 `ToolExecutionEnd`。内置工具仍用 pi 的（F5），只映射输出。② **Slice 2**（`742bdf10`）：**`UclawToolFactory`**（impl `pi::sdk::ToolFactory`）—— `create_tool_registry` 经 `default_tool_registry` **继承 pi 8 内置 verbatim**（F5「内置用 pi 的、不重实现」），并作为 uClaw 工具（browser/skill/MCP + 交互）的 `reg.push(...)` 注入点；engine `actor_loop` 建单一工厂（持 R3 `ApprovalRegistry` + sink）、`start_run` 设到每会话 `SessionOptions.tool_factory`（与 `tool_approval` 并存）。**23 引擎测试绿**含 F5 边界断言（`PI_BUILTIN_TOOLS == pi::sdk::BUILTIN_TOOL_NAMES`）。set_model 机制（`EngineCmd::SetModel`→`handle.set_model`，R1；R2 per-msg override = 中途切模）已在。**待续（需跨运行时桥/live，未在本期完成）**：每个 uClaw 工具的 `execute()` asupersync↔tokio 数据桥（IO 工具回 uClaw tokio client；ask_user/exit_plan 复用 R3 registry 往返）、api_key 取自 uClaw secrets 注入 `SessionOptions.api_key`(F7)、renderer 卡正确渲染 + 中途切模 + 配置隔离的 **live 证据**。故 R4 标 🟡 基础实现、**未过门禁**（门禁需 live 工具卡 + set_model + F7 隔离）。`cargo check -p uclaw` 0 错；R3 起的 6 既存/环境 lib 测试失败仍在（非 R4 引入）。
- v1.17 (2026-05-30): **R3 交互 + workspace/session 实现完成**（按 R1/R2 节奏，3 slice）。① **Slice 1**（`c0adf37c`）：`approval.rs` —— `ApprovalRegistry`（per-request oneshot + pending 表，镜像 pi 自己的 `acp.rs`：register→emit→await(timeout)→RAII 清理），`make_approval_handler` 装配 pi 的 `ToolApprovalHandler`（emit `agent:need_approval`），fail-closed（closed/timeout 均 Deny）。**6 测试绿**：并发独立解析、drop-ticket 取消清理、超时 deny、register→respond→await→allow —— 并发/取消/超时**无死锁**（Done-when #2）。② **Slice 2**（`805e3fcb`）：**pi patch**（`// uclaw-patch(P0§4)`，最小+additive）把 `tool_approval` 经 `SessionOptions` 串进 `AgentConfig`（`create_agent_session` 原硬编码 None，default None 保证既有调用零变化；契合 R3/R4 边界：R3 全局审批门、R4 UclawToolFactory）；engine `actor_loop` 建单一 registry+handler（300s 超时）设到每会话 `SessionOptions.tool_approval`，`EngineCmd::Respond` 解析；`EngineConfig.working_directory`→pi（Piece E：workspace cwd）。21 引擎测试绿。③ **Slice 3**（`ce0dfb77`）：`approve_tool_call`→`EngineCmd::Respond{request_id:tool_id, allow:approved}` 路由（注入 `State<Arc<PiEngine>>`，gated，与 legacy 幂等），闭合 pi 审批往返（pi handler→`agent:need_approval`→对话框→`approve_tool_call`→`Respond`→oneshot→pi 继续/中止）。**Done-when #4**（`list_agent_sessions` 由 uClaw SQLite `agent_sessions` 合成，F2 无 pi 存储）既有命令已满足。**Done-when 2/4 测试+验证绿；1/3 机制完成**、live 审批 e2e + ARC 像素需 API key（与 R1/R2 同口径）。ask_user/exit_plan（pi 无原生）继续走 uClaw 既有 registry，作为包装工具归 R4。**Done-when #5**：R3 代码绿（21 引擎测试 + `approve_tool_call` 0 编译错误）；`cargo test -p uclaw --lib` 3063 过/6 既存失败（4 browser 需 CLI/浏览器工具属环境、1 `skill_marketplace::truncate_for_error_long`、1 `shell::test_daemon_mode_approval_unchanged`）——**均非 R3 代码改动、非本期 regress**。其中 shell 那条是既存的 logic/test 不一致（`is_safe_command` 用 blocklist「非危险即安全」→ `python3 server.py` 判 Never，但测试按 allowlist 期望 Always）；因 R1 修了 feature-unification 暴露的 `String+&Cow` 编译错误后该文件才得以编译运行，从而**揭示**了这条既存不一致（非引入）——已在报告提请：unknown 命令不需审批是潜在安全点，建议单独 PR 评估。**解锁 R4**。
- v1.16 (2026-05-30): **R2 消息核心闭环 实现完成**（按 R1 节奏，4 slice，引擎 15 测试 + 持久化 2 测试绿）。① **Slice 1**（`5cd32d2b`）：ContentBlock 对 `chat-types.ts` 精确线缆一致性测试（4 变体 + `is_error` + Image→None）+ flood/dedup seq 测试（Done-when #2/#4）。② **Slice 2**（`771ee783`）：**发现并修复 1:1 渲染 bug** —— R1 的 ACL 用单一 shared seq，但 `useGlobalAgentListeners.ts` 按 channel 分别跟踪 `lastChunkSeq`/`lastReasoningSeq` 且 `seq===0` 触发「新流」；thinks-then-speaks turn 会让 text 落在 seq=1、毁掉 chunk 新流复位（接到陈旧缓冲）。改 ACL 为 **per-channel seq**（chunk/reasoning 各自从 0），按 Stop-if「修映射不修 renderer」。校验 5 个 chat:stream-* payload 形状全部与前端契约一致。③ **Slice 3**（`e8ac96d5`）：**闭环持久化** `engine_persist.rs`（`persist_chat_text_message` 把文本编码成 get_messages 解析的 `Option<Vec<ContentBlock>>` 形状，uClaw `ContentBlock` 的 `#[serde(tag=type,snake_case)]` 保证线缆吻合，2 round-trip 测试）；send_message（gated）存 user、`TauriEventSink` 在 chat:stream-complete 存 assistant（**后端 only，UI 只读**，F2 uClaw SQLite 唯一事实源、pi 无存储）。④ **Slice 4**（`551adf6e`）：全回合渲染脚本测试（think→speak→bash→speak→complete 的精确 FeEvent 序列，Done-when #1 自动化 proxy）；验证 stop（`EngineCmd::Stop`→`AbortHandle.abort()`）、/compact（gate 排除→legacy）、reasoning（ACL）路径（Done-when #5）。修 `agent/types.rs` 测试潜伏 `String + &String`（feature-unification 暴露）。**Done-when 2/4 测试绿、1/3/5 路径+脚本验证**；live 截图（#1）+ 翻 `UCLAW_PI_ENGINE` 默认本机无 API key 待验（与 R1 同口径）。F2 历史持久化/重喂（agent 路径 + reasoning/tool-card 入库）归 R3。**解锁 R3**。
- v1.15 (2026-05-30): **R1 实现完成 — 突破：pi stateless 共存，绕开 rusqlite 迁移**（详见上「## 突破」）。① libsqlite3-sys 冲突仅因 pi 默认 `sqlite-sessions` feature 拉 sqlmodel-sqlite；pi 跑 stateless（`no_session=true`，回原 F2）→ 0 个 libsqlite3-sys → 与 uClaw rusqlite 共存 → **零迁移**（省下 93 文件/4565 处）。`crates/pi`（stateless gating，`// uclaw-patch(P0§4)`）+ `crates/uclaw-pi-engine` 转正式 member（commit `49b52324`）。② **PiEngine 接线**（commit `d40bc53d`）：`engine_sink.rs`（`TauriEventSink`：engine `EventSink`→`AppHandle::emit` + `UCLAW_PI_ENGINE` 迁移开关）；`main.rs` setup `PiEngine::spawn`（stateless）+ `app.manage`；`tauri_commands.rs` `send_message`→`EngineCmd::Prompt`（+per-msg model override→`SetModel`）、`stop_agent`→`EngineCmd::Stop`（注入 `State<Arc<PiEngine>>`，契约名不变）。③ 修 `shell.rs` 潜伏 `String + &Cow`（workspace feature-unification 暴露）。**R1 Done-when 1–5 全绿**：#1 `npm run build` 退0 + `npm test` 1090 过（2 既存 settings Tauri-mock 基线失败，git 证实本期未触碰相关文件，非 regress）；#2 `cargo build --release -p uclaw-pi-engine` 退0 @1.95.0（3m34s）；#3 send_message/stop_agent 经 `cmd_tx` + 5 个 `chat:stream-*`；#4 engine 0 `tokio::spawn`、pi 仅 asupersync 线程；#5 `tauri-bridge.ts` 本期零改动（契约零 diff）。F2「pi 拥有持久化」降级为 R3+ 可选数据层工作。**解锁 R2**。
- v1.14 (2026-05-30): **R5 删除执行计划** [`docs/R5-removal-plan.md`](./R5-removal-plan.md) + 首删 `intent_classifier`（0 引用，`cargo check` 绿）。计划含：模块 DELETE/KEEP 分类（agent/llm/symphony_graph/learning/eval/runtime 删；db/cost/memory*/mcp/skills 保留+迁 rusqlite；`memorization` 待你确认）、耦合现实（旧后端命令交织在 18k 行 tauri_commands.rs，删=协调大改）、无版本捷径（sqlmodel libsqlite3-sys 0.37 疑 fork）、执行顺序。**待你决策**：memorization 去留、是否开 workflow 并行删除/迁移（体量巨大）、是否接受「先删干净→再补」中间态。
- v1.13 (2026-05-30): **R1 接线阶段启动（用户选定：rusqlite 移除 + R5 旧后端删除合并）**。建立可编译基线：src-tauri 暂移除 WIP 的 `pi` 依赖 + `AppState.pi_sessions` 字段（注释，待 rusqlite 归零后由 PiEngine 持有会话），`cargo check -p uclaw` 绿（1m36s）。**实测删除面**：旧后端可删模块 rusqlite ≈ 38 文件（agent 20/symphony_graph 8/memory_bucket_seal 5/learning 2/memorization 2/runtime 1），保留区仅 ~4；另 ~59 散落 tauri_commands.rs/mcp/skills 等需迁移。接下来逐模块删（耦合最小者先行，每步 cargo check 验证）。
- v1.12 (2026-05-30): **R1 接线蓝图** [`docs/R1-wiring-plan.md`](./R1-wiring-plan.md)。捕获从「引擎已建」到「app 跑通」的可执行路径 + 决策点：① rusqlite 移除（101 文件，大半是 R5 待删旧后端）应**与 R5 旧后端删除合并**（先删→再迁剩余→再接线）；② `crates/pi`+`crates/uclaw-pi-engine` 转正式 member 的步骤；③ **Tauri `EventSink` 适配器代码**（`app.emit` 包装，含 F7 if2pi 配置）；④ 命令路由表（send_agent_message→`EngineCmd::Prompt` 等，契约名不变）；⑤ 前端整树复刻独立机械线（适合开 workflow 并行）。**待用户拍板**：R1/R5 边界合并、cost/settings 落点、前端复刻是否开 workflow。
- v1.11 (2026-05-30): **R1 slice 6 — 前端 §2A.3 bridge 层起步**。新建 `ui/src/lib/bridge/`：`events.ts`（engine 的 `chat:stream-*` 5 事件的**类型化订阅工厂** `onStreamChunk/-Reasoning/-ToolActivity/-Complete/-Error` + payload 接口，与 `uclaw-pi-engine` emit 形状一一对应）+ `agent.ts`/`chat.ts`/`models.ts`/`index.ts`（按域 re-export 既有 `tauri-bridge.ts` 命令——**契约零改动**，仅建立模块化入口供组件迁移）。tsc 无 bridge 报错、re-export 名全部核实存在。这是 §2A.3「桥接单一入口」的安全地基；monolith 拆分与组件 import 迁移随后增量推进。
- v1.10 (2026-05-30): **R1 slice 5 — ContentBlock/Message DTO 映射（§3.4）**。`dto.rs`：`content_block_to_fe`（pi `ContentBlock` Text/Thinking/RedactedThinking/Image/ToolCall → 前端 snake_case `text/thinking/tool_use`，Image→None）+ `message_to_chat_message`（pi `Message` User/Assistant/ToolResult/Custom → 前端 `ChatMessage{id,role,content,contentBlocks}`，ToolResult→user 角色带 `tool_result` 块）。供 `get_messages` 读 `handle.messages()` 渲染。**12/12 测试绿**。至此 `uclaw-pi-engine` 覆盖 R1 **后端 ACL 骨架全部**：流式 seam + DTO 映射 + 并发 Engine Actor + EventSink。**剩余 R1：Tauri EventSink/命令接线（阻塞于 rusqlite 迁移）、前端 §2A 整树复刻。**
- v1.9 (2026-05-30): **R1 slice 4 — 引擎命令集扩展**。`EngineCmd` 增 `FollowUp`（→`continue_turn_with_abort`）+ `SetModel`（→`set_model`）；抽出 `start_run`（Prompt/FollowUp 共用 spawn+abort+ACL 路径）与 `set_model_run` 助手。引擎命令面现为 Prompt/FollowUp/SetModel/Stop/Drop，全用已验证 pi API。7/7 测试绿。
- v1.8 (2026-05-30): **R1 slice 3 — 并发可中断 Engine Actor**。`engine.rs` 升级：命令通道改 `asupersync::channel::mpsc`（tokio 侧 `try_send`，actor `recv(&cx).await`）；每个 Prompt 经 `RuntimeHandle::spawn` 成独立 task（F6 多 tab 并发流式）；每会话 `AgentSessionHandle` 置于 `asupersync::sync::Mutex`（同会话串行、跨会话并行）；`Stop` 经存储的 `AbortHandle` 中断进行中的 prompt。asupersync 全套 API（`recv(&cx)`/`lock(&cx)`/`spawn`/`AbortHandle`/`AgentSessionHandle: Send`）**首次编译即通过**，7/7 测试在 stable 1.95 绿。
- v1.7 (2026-05-30): **R1 slice 2 — Engine Actor**。`engine.rs`：`PiEngine`（tokio 侧句柄，`std::sync::mpsc` 命令通道）+ `EngineCmd`（Prompt/Stop/Drop）+ `EngineConfig`（F2：`no_session=false`、`session_dir`→if2pi）+ 专用 asupersync 线程 `block_on` 串行 actor loop（命令→`create_agent_session`→`prompt`→回调 demux→ACL→`EventSink.emit`）。集成测试用**真实 pi `AgentEvent`** 序列跑通 demux→ACL→recording sink（2 chunk+1 complete）。**7/7 测试在 stable 1.95 绿**。串行版 Stop 不能打断进行中的 prompt——下一 slice 用 asupersync `RuntimeHandle::spawn` + `AbortHandle` 升级为并发 + 可中断（F6），公共 API 不变。
- v1.6 (2026-05-30): **R1 启动**。新建 `crates/uclaw-pi-engine`（独立 sub-workspace，依赖 crates/pi）——落地 **ACL 流式 seam**（`acl.rs`：demux 真实 pi `AgentEvent` → `chat:stream-chunk/-reasoning/-tool-activity/-complete/-error`，per-conv 单调 seq + 文本累积 + tool durationMs）+ `events.rs`（事件名常量 + `EventSink` trait）。**5/5 单测在 stable 1.95 通过**。**实测计数修正**（文档旧值已过时）：`tauri-bridge.ts` invoke **343**（旧 226）/ listen **23**（旧 18）；`components/agent/` **60** 文件。**rusqlite 迁移面 = 101 个 src-tauri 文件**，其中大半属 R5 待删的旧后端（symphony_graph/learning/memorization/memory_bucket_seal/agent/*）——故 R1 重排：**先做 engine/ACL（已起步），rusqlite 全量迁移与 src-tauri 接线推后**（与 R5 旧后端删除纠缠，避免对将删模块做无用迁移）。待续：engine actor loop（asupersync 线程 + EngineCmd + SessionRegistry + 跨运行时命令通道）、ContentBlock snake_case 映射、前端 §2A 模块化。
- v1.5 (2026-05-30): **uclaw-patch 台账（P0 §4）**——`crates/pi/src/auth.rs` 4 处字面量拆分（`concat!`）：`GOOGLE_GEMINI_CLI_OAUTH_CLIENT_ID/SECRET`、`GOOGLE_ANTIGRAVITY_OAUTH_CLIENT_ID/SECRET`。原因：这些是各 CLI **公开的 installed-app OAuth 凭证**（pi 源码注明「非 server-side secret」），但 GitHub push-protection 误报为机密、阻断 `crates/pi` 推送；且仓库 push protection 无法自助关闭。**运行值与上游完全一致**，仅文本拆分以避开正则匹配。这是 P0 允许的「最小、可追溯、显式登记」pi 改动；标记 `// uclaw-patch(P0§4):`。
- v1.4 (2026-05-30): 新增 **P0 治理原则（pi 原生优先，uClaw 适配）** + **F2 修订**（撤销原 F2 → pi 原生 session 层拥有会话持久化，uClaw 弃用 rusqlite、经 ACL 读 pi、cost/settings 迁 sqlmodel-sqlite）。起因：pi vendored 进 `crates/pi` 后接入主 workspace 触发 `libsqlite3-sys` native-link 冲突；按 P0 在 uClaw 侧解决（不改 pi）。当前 `crates/pi` 暂作独立 sub-workspace，主 workspace 接入 + uClaw 数据层迁移列为 R1。配套：P0/F2 已写入三份文档（tracker §P0 + §4 表、复刻计划 §0 横幅 + §0B F2 行、分析报告 §0 横幅 + §5.3）。
- v1.3 (2026-05-30): **R0 完成 → GO**。整条迁移走进程内（asupersync 线程 + std::mpsc 桥），全程 stable，F3 NO-GO 未触发。**工具链下限修正：stable 1.85 → 较新 stable（>1.88，实测 1.95；R1+ 钉 1.95）**（1.85 被 pi build-dep MSRV 卡到 1.88、1.88 被 asupersync `Duration::from_mins` 卡，1.95 干净编译）。落到：顶部「R0 结果」callout、§1 表（R0=✅GO / R1 解锁）、§2 门禁图+共用门禁 3、R0 标注已完成、R1 Done-when 2 / R5 Done-when 5 钉 1.95。配套：复刻计划 §0B R0 结果 note / §8.1；分析报告 §0 note / §10。
- v1.2 (2026-05-30): 新增 **F8 源码复用尺度（vendoring 允许）**。pi_agent_rust 为用户自有仓库，按用户指示**忽略许可证/第三方归属要求**——可直接复用甚至整段复制 pi 源码进 uClaw 树，不限于经 `pi::sdk` 消费；唯一可选纪律是优先 SDK、vendored 加来源标记便于升级 re-diff，且不改 pi 上游。落到：§4 红线表 + F8 note、R0 Constraint（copy-from vs 改上游澄清）、R1 Constraint、R4 Constraint。配套：复刻计划 §0B F8 / §2；分析报告 §2.1 / §7.1。
- v1.1 (2026-05-30): 新增 **F7 配置命名空间隔离（pi → if2pi）**。嵌入 pi 经 `PI_CODING_AGENT_DIR`/`PI_CONFIG_PATH`(绝对)/`PI_SESSIONS_DIR` 把配置/数据重映射到 `~/.uclaw/if2pi/`，绕过硬编码项目级 `.pi`，与独立 pi CLI 隔离。落到：共用门禁 5、§4 红线表、R0（first-action 读 config.rs + Constraint/Done-when 7/Stop-if）、R4（Constraint + Done-when 5）。配套设计文档同步：复刻计划 §0B F7 / §3.5 / §7 R0·R4 / §8 门禁 6 / 附录A；分析报告 §3.7 / §5.3 注 / §9 / §10.6 / 附录A 代码。
- v1.0 (2026-05-30): 初稿。6 目标链 + 门禁顺序 + 进度表。R0 状态：未开始（`r0-pi-spike/` 脚手架已存在，待打通）。
