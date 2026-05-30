# uClaw × pi_agent_rust 迁移目标链（Codex `/goal` 序列）

> 版本 v1.0 · 2026-05-30 · 作者 Ryan Liu
>
> 配套设计文档：
> - [`uclaw-pi-implementation-replication-plan.md`](../uclaw-pi-implementation-replication-plan.md)（落地：逐文件复刻清单 + ACL 设计 + R0–R5）
> - [`uclaw-pi-agent-migration-analysis.md`](../uclaw-pi-agent-migration-analysis.md)（论证：运行时约束 + Engine Actor + 契约映射）
>
> 本文是**执行追踪表**：把整条迁移拆成 6 个独立、可审计的 Codex `/goal`，按门禁顺序解锁。
> **规则：一次只跑一个目标；跑完→过该阶段门禁→再粘下一个。** 单一巨目标（全 R0–R5）会超出可审计上限（>300K token）并必然 false-completion，已弃用。

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
| **R0** | 进程内引擎探针（go/no-go） | ⬜ 未开始 | — | 100K | `r0-pi-spike/R0-VERDICT.md` |
| **R1** | 前端整树复刻 + ACL 骨架 | 🔒 锁（待 R0=GO） | R0=GO | 200K | `crates/uclaw-pi-engine` 骨架 |
| **R2** | 消息核心闭环 | 🔒 锁 | R1 | 150K | 1:1 渲染 + ACL 映射单测 |
| **R3** | 交互 + workspace/session（F2 无状态） | 🔒 锁 | R2 | 150K | 审批/ask_user/plan 回填 + ARC |
| **R4** | 工具/MCP/模型（F5） | 🔒 锁 | R3 | 150K | UclawToolFactory + set_model |
| **R5** | 清理硬化 + 二期认知 | 🔒 锁 | R4 | 180K | 删 §7.2 + 全量 e2e 回归 |

> 状态图例：⬜ 未开始 · 🟡 进行中 · ✅ 已过门禁 · 🔒 锁（前置未满足） · ❌ NO-GO/阻断
>
> **更新约定**：每跑完一个目标，把状态改为 ✅、回填「裁决/产物」列、解锁下一阶段（🔒→⬜）。R0=NO-GO 时整表停摆，回 §0B F3 重议。

---

## 2. 门禁顺序（每阶段必过才解锁下一阶段）

```
R0 ──[GO?]──┬─ NO-GO → 停摆，回 F3（uClaw 转 nightly / 重开 sidecar 对冲）
            └─ GO   → R1 ──→ R2 ──→ R3 ──→ R4 ──→ R5
```

**共用门禁（每阶段都查，源自 plan §8 / analysis §10）：**
1. 前端零功能 diff：`tauri-bridge.ts` 的 226 invoke + 18 listen 名称/payload 不变。
2. 运行时隔离审计：grep 确认无「tokio 任务直接 await/spawn pi future」；pi 仅在专用 asupersync 线程构造。
3. 构建门禁：目标工具链（R0 裁决定，1.85 或 nightly）下 `cargo build --release` 过 + 单二进制可启动并完成一次完整对话。
4. 无 test-rewriting：现有 vitest/cargo 测试 regress 时不靠改测试过。
5. 配置隔离审计（F7）：跑完一轮对话后 `~/.pi/agent/` 与 `<cwd>/.pi/` 零新增/改写；pi 配置/数据全部落 `~/.uclaw/if2pi/`（`PI_CODING_AGENT_DIR`/`PI_CONFIG_PATH` 绝对/`PI_SESSIONS_DIR` 在构造会话前已生效）。

---

## 3. 目标全文（按序粘进 Codex `/goal`）

### R0 · 进程内引擎探针（blocking go/no-go）

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
  2. crates/uclaw-pi-engine 存在,stable 1.85 下 `cargo build --release` 过,导出 PiEngine + EngineCmd + 事件翻译器。
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
  5. CI 钉定工具链(1.85 或 nightly,取决 R0 裁决)。

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
| **F2** | pi 无状态（`no_session=true`），uClaw SQLite 唯一事实源，每轮重喂全量历史 | R2/R3 Constraints + Stop-if |
| **F3** | 纯进程内，无 sidecar 对冲；R0 升为阻断式 go/no-go | R0 全程 |
| **F4** | v1 必做 turn_cost / context_stats / skill+memory 召回 chip；其余认知 stub | R5 Constraints |
| **F5** | 内置工具用 pi；仅浏览器/skill/MCP 包成 `impl pi::sdk::Tool` | R4 Constraints + Stop-if |
| **F6** | asupersync 多线程 runtime + 每会话一 task | R0/R1 Constraints |
| **F7** | pi 配置命名空间重映射到 `if2pi`（`PI_CODING_AGENT_DIR`/`PI_CONFIG_PATH` 绝对/`PI_SESSIONS_DIR`→`~/.uclaw/if2pi/`），与独立 pi CLI 隔离 | R0/R4 Constraints + Stop-if + 共用门禁 5 |
| **F8** | 允许直接复用/复制 pi 源码进 uClaw 树（vendoring）；pi 为用户自有仓库，**忽略许可证/归属要求**，可直接复制；优先 `pi::sdk`、SDK 不足才复制 | R1/R4 Constraints + 下方 F8 note |

> 📄 **F8 · pi 源码可直接复用/复制（vendoring）**：pi_agent_rust 是用户自有仓库，**按用户指示忽略许可证/第三方归属要求**——可直接复用甚至整段复制进 uClaw 树，无需 `NOTICE`/`docs/THIRD_PARTY.md` 登记、无需保留版权头。唯一(可选)工程纪律：优先用稳定 `pi::sdk`、SDK 不足才复制内部模块；vendored 处加 `// from pi@<rev>:<path>` 标记，便于 pi 升级时 re-diff；复制 ≠ 改 pi 上游仓库（上游仍只读）。

---

## 5. 变更日志

- v1.2 (2026-05-30): 新增 **F8 源码复用尺度（vendoring 允许）**。pi_agent_rust 为用户自有仓库，按用户指示**忽略许可证/第三方归属要求**——可直接复用甚至整段复制 pi 源码进 uClaw 树，不限于经 `pi::sdk` 消费；唯一可选纪律是优先 SDK、vendored 加来源标记便于升级 re-diff，且不改 pi 上游。落到：§4 红线表 + F8 note、R0 Constraint（copy-from vs 改上游澄清）、R1 Constraint、R4 Constraint。配套：复刻计划 §0B F8 / §2；分析报告 §2.1 / §7.1。
- v1.1 (2026-05-30): 新增 **F7 配置命名空间隔离（pi → if2pi）**。嵌入 pi 经 `PI_CODING_AGENT_DIR`/`PI_CONFIG_PATH`(绝对)/`PI_SESSIONS_DIR` 把配置/数据重映射到 `~/.uclaw/if2pi/`，绕过硬编码项目级 `.pi`，与独立 pi CLI 隔离。落到：共用门禁 5、§4 红线表、R0（first-action 读 config.rs + Constraint/Done-when 7/Stop-if）、R4（Constraint + Done-when 5）。配套设计文档同步：复刻计划 §0B F7 / §3.5 / §7 R0·R4 / §8 门禁 6 / 附录A；分析报告 §3.7 / §5.3 注 / §9 / §10.6 / 附录A 代码。
- v1.0 (2026-05-30): 初稿。6 目标链 + 门禁顺序 + 进度表。R0 状态：未开始（`r0-pi-spike/` 脚手架已存在，待打通）。
