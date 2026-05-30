# uClaw × pi_agent_rust 实施复刻计划（Implementation & Replication Plan）

> 版本 v1.0 · 2026-05-30 · 作者 Ryan Liu
>
> 配套文档：[`uclaw-pi-agent-migration-analysis.md`](./uclaw-pi-agent-migration-analysis.md)（论证底座，给出 asupersync↔tokio 约束、Engine Actor 架构、契约映射总览）。
>
> 本文是**落地文档**：把已锁定的 6 条架构决策固化，给出**前端逐文件复刻清单**、**后端反腐层（ACL）详细设计**、**Workspace+ARC 复刻专节**、**Agent message view 1:1 复刻专节**，以及分阶段实施与验收门禁。
>
> 一句话定位：**前端原盘复刻 uClaw、契约字符串零改动；后端整体换成 pi 引擎,中间用反腐层(ACL)把 pi 的 `AgentEvent`/类型翻译回 uClaw 既有的命令与事件形状。**

---

## 0. 已锁定决策（收录 + 技术校正）

> 🧭 **P0 · 治理原则（2026-05-30，最高优先级，覆盖全文）**：**pi 原生优先，uClaw 适配**——尽量不改 vendored `crates/pi` 的源码（镜像上游 pi），一切适配/桥接写在 uClaw 侧（ACL/services/engine 或改 uClaw 既有代码）；uClaw 与 pi 冲突时**改 uClaw 去贴合 pi，不改 pi**。本原则可**反向修订**本文以 uClaw 为中心的决策（尤其 **F2 持久化归属**）。详见执行追踪表 `docs/MIGRATION_GOALS.md` §P0。

| # | 决策 | 状态 | 备注 |
|---|---|---|---|
| 1 | **桌面框架 = Tauri v2 + React + TS + Vite**；Rust-first，后端直接依赖 pi 的 crate；系统 WebView，二进制 ~15–30MB | ✅ 采纳 | 与现状一致,uClaw 本就是 Tauri v2 |
| 2 | **进程内嵌入 Rust 核心,零 sidecar、零 localhost 端口**;`#[tauri::command]` 驱动 `AgentLoop`;交付单签名二进制 | ✅ 采纳（**含运行时校正**） | 见 §1：观感是"命令直接驱动 Agent",实现上必经 Engine Actor 线程通道 |
| 3 | **前端基座 = uClaw,高保真复用**(约束 A);数据层从 uClaw 的 Tauri 命令**重指向 pi 基础设施** | ✅ 采纳 | 复刻尺度 = 尽量原盘复刻前端逻辑与 UI/UX |
| 4 | **主题思想 = uClaw 的 Workspace + Session 范式**(约束 A);复用 `WorkspaceInfo`/`WorkspaceSession`/`TabItem` 与对应 Jotai 原子 | ✅ 采纳 | 见 §5.2 / §6 |
| 5 | **Agent message view 1:1 原盘复刻**(约束 A);整套消息渲染栈逐文件复制,仅替换数据源 | ✅ 采纳 | 见 §5.4 / §7;靠"前端契约 + 后端反腐层"实现 |
| 6 | **复刻左栏 workspace 管理 + ARC 式切换**(约束 C) | ✅ 采纳 | 见 §6;符号已核实存在 |

> 决策里引用的前端符号已在 `ui/src` 核实全部真实存在：`workspaceSlideVariants`(`components/app-shell/LeftSidebar.tsx:111`)、`useWorkspaceSwipe`/`useWorkspaceArrowSwitch`(`hooks/useWorkspaceSwipe.ts`)、`workspaceSwitchDirectionAtom`+`swipeGestureAtom`(`atoms/workspace.ts:83/104`)、`WorkspaceSwitcherBar`(`components/workspace/WorkspaceSwitcherBar.tsx`)、`WorkspaceInfo`/`WorkspaceSession`(`atoms/workspace.ts:4/15`)、`TabItem`(`atoms/tab-atoms.ts`)。

---

## 0B. 二次打磨决策日志（v1.1，权威覆盖）

> 经一轮逼问(grilling)对 6 个承重不确定点定案。**本节为权威结论**;正文相应处(§1/§2A/§3.4/§7/附录 B)已就地校正,如有残留冲突以本节为准。

| Fork | 议题 | 定案 | 覆盖/影响的章节 |
|---|---|---|---|
| **F1** | 交付形态 | **在现有 `uclaw-pi` 仓库内原地演进,直接复用现有 `ui/` 树,不另起 `desktop/` 工程。** §2A 结构路径去掉 `desktop/` 前缀(即 `ui/src/…`、`src-tauri/src/…`);前端模块化在 `ui/` 内**增量**进行,**复用优先于重写**。 | §2A.2/§2A.3 路径前缀;§2 |
| **F2** | 会话持久化 | ⚠️ **已于 2026-05-30 修订(P0)——本行原内容作废。** 原:pi 无状态、uClaw 为唯一事实源。**新:pi 原生 session 层拥有会话**(`no_session=false`,`session_dir=~/.uclaw/if2pi/agent/sessions`);**uClaw 弃用 rusqlite 会话/db 层**;`get_messages`/`list_agent_sessions` 经 ACL 读 pi;cost/settings 迁 `sqlmodel-sqlite`。详见 `docs/MIGRATION_GOALS.md` §P0「F2 修订定案」。 | 恢复并强化 §5.3;R1 数据层迁移 |
| **F3** | 嵌入对冲 | **纯进程内,不做 sidecar 对冲。** 删除 `SessionTransport`/`RpcSubprocess` 回退。**代价:把"pi 能否在 stable 1.85 编译 + 运行稳定"变成不可回退的硬赌注**——R0 升为**阻断式 go/no-go 门禁**;若 pi 需 nightly,则 uClaw 全量工具链随之 nightly(已接受)。`panic="abort"` 单进程崩溃风险亦已接受。 | §1;§7 R0;§8 风险 |
| **F4** | v1 认知红线 | **v1 必做:** `agent:turn_cost`(从 pi `Usage` 算)、`agent:context_stats`(从 usage 估算)、`agent:skill-recalled` + `agent:memory-recall` chip(由**保留的 uClaw memory/skill 服务**驱动:召回→注入 prompt→emit chip,正好契合 F2 的"uClaw 拥有一切")。**v1 stub/隐藏:** heartbeat、reflection、proactive-learning、symphony、gbrain、teams、GEP。 | §3.3;§4.4 🔵 项;**附录 B** |
| **F5** | 工具与渲染器 | **采用 pi 内置工具**(read/bash/edit/write/grep/find/ls),在 ACL 把 pi `ToolOutput` 归一成 uClaw `tool-renderers/` 期望的形状;浏览器/skill/MCP 等 uClaw 独有工具才包成 `impl pi::sdk::Tool`。 | §3.4 DTO;§7 R4 |
| **F6** | 并发模型 | **asupersync 多线程 runtime + 每会话一 task**(非单条 current_thread),保证 N 个 tab 并发流式互不阻塞。 | **覆盖原 §1/§5.2 的"单线程"措辞** |
| **F7** | 配置命名空间 | **嵌入的 pi 把全部 `pi` 命名空间配置目录/文件重映射到 `if2pi`,与机器上独立 pi CLI 物理隔离。** 机制=Engine 启动时设进程环境覆盖(set-once,构造任何会话前):`PI_CODING_AGENT_DIR`→`<uClaw 数据目录>/if2pi/agent`、`PI_CONFIG_PATH`→该目录绝对 `settings.json`(顺带绕过 pi 硬编码的项目级 `.pi` 合并)、`PI_SESSIONS_DIR`→`…/if2pi/agent/sessions`(防御性,F2 下不落盘);auth 经 `SessionOptions.api_key` 直接注入,不读共享 `~/.pi/agent/auth.json`。 | 新增 §3.5;§7 R0/R4;§8 门禁 |
| **F8** | 源码复用尺度 | **允许直接复用、甚至整段复制 pi_agent_rust 源码进 uClaw 树(vendoring),不限于经 `pi::sdk` 稳定面消费。pi 为用户自有仓库,按用户指示忽略许可证/第三方归属要求,可直接复制使用(无需 NOTICE/THIRD_PARTY 登记)。** 动机:pi 绝大多数模块 `#[doc(hidden)]`+声明不稳定,仅 `pi::sdk`+`error` 稳定;直接复制可取用 SDK 未暴露的内部(`Config::global_dir`、provider、sse 解析、`tool_approval` 装配等),并在 pi 升级破坏 SDK 时就地 fork。**唯一(可选)纪律**:优先用稳定 SDK、SDK 不足才复制;vendored 处加 `// from pi@<rev>:<path>` 标记便于 pi 升级 re-diff;复制≠改 pi 上游仓库(只读不变)。 | §2;§7.2;红线表 |

> ⚠️ **F3 放大的风险(必须直说):** 因为放弃了 sidecar 对冲,整条路线的可行性**完全押在 R0 探针上**——"pi(+`asupersync`)能在 stable rustc 1.85 编译,且单进程内长时运行稳定"。R0 失败时没有就地回退,只能要么接受 uClaw 转 nightly、要么回到本日志重开 F3。**因此 R0 必须最先做,且其结论是后续一切的前置条件。**
>
> ✅ **R0 已完成（2026-05-30）：GO（权威覆盖)。** 进程内嵌入可行,全程 **stable**,F3 的 NO-GO 触发条件(只在 nightly / 需 `RUSTC_BOOTSTRAP`)**未发生**(无 nightly、无 `#![feature]`)。**工具链下限修正:不是 1.85,而是较新 stable（>1.88,实测 1.95.0；R1+ 工具链/CI 钉 `1.95`）——本文凡出现「stable 1.85」均以此为准。** 三级台阶(皆 stable 版本下限,非 nightly):`1.85`❌(pi build-dep vergen-gix/sysinfo/time/cargo_metadata 把 MSRV 抬到 1.88)、`1.88`❌(asupersync 0.3.2 用 unstable `Duration::from_mins`)、`1.95`✅(pi+asupersync+556 crate 干净编译)。§3.3 seam(seq 单调、text 累积)与运行时隔离(无 tokio、std::mpsc 桥、主线程零 `.await`)已端到端验证。**这是 stable 内部调整,不触发 F3 的 nightly 对冲分支——无需重开 F3、无需 sidecar。** 详见 `r0-pi-spike/R0-VERDICT.md` 与执行追踪表 `docs/MIGRATION_GOALS.md`。

---

## 1. 运行时校正：决策 2 的"直接调用"该如何成立

决策 2 的方向完全正确(零 sidecar、零端口、单进程、单签名二进制),但有一句必须在实现层校正：

> **"`#[tauri::command]` 直接调用 `AgentLoop`" —— 字面不可成立。**
> Tauri 的命令运行在 **tokio**;pi 的 `Agent`/`AgentSession`/`AgentLoop` future 运行在 **asupersync**（pi 锁 `asupersync =0.3.2`,其 `src/` 仅 1 处无关 `tokio::`）。两个运行时不能互相 `poll`,因此 **tokio 任务不能 `.await` 一个 pi future**。

**正确形态(= 分析报告 §5 的 Engine Actor)：**

```
#[tauri::command]         asupersync 多线程 runtime（专用，非 current_thread）
   (tokio)        mpsc 命令通道          ┌────────────────────────────┐
 send_message ───────────────────────▶ │ SessionRegistry            │
                                        │  conv_id → AgentSessionHandle│
 事件转发任务 ◀──── tokio mpsc ──────── │  每会话一 task：             │
   app.emit                on_event回调 │  handle.prompt_with_abort   │
                                        │   (on_event 推 AgentEvent)  │
                                        └────────────────────────────┘
```

- **对外观感不变**：前端 `invoke('send_message')` → 命令立即向 Engine 发指令 → 流式事件照常 `emit`。看起来就是"命令直接驱动 Agent"。
- **零 sidecar、零端口、单二进制**：pi 与 Tauri 在**同一进程**,pi 跑在一个**专用 asupersync runtime** 上(由独立 OS 线程承载);跨边界只传**数据**(经 channel),不传 future,**零序列化**(进程内)。
- **并发模型(F6 定案)**:asupersync 用**多线程 runtime**,**每会话一 task**——N 个 tab 同时流式互不阻塞;`SessionRegistry` 内对 `AgentSessionHandle` 的访问按会话隔离。**不可**用单条 `current_thread` runtime(会串行化多 tab)。
- **无对冲(F3 定案)**:不引入 `SessionTransport`/sidecar;Engine 只有进程内一种形态。可行性前置于 R0(见 §0B ⚠️)。
- 这层线程通道 + 类型翻译,就是决策 3 说的"**后端反腐层**"的运行时部分(详见 §4)。

> 单进程的代价是 pi `panic="abort"` 会拖垮整个 app。**F3 已定:不做 sidecar 对冲**,该风险已接受;缓解仅限关键路径输入校验 + `catch`-不可行处的边界隔离 + 监督重启 Engine 线程。崩溃隔离的根因消解留待二期(若届时确有必要再重开 F3)。

---

## 2. 总体复刻策略

复刻分两条独立工作流,边界就是 **`lib/tauri-bridge.ts`** 这一层:

1. **前端(`ui/`):原盘复刻,契约字符串零改动。** 组件、hooks、atoms、framer-motion 动画逐文件复制。前端只认 `invoke('snake_case', …)` 与 `listen('event:name', …)` 的**字符串与 payload 形状**,不关心后端是谁。
2. **后端(`src-tauri/` + 新 `crates/uclaw-pi-engine`):整体换 pi 引擎 + ACL。** ACL 负责:把前端命令翻译成 `EngineCmd`、把 pi 的 `AgentEvent` 翻译回 `chat:stream-*`/`agent:*`、把 pi 的类型翻译回前端期望的 DTO(`ChatMessage`/`ContentBlock`/`WorkspaceSession`…)。**(F8:Engine/ACL 实现可直接复用甚至整段复制 pi 源码——pi 为用户自有仓库,无许可负担;优先稳定 `pi::sdk`、SDK 不足才 vendoring。)**

**"前端零改动"的判据**:复刻完成后**渲染产物(UI/UX/契约字符串)相对 uClaw 无功能性差异**——但**文件组织按 §2A 模块化结构重排**(见下"原盘复刻"的精确含义)。`tauri-bridge.ts` 的 226 个 `invoke()`/18 个 `listen()` 的**命令名与 payload 全部保留**,但**单体文件本身不复刻**,拆成 `lib/bridge/<domain>`。

---

## 2A. 模块化架构纪律（前后端均反 god file，专节）

> 这是把决策 3/5 中"复用 uClaw"收敛为**工程纪律**的专节,并**校正"原盘复刻"的含义**。

### 2A.0 "原盘复刻"的精确含义（校正 §4/§5.4/§7 的"🟢原样复制"）

> **原盘复刻 = 复刻 UI/UX 与组件内部逻辑,而非整文件搬运其桥接与目录结构。**
>
> 具体:消息渲染栈、ARC 动画等的**组件实现逻辑逐一复刻**(视觉/交互 1:1),但
> 1. **按 §2A 特性化结构重新归类**(归入 `features/<domain>/`);
> 2. 组件内对 `tauri-bridge.ts` 的调用点,改为 `lib/bridge/<domain>` 的**细粒度函数**;
> 3. **不复刻** `tauri_commands.rs` / `tauri-bridge.ts` 两个单体本身。
>
> 因此 §4 清单中的"🟢原样复制"应读作"**逻辑原样、位置重排、桥接细化**"。

### 2A.1 反面教材（双重 god file,均禁止复刻）

| 反面教材 | 实测规模 | 病征 |
|---|---|---|
| `src-tauri/src/tauri_commands.rs` | **18,516 行 / 382 命令** | 单体;命令内联业务逻辑;无服务边界 |
| `src-tauri/src/main.rs` 的 `generate_handler!` | **约 485 行**列表(文件共 1,700 行) | 注册集中在 main,巨型宏列表 |
| `ui/src/lib/tauri-bridge.ts` | **2,846 行 / 226 invoke** | 前端桥接单体 |

> `ts-rs`/`tauri-specta` 当前**未启用**——本项目应引入**类型生成**,杜绝前端手抄命令名/payload。

> **路径前缀(F1 定案)**:不另起 `desktop/` 工程,全部在现有 `uclaw-pi` 仓库**原地**进行——下文结构去掉 `desktop/` 前缀(即 `src-tauri/src/…`、`ui/src/…`);前端**复用现有 `ui/` 树**、模块化**增量**推进,复用优先于重写。

### 2A.2 后端结构（`src-tauri/src/`，原地于 uclaw-pi 仓库）

```
src/
  main.rs                  # 仅 builder/window/setup，保持精简
  state.rs                 # AppState：对 pi-engine 各服务的 Arc 句柄（含 PiEngine）
  events.rs                # agent:* / chat:stream-* 事件载荷结构体 + emit 辅助
  services/                # trait 服务层（= 反腐层 ACL，可脱离 Tauri 单测）
    mod.rs
    agent_service.rs       # 包装 pi AgentSessionHandle → 流式（经 Engine Actor）
    session_service.rs     # 会话/历史读写（uClaw SQLite 为唯一事实源，F2）
    workspace_service.rs   # 桌面自有 workspace 存储(rusqlite) + 会话分组
    model_service.rs   skill_service.rs   memory_service.rs
    mcp_service.rs     cron_service.rs    terminal_service.rs
    diagnostics_service.rs
  commands/                # 薄 #[tauri::command]，一域一文件
    mod.rs                 # handlers() 聚合 → generate_handler!（main.rs 不出现长列表）
    agent.rs  chat.rs  workspace.rs  session.rs  models.rs
    skills.rs memory.rs mcp.rs  cron.rs  terminal.rs  diagnostics.rs  ui_store.rs
```

**后端强制纪律(写入 ADR 验收项):**
1. 命令文件**一域一文件**;单文件软上限 ~400 行,超出即拆。
2. 命令体**只做四件事**:解析入参 → 调用服务 → 映射结果/错误 → emit 事件。**禁止内联业务逻辑**。
3. 业务逻辑只在 `services/` 的 **trait 实现**中(如 `trait WorkspaceService { fn list(); fn create(); … }`),不依赖 Tauri 可单测。
4. 注册集中在 `commands/mod.rs` 聚合器,`main.rs` 不出现长 `generate_handler!`。
5. **服务层即 ACL**:pi 内部类型在此翻译为前端契约(`agent:*`/`chat:stream-*`/`WorkspaceSession`/`ChatMessage` 等,见 §3)。`agent_service` 经 §1 的 Engine Actor 驱动 pi。

> 与 §3 的关系:§3 定义"翻译成什么形状",§2A.2 的 `services/` 定义"翻译代码住在哪、如何可单测"。

### 2A.3 前端结构（`ui/src/`，特性化 feature-based，原地演进）

```
ui/src/
  app/                 # 路由 / 壳装配（精简）
  shared/
    ui/                # shadcn 原语（设计系统）
    lib/  theme/  i18n/  # 通用工具 / 多主题 / i18next(en/zh)
  lib/bridge/          # IPC 桥接，按域，镜像后端 commands/
    index.ts           # 薄聚合 / re-export，不写逻辑
    client.ts          # invoke/listen 薄封装 + 错误归一
    events.ts          # agent:* / chat:stream-* 订阅工厂
    agent.ts chat.ts workspace.ts session.ts models.ts
    skills.ts memory.ts mcp.ts cron.ts terminal.ts files.ts preview.ts diagnostics.ts ui-store.ts
  features/<domain>/   # 按域自包含：components/ hooks/ atoms/ lib/ index.ts
    chat-agent/        # message view：ai-elements + agent + chat 渲染栈
    workspace/         # ARC 侧栏 + session/tab
    dock/  files/  preview/  focus-mode/  settings/  trajectory/  …
```

**前端强制纪律(写入验收项):**
1. **特性化自包含**:每个 `features/<domain>/` 自带 components/hooks/atoms/lib,经 `index.ts` barrel 暴露最小公共接口;**禁止跨特性深引用**对方内部文件(只走 `index.ts`)。
2. **文件体量上限**:组件 ≤ ~300 行、hook/atom 模块 ≤ ~200 行、bridge 单域 ≤ ~200 行,超出即拆。
3. **关注点分离**:展示组件不直接 `invoke`;数据访问走 `lib/bridge/*` + hooks(TanStack Query / Jotai),副作用集中于 hooks。
4. **共享只下沉**:跨特性复用之物放 `shared/`,不横向依赖。
5. **桥接单一入口**:所有 IPC 经 `lib/bridge/`,组件/atoms 不直接触碰 `@tauri-apps/api`;命令名/payload 类型由 `tauri-specta`/`ts-rs` **生成**,不手抄。

### 2A.4 复刻面 → 特性归类对照（承接 §4）

| §4 清单区域 | 归入 `features/` | bridge 域(`lib/bridge/`) |
|---|---|---|
| §4.1 app-shell | `app/` + `shared/` | —(git→`workspace.ts`) |
| §4.2 workspace + ARC | `features/workspace/` | `workspace.ts` |
| §4.3 tabs/sessions | `features/workspace/`(tab 子域)或 `features/session/` | `session.ts` |
| §4.4 agent message view | `features/chat-agent/` | `agent.ts` `chat.ts` `events.ts` |
| §4.5 ai-elements/composer/chat | `features/chat-agent/`(+ `shared/ui` 原语) | `chat.ts` |
| §4.6 global hooks | `features/chat-agent/hooks/` + `lib/bridge/events.ts` | `events.ts` |

> 据此,§4 中标注的 `tauri-bridge.ts` 调用点 → 改为对应 `lib/bridge/<domain>` 函数;`useGlobalAgentListeners.ts` 的裸 `listen()` → 收敛进 `lib/bridge/events.ts` 的订阅工厂(事件名不变)。

---

## 3. 后端反腐层（ACL）详细设计

### 3.1 ACL 的三个拦截 seam

前端访问后端共 3 类入口,ACL 必须覆盖全部:

| Seam | 位置 | 规模 | ACL 职责 |
|---|---|---|---|
| **S1 主桥** | `lib/tauri-bridge.ts` | 226 `invoke` + 18 `listen` | 主拦截面:命令→EngineCmd、事件订阅包装 |
| **S2 全局监听器** | `hooks/useGlobalAgentListeners.ts`(893 行,**裸 `listen()`**)、`useGlobalChatListeners.ts`、`useHomeOfficeAgentSync.ts`、`usePetStateSync.ts` | 流式事件实际落地处 | 后端必须 emit 这些事件名;前端不改 |
| **S3 直连 invoke** | 23 个文件绕过主桥直接 `invoke()`(settings×7、files-rail/preview×6、stt×3、AgentView 的 `stt_model_status`、`useGlobalAgentListeners` 的 `preview_resolve_chips` 等) | 边角 | 后端保留这些命令名;多为基建,非消息核心 |

> 结论:**消息可视主循环**只依赖 S1 的少数命令 + S2 的 5 个 `chat:stream-*` + 一组 `agent:*`。优先打通这部分,其余命令保留实现或 stub。

### 3.2 命令适配表（前端命令名不变 → pi）

| 前端命令(保留名) | 入参(保留形状) | ACL → pi |
|---|---|---|
| `send_message` / `send_agent_message` | `{sessionId??conversationId, userMessage??content, channelId, modelId, workspaceId}` | `EngineCmd::Prompt{conv_id,input,abort}` → `handle.prompt_with_abort` |
| `stop_agent` / `stop_generation` / `interrupt_current_agent_run` | `{conversationId}` | `EngineCmd::Stop` → 触发 `AbortHandle` |
| `agent_steer` / `agent_follow_up` / `queue_agent_message` | `{conversationId,text}` | `EngineCmd::Steer/FollowUp`(pi RPC `steer`/`continue_turn`) |
| `respond_ask_user` | `{requestId,answer}` | per-call `oneshot` 回填 ask_user 工具返回 |
| `respond_permission` / `approve_tool_call` | `{requestId,decision}` | 回填 `tool_approval` 的 `ToolApprovalDecision` |
| `respond_exit_plan_mode` / `respond_plan_mode_suggest` | … | plan 交互回填 |
| `create_agent_session` | `{workspaceId,…}` | uClaw 建会话行(事实源);pi 侧 `SessionOptions{no_session:true}`(F2) |
| `list_agent_sessions` | — | **读 uClaw SQLite**(消息+元数据)合成 `WorkspaceSession[]`(F2,无 pi 存储) |
| `get_agent_session_messages` / `get_messages` | `{sessionId}` | **读 uClaw SQLite**(前端契约零改);非 `handle.messages()` |
| `fork_agent_session` / `rewind_session` / `truncate_messages_from` | … | pi `fork` / 会话回退 |
| `delete/toggle_archive/toggle_pin/move_*_agent_session`、`update_agent_session_title` | … | 元数据索引(uClaw SQLite)+ pi session 联动 |
| `list_spaces`/`get_active_workspace_id`/`set_active_workspace_id`/`update_workspace`/`reorder_workspaces`/`create_space`/`delete_space` | … | **保留 uClaw 现实现**(workspace 是 uClaw 概念,pi 无),仅把 cwd→pi `working_directory` |
| `set_active_model`/`set_role_model`/`configure_provider`/`list_provider_models` | … | `handle.set_model` / provider 配置;key 解析对齐 pi `auth`/`SessionOptions.api_key` |

### 3.3 事件适配表（pi `AgentEvent` → 前端事件名）

> 前端实际在 `useGlobalAgentListeners.ts` 用**裸 `listen()`** 落地这些事件,因此后端**必须**按下表 emit。`payload` 形状须与 `lib/chat-types.ts` 对齐。

| 前端事件(必须 emit) | payload | 来源 pi `AgentEvent` | ACL 特殊处理 |
|---|---|---|---|
| `chat:stream-chunk` | `{conversationId, delta, seq}` | `MessageUpdate{TextDelta}` | **`seq` 由 ACL 单调自增合成**(pi 不提供) |
| `chat:stream-reasoning` | `{conversationId, delta, seq}` | `MessageUpdate{ThinkingDelta}` | 同上合成 seq |
| `chat:stream-tool-activity` | `{conversationId, activity:{type:'tool_start'\|'tool_result', toolName, toolCallId, input/result, durationMs, isError, timestamp}}` | `ToolExecutionStart`→tool_start;`ToolExecutionEnd{result,is_error}`→tool_result | 映射 `ToolOutput`→`result`;写类工具同步喂 `pendingWriteToolsAtom` 期望字段 |
| `chat:stream-complete` | `{conversationId, text, truncated}` | `TurnEnd`/`AgentEnd{messages}` | 取最终 assistant 文本 |
| `chat:stream-error` | `{conversationId, error}` | `AgentEnd{error}`/provider err | 用户中止映射到 `stoppedByUserSessionsAtom` 语义 |
| `agent:stream-reset` | `{conversationId}` | 自动重试/重连 | `AutoRetryStart` 时 emit |
| `agent:need_approval` | `{requestId,conversationId,toolName,input}` | `tool_approval` 回调触发 | 挂 pending 表,等 `respond_permission` |
| `agent:ask_user_request` | `{requestId,conversationId,prompt}` | ask_user 工具 execute | 挂 pending,等 `respond_ask_user` |
| `agent:exit_plan_request` / `agent:plan_mode_suggest` | … | plan 工具 | pi 需驱动,否则 plan UI 静默 |
| `agent:turn_cost` | `{conversationId,inputTokens,outputTokens,cacheReadTokens,…}` | `Usage`(在 `TurnEnd`/`AgentEnd`) | 写入 `cost_store`;pi 无成本时**置零** |
| `agent:context_stats` | `{conversationId,…}` | 由 ACL 从消息/usage 估算 | pi 无则估算或置零(驱动 `ContextRing`) |
| `session:title-pending`/`session:title-updated` | `{sessionId,title,emoji}` | ACL 自驱(可调 pi 生成标题或本地启发) | 驱动 `WorkspaceRail`/`TabBar` 标题 |
| **认知事件(uClaw 独有)** `agent:heartbeat`/`agent:stalled`/`agent:stall-recovered`/`agent:interrupted-recovered`/`agent:reflection-update`/`agent:proactive-learning`/`agent:memory-recall`/`agent:skill-recalled` | … | **pi 无对应** | **首期 no-op stub**:对应 banner/chip 不出现(可接受的优雅降级);二期用 pi `subscribe`+自定义工具补 |

### 3.4 数据模型重指向（DTO 映射）

前端类型与 pi 类型的翻译,全部在 ACL 完成,前端类型定义**不变**:

| 前端类型(`lib/*`、`atoms/*`,不变) | pi 类型 | 映射说明 |
|---|---|---|
| `ChatMessage{id,role,content,reasoning?,model?,error?,stopped?,toolActivities?,contentBlocks?,createdAt}` (`lib/chat-types.ts`) | `Message`/`AssistantMessage` | 角色/内容/思考/工具活动逐字段映射 |
| `ContentBlock = text\|thinking\|tool_use\|tool_result`(**snake_case 线缆**) | pi `ContentBlock` | **必须输出 snake_case**,`NativeBlockRenderer` 按序渲染依赖此精确形状 |
| `ChatToolActivity{toolCallId,type,toolName,status,input,result,isError,durationMs,liveOutput?}` | `ToolExecutionStart/End` + `ToolUpdate` | `liveOutput` ← `ToolUpdate` 流式片段 |
| `WorkspaceInfo`/`WorkspaceSession`(`atoms/workspace.ts`) | uClaw 自有(workspace 非 pi 概念) | **保留 uClaw 实现**;`WorkspaceSession.id` = pi session id;`spaceId`/`pinnedAt`/`archived`/`imChannelType` 仍由 uClaw 元数据索引提供 |
| `TabItem{id,type,sessionId,title,workspaceId}`(`atoms/tab-atoms.ts`) | — | tab↔pi session 经 `sessionId` 1:1 |
| `AgentSessionState`(pi) | → `list_agent_sessions` 行 | `provider/model_id/message_count` 填入会话列表 |

**持久化归属(F2 定案,覆盖原"pi 拥有存储"写法)**:**pi 无状态(`SessionOptions.no_session=true`),uClaw 为唯一事实源。**
- uClaw 既有 `db/`+`session.rs` **保持为消息正文存储**(`conversations`/`messages` 表不变);`get_messages` 仍读 uClaw SQLite(前端契约零改)。
- **每轮 prompt**:`agent_service` 从 uClaw 取该会话**完整历史** → ACL 转成 pi `Message[]` → 用 `Agent::run_with_messages_with_abort`(而非 `handle.prompt` 追加)驱动 pi;pi 不落盘、不持有跨轮状态。
- **好处**:无历史迁移、无双写、无 split-brain;压缩/记忆/技能注入时机由 uClaw 完全掌控(正好承接 F4 的 memory/skill 召回)。
- **代价**:每轮重传历史(进程内、零序列化,可接受);超长会话靠 uClaw 侧压缩/截断控制 token。
- `WorkspaceSession` 的分组/置顶/归档/im_channel/成本聚合本就在 uClaw,**无变化**。

### 3.5 配置命名空间隔离（pi → if2pi，F7 定案）

> **问题**:pi 的配置/数据全部挂在 `pi` 命名空间——全局目录 `Config::global_dir()` 默认 `~/.pi/agent/`(含 `settings.json`/`models.json`/`auth.json`/`keybindings.json`/`extension-permissions.json`/`sessions/`/`skills/`),项目级 `Config::project_dir()` **硬编码** `.pi`(`config.rs:388`,即 `<cwd>/.pi/settings.json`)。**若用户机器上还装了独立 `pi` CLI,嵌入版与它共享同一批文件 → auth/settings/sessions 互相覆盖。**
>
> **定案**:嵌入的 pi 一律读写 `if2pi` 命名空间,与独立 pi 物理隔离。Engine 线程**在构造任何 `AgentSession` 之前**设进程环境覆盖(set-once):

| pi 原始(冲突源) | 覆盖手段 | if2pi 目标 |
|---|---|---|
| 全局目录 `~/.pi/agent/`(`global_dir`,env `PI_CODING_AGENT_DIR`) | env `PI_CODING_AGENT_DIR` | `<uClaw 数据目录>/if2pi/agent/` |
| settings 解析(global + 项目 `.pi` 合并) | env `PI_CONFIG_PATH`=**绝对路径** | `<…>/if2pi/agent/settings.json`(绝对路径**直接绕过**项目级 `.pi` 合并——`load_with_roots` 在 `config_path` 为 `Some` 时只读该文件) |
| 会话目录 `~/.pi/agent/sessions/`(env `PI_SESSIONS_DIR`) | env `PI_SESSIONS_DIR` | `<…>/if2pi/agent/sessions/`(防御性;F2 `no_session=true` 下 pi 不落盘) |
| auth `~/.pi/agent/auth.json` | `SessionOptions.api_key` 直接注入 | uClaw 拥有 key,不读共享 `auth.json`;若仍走文件,亦落在重映射后的 if2pi 目录 |

> **唯一不可经 env 覆盖的点**:`project_dir()` 硬编码 `PathBuf::from(".pi")`(`config.rs:388`)。因此必须用绝对 `PI_CONFIG_PATH` 把整条 settings 解析锁死到 if2pi,使项目级 `.pi` 永不被读。
>
> **`<uClaw 数据目录>` 取值**:沿用 uClaw 既有 `~/.uclaw/`(见分析报告 §1.3),即 **`~/.uclaw/if2pi/agent/`**——所有 uClaw 拥有之物在同一卸载根下,且 `if2pi` 子树名与独立 `pi` 不撞。
>
> **接线位置**:env 覆盖写在 `crates/uclaw-pi-engine` 专用 asupersync 线程的 `actor_loop` 启动处(早于任何 `create_agent_session`);本子节是 §3 ACL 的"配置 seam",与 §3.1–3.4 的命令/事件/DTO seam 并列。

---

## 4. 前端逐文件复刻清单

**复刻动作图例**(⚠️ "原样"按 §2A.0 校正:逻辑原样、**位置重排进 `features/<domain>/`**、桥接改 `lib/bridge/<domain>`):
- 🟢 **原样复制**：组件逻辑/UI/动画 1:1,仅重新归类(纯展示,不直接 invoke)
- 🟡 **改数据源**：复制后把其 `invoke()`/事件来源指向 `lib/bridge/<domain>`→ACL(命令/事件名不变,后端换 pi)
- 🔵 **经 ACL**：依赖 ACL 合成/stub 的事件或字段(认知事件、seq、cost 等)

### 4.1 App Shell & 布局 — `components/app-shell/`

| 文件 | 动作 | 说明 / 触达 |
|---|---|---|
| `AppShell.tsx` | 🟢 | 三栏布局 `[LeftSidebar | MainArea | RightSidePanel]`;Settings overlay;swipe 限定在 LeftSidebar |
| `LeftSidebar.tsx` | 🟢 | `workspaceSlideVariants`(:111)、`GesturePreviewCard`(:130)、`useWorkspaceSwipe(sidebarRef)`(:352)、`AnimatePresence custom={switchDirection}`;新建会话按钮→`handleNewAgentSession` |
| `RightSidePanel.tsx` | 🟢 | 可折叠右栏(tab 化) |
| `ModeSwitcher.tsx`/`NavigatorPanel.tsx`/`Panel.tsx`/`PanelHeader.tsx`/`VersionWatermark.tsx` | 🟢 | 框架件 |
| `TabSessionSyncer.tsx` | 🟡 | tab↔session 同步;读 `list_agent_sessions` |
| `WorkspaceTabCleaner.tsx` | 🟢 | 关闭已失效 workspace/session 的 tab |
| `SidebarGitActions.tsx` | 🟡 | git IPC(`activeWorkspaceCwdAtom`、`branchSyncTickAtom`);命令保留 |

### 4.2 Workspace + ARC — `components/workspace/`、`hooks/useWorkspaceSwipe.ts`、`atoms/workspace.ts`

| 文件 | 动作 | 说明 / 触达 |
|---|---|---|
| `atoms/workspace.ts` | 🟡 | `WorkspaceInfo`/`WorkspaceSession`/`SwipeGestureState` + 全部原子;命令 `listSpaces/getActiveWorkspaceId/setActiveWorkspaceId/updateWorkspace/reorderWorkspaces`(经 ACL,后端保留 uClaw 实现) |
| `hooks/useWorkspaceSwipe.ts` | 🟢 | `useWorkspaceSwipe(scopeRef)` + `useWorkspaceArrowSwitch()`;纯手势→`swipeGestureAtom`/`selectWorkspaceAtom` |
| `WorkspaceRail.tsx` | 🟡 | 会话列表;`togglePin/toggleArchive/deleteAgentSession` |
| `WorkspaceSwitcherBar.tsx` | 🟢 | 图标密度坍缩(ResizeObserver、≤5 全图标 / >5 active=图标+其余 6px 点);见 §6 |
| `WorkspaceHeader.tsx`/`SessionItem.tsx`/`IconPicker.tsx`/`WorkspaceCreateDialog.tsx` | 🟢/🟡 | header/会话项/图标选择/新建;SessionItem 标题来自 `session:title-*` |
| `lib/workspace-icons.ts` | 🟢 | `getWorkspaceIcon()`(兼容 legacy emoji + lucide) |

### 4.3 Tabs / Sessions — `components/tabs/`、`atoms/tab-atoms.ts`

| 文件 | 动作 | 说明 / 触达 |
|---|---|---|
| `atoms/tab-atoms.ts` | 🟡 | `TabItem`、`tabsAtom`、`visibleTabsAtom`、`activeTabIdAtom`、`tabMruAtom`、`tabStreamingMapAtom`(派生自 agent streaming)、`tabIndicatorMapAtom` |
| `TabBar.tsx`/`TabBarItem.tsx`/`TabBarWorkspaceChip.tsx` | 🟢 | tab 条;chip 为补充视觉 |
| `MainArea.tsx`/`TabContent.tsx` | 🟡 | `TabContent` 按 `type`(`chat\|agent\|browser\|symphony`)分发到 AgentView 等 |
| `TabSwitcher.tsx`/`TabPreviewPanel.tsx`/`TabCloseConfirmDialog.tsx`/`TabErrorBoundary.tsx`/`index.ts` | 🟢 | 切换/预览/关闭确认/错误边界 |

### 4.4 Agent Message View（复刻核心）— `components/agent/`

> 决策 5 的 1:1 复刻核心。**全部 🟢 原样复制**(纯渲染),数据从 atoms 读取;atoms 由 S2 监听器经 ACL 喂入。少数 🔵 依赖被 stub 的认知事件。

**顶层与渲染栈(原样复制):**
- `AgentView.tsx`(1926 行,🟡:`getAgentSessionPath/getAgentSessionMessages/sendAgentMessage/stopAgent/createAgentSession/forkAgentSession/rewindSession/agentSteer/agentFollowUp/queueAgentMessage` 经 ACL;读 `agentStreamingStatesAtom`/`liveMessagesMapAtom`/`skillRecallsMapAtom`/`agentStreamErrorsAtom`/`allPendingAskUserRequestsAtom`/`allPendingExitPlanRequestsAtom`;另 `stt_model_status` 直连)
- `AgentMessages.tsx`(1267 行,🟢:消息列表;`NativeBlockRenderer`/`ToolActivityList`/`ThinkingBlock`/`CompactingIndicator`/`CompactBoundaryDivider`、`normalizeAgentMarkdown`、`parseSkillCitations`、平滑滚动)
- `SDKMessageRenderer.tsx`(1150 行,🟢)、`NativeBlockRenderer.tsx`(🟢,按序渲染 `ContentBlock[]`,**依赖 snake_case 形状**)、`ContentBlock.tsx`(609 行,🟢,含 `ThinkingBlock`)、`ToolActivityItem.tsx`(🟢,`ToolActivityList`)

**工具结果渲染器 `tool-renderers/`(全 🟢):** `index.tsx`、`bash-result.tsx`、`BashStreamView.tsx`、`read-result.tsx`、`write-result.tsx`、`edit-result.tsx`、`screenshot-result.tsx`、`gbrain-result.tsx`、`collapsible-result.tsx`、`default-result.tsx`、`pierre-theme.ts`。辅助:`tool-phrase.ts`、`tool-utils.ts`。

**交互 banner / 面板:**
- 核心交互(🟡,后端须 emit 对应事件)：`AskUserBanner.tsx`、`PermissionBanner.tsx`/`PermissionModeMenu.tsx`/`PermissionModeSelector.tsx`、`ExitPlanModeBanner.tsx`/`PlanModeSuggestBanner.tsx`/`PlanModeDashedBorder.tsx`/`PlanViewer.tsx`、`QueuedMessagesBanner.tsx`
- 认知降级(🔵,首期 stub→不显示)：`AgentHeartbeatBanner.tsx`、`SkillCitationChips.tsx`/`SkillRecallChips.tsx`/`SkillSuggestionBar.tsx`、`SessionEvalBadge.tsx`、`TrajectoryReel.tsx`、`AgentStatusBar.tsx`(部分字段)、`ContextUsageBadge.tsx`(🔵 cost/context)
- 自包含面板(🟢/🔵,事件缺则休眠)：`ActiveTasksBar.tsx`、`TaskBadge.tsx`/`TaskProgressCard.tsx`、`BackgroundTasksPanel.tsx`、`AgentTeamsPanel.tsx`/`TeamNode.tsx`、`AutomationRunBanner.tsx`、`ModeBanner.tsx`、`StrategyPresetSelector.tsx`、`BrowserViewer.tsx`/`BrowserPreviewOverlay.tsx`/`AutoPreviewPopover.tsx`、`ChannelFeed.tsx`、`PetWidget.tsx`、`SidePanel.tsx`、`MoveSessionDialog.tsx`、`AgentHeader.tsx`、`AgentPlaceholder.tsx`、`index.ts`

### 4.5 ai-elements / composer / chat

- `components/ai-elements/`(全 🟢)：`message.tsx`(`Message/MessageHeader/MessageContent/MessageActions/MessageResponse`)、`reasoning.tsx`(`Reasoning/ReasoningTrigger/ReasoningContent`,`isStreaming`)、`conversation.tsx`(`Conversation/ConversationContent/ConversationScrollButton`、`useConversationContext`)、`sticky-user-message.tsx`、`scroll-minimap.tsx`、`rich-text-input.tsx`、`context-divider.tsx`、`provider-avatar.tsx`、`speech-button.tsx`
- `components/composer/`(全 🟢)：`ComposerMentionController.tsx`、`ComposerMentionPopup.tsx`、`MentionChipNode.ts`(`chipToWireText`)、`composer-serialize.ts`(`serializeDocToWireText`)
- `components/chat/`(35 文件,🟡:遗留 chat 路径)：`ChatView.tsx`/`ChatMessages.tsx`/`ChatMessageItem.tsx`/`ChatToolBlock.tsx`/`ChatToolActivityIndicator.tsx`/`ParallelChatMessages.tsx`/`ChatInput.tsx`/`ModelSelector.tsx`/`MemoryRecallChip.tsx`(🔵)/`ProactiveLearningChip.tsx`(🔵)/`TurnCostBar.tsx`(🔵)/`ContextRing.tsx`(🔵) 等;流式 IPC 已迁到 `useGlobalChatListeners`

### 4.6 全局监听器 & 状态同步 — `hooks/`

| 文件 | 动作 | 事件→atom 接线(后端须 emit) |
|---|---|---|
| `useGlobalAgentListeners.ts`(893 行,**裸 listen**) | 🟡/🔵 | `chat:stream-chunk/-reasoning/-complete/-error/-tool-activity`→`agentStreamingStatesAtom`/`liveMessagesMapAtom`/`agentStreamErrorsAtom`/`pendingWriteToolsAtom`;`agent:stream-reset`、`session:title-*`、`agent:turn_cost`(🔵)、`agent:context_stats`(🔵)、`agent:skill-recalled`(🔵)、`agent:memory-recall`(🔵)、`agent:proactive-learning`(🔵)、`browser:task-run/-step`、`preview_resolve_chips`(直连 invoke) |
| `useGlobalChatListeners.ts`(232) | 🟡 | 遗留 chat 流式(用桥 wrapper)+ `registerPendingTitle` |
| `useHomeOfficeAgentSync.ts`(56) | 🟢 | `chat:stream-*`+`agent:stream-reset`→home-office 面板 |
| `usePetStateSync.ts`(133) | 🟢 | `chat:stream-*`+`agent:stream-reset`+`chat:pet-celebrate`→pet 动画 |

---

## 5. Workspace + ARC 式切换 复刻专节（决策 6）

ARC 式切换 = **左栏整屏在 workspace 之间横向滑动切换**,有手势拖拽实时跟手 + 松手定格的双态动画。复刻三块:

### 5.1 数据与方向计算(`atoms/workspace.ts`)
- `selectWorkspaceAtom`(:178)是切换枢纽:**在翻转 `activeWorkspaceIdAtom` 之前**先算方向写入 `workspaceSwitchDirectionAtom`('forward'=去更大 sortOrder→从右滑入,'backward'=从左滑入);支持调用方传 `{id,direction}` 覆盖(键盘环绕/swipe 用),否则按 sortOrder 索引比较。**复刻时这段时序不可改**:消费者(LeftSidebar/TabBar/RightSidePanel)需在同一渲染读到正确方向。
- `swipeGestureAtom`(:104,`{offsetPx,containerWidth,previewWorkspaceId}`):拖拽中非空,记录当前 workspace 的视觉位移(经橡皮筋阻尼)与被预览的目标 workspace;松手归 null,交还 `AnimatePresence` 的常规 cross-pass。

### 5.2 滑动动画(`LeftSidebar.tsx`)
- `workspaceSlideVariants`(:111):`enter` `x:'100%'/'-100%'`(随 custom 方向)、`center` `x:'0%'`、`exit` 反向;**纯平移、无淡入淡出**——这是 ARC/iOS 观感的关键,复刻须保留 transform-only。
- `AnimatePresence custom={switchDirection}` 包 `motion.div variants={workspaceSlideVariants}`;`GesturePreviewCard`(:130)在拖拽时显示目标 workspace 预览。
- 手势源 `useWorkspaceSwipe(sidebarRef)`(:352):trackpad 双指/拖拽 → 写 `swipeGestureAtom` 实时跟手 → 过阈值松手 → `selectWorkspaceAtom({id,direction})` 提交。

### 5.3 图标密度坍缩(`WorkspaceSwitcherBar.tsx`)
- `ResizeObserver` 测容器宽;slot 预算 = 28px + 4px gap(首图标不计前导 gap)。
- ≤5 workspace → 全尺寸图标(24/28px);>5 → active=图标,其余坍缩为 6px 圆点;iOS 式"图标让位"靠对非拖拽图标的 CSS transition。
- 复刻须保留该测量/坍缩逻辑,否则多 workspace 时溢出。

> ARC 复刻验收:多 workspace 下 swipe/箭头/点击切换,方向正确、跟手、松手定格、图标坍缩与 uClaw 像素级一致;且**切换不重建 AgentView/会话状态**(workspace 切换只换可见 tab 集,见 `visibleTabsAtom`)。

---

## 6. Agent Message View 1:1 复刻专节（决策 5）

**目标**:`components/agent/` 整套渲染栈 + `ai-elements/` 逐文件复制,**仅数据源替换**。实现靠"前端契约 + 后端反腐层":

1. **前端契约不变**:渲染只读 `atoms/agent-atoms.ts`(`agentStreamingStatesAtom`/`liveMessagesMapAtom`/`AgentStreamState`/`LiveOutput`/`ToolActivity`/`ActivityGroup`)与 `lib/chat-types.ts`(`ChatMessage`/`ContentBlock`/`ChatToolActivity`)。这些类型与 atom 名**一字不改**。
2. **事件由 ACL 喂入**:`useGlobalAgentListeners.ts` 的 5 个 `chat:stream-*` 必须由后端按 §3.3 形状 emit。`NativeBlockRenderer` 按序渲染 `ContentBlock[]`,**强依赖 snake_case 线缆**(`text/thinking/tool_use/tool_result`)——ACL 必须把 pi 的 `ContentBlock`/`AssistantMessage` 转成此精确形状。
3. **流式细节对齐**:`seq` 单调递增由 ACL 合成(去重/排序);思考流→`chat:stream-reasoning`(`reasoning.tsx` 的 `isStreaming`);工具活动 start/result + `liveOutput`(←pi `ToolUpdate`)。
4. **交互闭环**:`AskUserBanner`/`PermissionBanner`/`ExitPlanModeBanner` 依赖 `agent:ask_user_request`/`agent:need_approval`/`agent:exit_plan_request` + `respond_*`/`approve_*` 命令;ACL 用 per-request `oneshot` + pending 表回填 pi 的 `tool_approval`/ask_user 工具(分析报告 §5.4)。
5. **降级项明确**:认知 chip/banner(skill-recall/memory-recall/heartbeat/reflection/proactive-learning)首期 stub→不显示,不破坏布局;二期补齐。

> 复刻验收:同一组对话脚本下,消息气泡、markdown/代码块、工具卡片(bash/read/write/edit/screenshot)、思考块、压缩分隔、滚动 minimap、流式光标的渲染与 uClaw **视觉一致**;审批/询问/中止/`/compact` 交互可用。

---

## 7. 分阶段实施（前端复刻 + ACL 视角）

> 与分析报告 Phase 0–5 对齐,这里聚焦"复刻 + ACL"的可操作切片。每阶段门禁:`ui/` 无功能性 diff + 既有交互回归通过。

- **R0 · 进程内引擎探针(阻断式 go/no-go,F3 定案)**：跑通 `examples/basic_sdk.rs`;**实测 `pi`+`asupersync` 能否在 stable rustc 1.85 编译**(不行则评估 uClaw 转 nightly 的代价,见 §0B ⚠️);搭最小 **Engine Actor**(专用 asupersync 多线程 + 一条 mpsc + `on_event`→tokio)直接进程内打通 `send_message→chat:stream-chunk→chat:stream-complete`(**不经 sidecar**),验证 §3.3 映射。**探针即设 `PI_CODING_AGENT_DIR`/`PI_CONFIG_PATH`(绝对)/`PI_SESSIONS_DIR` 指向 `~/.uclaw/if2pi/agent`(F7),全程不得污染 `~/.pi/` 或项目级 `.pi`。** **此结论是后续一切前置条件。**
- **R1 · 前端整树复刻 + ACL 骨架**：把 `ui/` 按 §4 清单整树复制到新基座;新建 `crates/uclaw-pi-engine`(Engine Actor + 专用 asupersync 线程 + `EngineCmd` + 事件翻译);先实现 `Prompt/Stop` 与 5 个 `chat:stream-*`;`AppState.manage(pi_engine)`。
- **R2 · 消息核心闭环**：`AgentView`/`AgentMessages`/`NativeBlockRenderer` 跑通真实 pi 流;`get_messages` 读 uClaw SQLite(F2);每轮历史经 ACL→pi `Message[]`;`ContentBlock` snake_case 映射;seq 合成。
- **R3 · 交互 + workspace/session(F2 无状态)**：审批/ask_user/plan 回填;workspace CRUD(保留 uClaw 实现)+ ARC 切换跑通;`list_agent_sessions` 合成 `WorkspaceSession[]`;**持久化保持 uClaw 为唯一事实源**——`agent_service` 每轮从 uClaw 取全量历史经 ACL 喂 pi(`run_with_messages`),pi `no_session=true`,**不引入 pi 存储/迁移**。
- **R4 · 工具/MCP/模型(F5)**：`UclawToolFactory` 注入——**内置工具用 pi**,ACL 把 pi `ToolOutput` 归一成 `tool-renderers/` 期望形状;MCP/浏览器/skill 包成 `impl pi::sdk::Tool`;`set_model`/provider 配置对接,**key 解析对齐 if2pi(F7):auth 经 `SessionOptions.api_key` 注入,pi 配置/models 全在 `~/.uclaw/if2pi/`,不读共享 `~/.pi/agent/auth.json`**。**形态自始至终是进程内**(F3,无 `SessionTransport` 切换动作)。
- **R5 · 清理硬化 + 二期认知**：删除 uClaw 旧 agent **执行层**(agentic_loop/dispatcher/llm/providers 等,分析报告 §7.2),但**保留 memory/skill 服务**(F4:它们驱动 v1 召回 chip,移入 `services/memory_service`、`services/skill_service`);二期再议 heartbeat/reflection/proactive 等 stub 项;CI 工具链定基线(stable **1.95**,R0 裁决:较新 stable 非 nightly);成本/中止/压缩/会话切换/召回 chip 全量回归。

---

## 8. 复刻验收门禁与风险

### 8.1 门禁(每阶段必过)
1. **前端零改动**:`ui/` 相对 uClaw 源无功能性 diff;`tauri-bridge.ts` 的 226 invoke + 18 listen 名称与 payload 不变。
2. **事件契约回归**:断言后端按 §3.3 形状 emit 5 个 `chat:stream-*` + `agent:need_approval`/`ask_user_request`/`exit_plan_request`/`turn_cost`/`stream-reset` + `session:title-*`。
3. **交互 e2e**:流式回显(文本/思考/工具)、审批通过/拒绝、ask_user、`stop_agent` 中止、`/compact`、成本累计(或归零占位)、workspace 切换(ARC)、tab/session 增删改、会话标题。
4. **运行时隔离审计**:grep 确认无"tokio 任务直接 await pi future";pi 仅在专用 asupersync 线程构造/驱动。
5. **构建门禁**:stable rustc **1.95**(R0 实测下限,非 nightly)全量 `cargo build --release` 通过,单二进制可启动并完成一次完整对话。
6. **配置隔离审计(F7)**:运行一轮完整对话后 `~/.pi/agent/` 与任意 `<cwd>/.pi/` **零新增/零改写**;所有 pi 配置/数据落 `~/.uclaw/if2pi/`(确认 `PI_CODING_AGENT_DIR`/`PI_CONFIG_PATH`/`PI_SESSIONS_DIR` 已在构造会话前生效)。

### 8.2 风险与缓解(复刻特有,补充分析报告 §9)
| 风险 | 影响 | 缓解 |
|---|---|---|
| `ContentBlock` 形状不符(camelCase/字段缺) | 消息错位/空白 | ACL 严格输出 snake_case;加映射单测对照 `lib/chat-types.ts` |
| `seq` 缺失导致乱序/重复 | 流式抖动 | ACL 维护 per-conv 单调 seq |
| 认知事件 stub 导致面板空白 | 视觉非 1:1 | 明确"降级清单";二期补;门禁不卡这些 |
| 23 个直连 invoke 被遗漏 | settings/STT/preview 局部失效 | R1 建直连命令清单,逐个保留实现或 stub |
| workspace 概念 pi 无 | 列表/切换断裂 | workspace 保留 uClaw 实现,仅 cwd→pi `working_directory` |
| 审批回填竞态 | 卡审批 | per-request oneshot + pending 表 + 超时/取消传播 |
| 单进程 `panic=abort`(F3 无对冲) | 崩溃即退,**已接受** | 输入校验 + 边界隔离 + 监督重启 Engine 线程;根因消解留二期 |
| **pi 无法在 stable 1.85 编译**(F3 无对冲) | **路线阻断** | R0 阻断式门禁先验;失败则 uClaw 转 nightly(已接受)或重开 F3 |
| **嵌入 pi 与独立 pi CLI 共享 `~/.pi/agent/`**(F7) | auth/settings/sessions 互相覆盖 | Engine 启动设 `PI_CODING_AGENT_DIR`/`PI_CONFIG_PATH`(绝对)/`PI_SESSIONS_DIR`→`~/.uclaw/if2pi/`;项目级硬编码 `.pi` 经绝对 `PI_CONFIG_PATH` 绕过;§8.1 门禁 6 审计零污染 |

---

## 附录 A · 关键文件索引（复刻锚点）
- 前端契约:`ui/src/lib/tauri-bridge.ts`(226 invoke/18 listen)、`ui/src/hooks/useGlobalAgentListeners.ts`(裸 listen 落地)、`ui/src/lib/chat-types.ts`、`ui/src/lib/agent-types.ts`、`ui/src/atoms/{workspace,tab-atoms,agent-atoms}.ts`。
- ARC:`ui/src/components/app-shell/LeftSidebar.tsx:111`、`ui/src/hooks/useWorkspaceSwipe.ts`、`ui/src/components/workspace/WorkspaceSwitcherBar.tsx`。
- 消息核心:`ui/src/components/agent/{AgentView,AgentMessages,SDKMessageRenderer,NativeBlockRenderer,ContentBlock,ToolActivityItem}.tsx` + `tool-renderers/`、`ui/src/components/ai-elements/{message,reasoning,conversation}.tsx`。
- 后端:新 `crates/uclaw-pi-engine`(Engine Actor/ACL)、`src-tauri/src/app.rs`(AppState)、`src-tauri/src/main.rs`(启动专用线程 + manage)、`src-tauri/src/tauri_commands.rs`(命令体改发 EngineCmd)。
- pi:`src/sdk.rs`(`create_agent_session:1651`、`SessionTransport:646`、`tool_approval:None@1765`)、`src/agent.rs`(`AgentEvent:935`)、`examples/basic_sdk.rs`。
- pi 配置命名空间(F7 锚点):`src/config.rs`(`global_dir`→`PI_CODING_AGENT_DIR` 默认 `~/.pi/agent`:1025、`project_dir`硬编码 `.pi`:388、`config_path_override_from_env`/`PI_CONFIG_PATH`:377、`sessions_dir`/`PI_SESSIONS_DIR`:1040、`auth_path`:412)、`src/tools.rs`(agent-dir 读放行注释:2069)。

## 附录 B · v1 事件分级（F4 定案）

**v1 必做(不 stub):**
- `agent:turn_cost` — 从 pi `Usage`(`TurnEnd`/`AgentEnd`)直接算,写 `cost_store`。
- `agent:context_stats` — 从 usage/历史估算 token 窗口,驱动 `ContextRing`/`ContextUsageBadge`。
- `agent:skill-recalled` + `agent:memory-recall` — 由**保留的 uClaw skill/memory 服务**驱动:召回→注入 prompt→emit chip(契合 F2 uClaw 拥有一切)。对应 `SkillRecallChips`/`MemoryRecallChip` v1 即可见。

**v1 stub/隐藏(保布局、不报错,二期再议):**
`agent:heartbeat`、`agent:stalled`、`agent:stall-recovered`、`agent:interrupted-recovered`、`agent:reflection-update`、`agent:proactive-learning`、`budget:threshold`,以及 symphony / gbrain / teams / GEP / home-office / pet 相关事件(对应面板休眠)。
