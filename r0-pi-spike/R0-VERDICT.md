# R0 裁决 · pi + asupersync 进程内嵌入可行性(F3 阻断式 go/no-go 门禁)

> 日期 2026-05-30 · 探针 `r0-pi-spike/` · 作者 R0 自动化验证
> 关联:`uclaw-pi-implementation-replication-plan.md` §0B F3 / §1 / §7 R0 / §8;`uclaw-pi-agent-migration-analysis.md` §2.5 / §5.2 / §10。

## 裁决(一行)

**GO —— 整条迁移可走「进程内嵌入 + 专用 asupersync 线程 + 通道桥」,且全程 STABLE,绝不需要 nightly。**
**但有一处必须修正计划假设:最低 stable 不是 1.85,而是一个「较新的 stable」(实测 1.95.0 通过;下限 > 1.88)。**

- F3 的 NO-GO 触发条件是「只在 nightly 能编译 / 需 `RUSTC_BOOTSTRAP=1`」——**未触发**。pi(+asupersync)在 stable 完整编译并运行通过。
- 代价仅为「把工具链下限从计划写的 1.85 上调到较新的 stable」,这远好于 F3 已接受的最坏情况(「uClaw 全量转 nightly」)。**无需重开 F3,无需 sidecar 对冲。**

---

## 关键证据

### 1) 构建闸门(Done-when #1)——三级台阶,结论是「stable,但要够新」

| rustc | 结果 | 卡在哪 | 性质 |
|---|---|---|---|
| **stable 1.85.0** | ❌ 解析期失败(0.6s,未编译任何 crate) | `vergen-gix 9.1.0`(pi 的 **build-dep**,MSRV 1.88)+ `vergen`/`vergen-lib`(1.88)+ `cargo_metadata 0.23.1`(1.86)+ `sysinfo 0.38.4`(1.88)+ `time 0.3.47`(1.88)。这些依赖**无任何 ≤1.85 版本**能满足 pi 的语义约束。 | **stable 版本下限**(MSRV 解析),非 nightly |
| **stable 1.88.0** | ❌ 编译期失败(asupersync 编译到一半,22 个错误) | `asupersync 0.3.2` 自身用了 **unstable 库 API `Duration::from_mins`**(feature `duration_constructors`, rust#120301),在 1.88 报 `E0658`。出现在 `asupersync-0.3.2/src/web/middleware.rs:150`、`messaging/kafka.rs:1729` 等处。 | **stable 新 API 下限**(该 API 后来在更晚的 stable 才稳定),非 nightly |
| **stable 1.95.0** | ✅ **退出 0** | —— | pi + asupersync + 全部 556 个 crate 干净编译;唯一一次报错是探针自己的 API 误用(`AssistantMessage::new`,已改 `::default()`),与 pi/nightly 无关。 |

证据行(最终,stable 1.95.0):
```
rustc 1.95.0 (59807616e 2026-04-14)
   Compiling r0_pi_spike v0.0.0 (/Users/ryanliu/Documents/uclaw-pi/r0-pi-spike)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.20s   # 增量(仅探针)
# 一次性冷构建(pi + 556 crate):real 155.72s(~2.6 分钟),user 86.73s
```

> **为何不是 nightly:** 两道台阶——(①)传递依赖 MSRV 抬到 1.88;(②)asupersync 用的 `Duration::from_mins` 是「后来稳定的 stable API」——**都是 stable 版本下限问题**。在足够新的 stable 上,①的依赖 MSRV 被满足、②的 API 已稳定,于是正常 stable 编译。**全程没有 `#![feature(...)]` 源码、没有 `RUSTC_BOOTSTRAP`、没装也没用 nightly。**

> **`asupersync default-features` 状态:** 探针与 pi 都以 `default-features = false` 引入 asupersync(探针 `features = ["tls-native-roots"]`;pi `["tls-native-roots","test-internals"]`)。关掉 default features 不能绕过 `from_mins`(它在 `web`/`messaging` 模块,属被默认编译路径)。

> **`panic = "abort"`:** pi 仅在 `[profile.release]` 设 `panic = "abort"`(`pi Cargo.toml:342`);探针走 dev profile(unwind),未触发 abort 语义。F3 已接受「单进程 pi panic=abort 会拖垮整个 app」——本探针不改变该结论,仅说明 release 嵌入时该风险照旧存在。

### 2) Engine Actor 桥 + 事件序列(Done-when #2)

专用 asupersync 线程跑 `current_thread` runtime;事件经 **`std::mpsc`(纯数据)** 回主线程(= 真实 uClaw 的 tokio 侧镜像)。捕获序列:
```
AgentStart  →  MessageUpdate(TextDelta)  →  MessageUpdate(TextDelta)  →  TurnEnd  →  AgentEnd
```
> **诚实标注:** 本机**无 API key、无 `~/.config/pi/auth.json`**,`create_agent_session` 在 `resolve_api_key` 处真实失败(捕获:`Validation("No API key found for provider anthropic ...")`)——这与编译闸门无关,纯属环境缺凭证。因此上面的事件序列由**确定性注入「真实 pi `AgentEvent` 类型」**(`pi::sdk::AgentEvent` / `pi::model::AssistantMessageEvent::TextDelta` 等,**非自造 mock**)经**同一条** `demux → mpsc 桥 → ACL` 代码路径产生。探针已实现并编译通过 `MODE=live`(真实 prompt)分支;设 `ANTHROPIC_API_KEY` 后 `cargo run` 即走真实流式,无需改码。

### 3) §3.3 翻译 seam(Done-when #3)——合成出的前端 payload

ACL 住在主线程(tokio 侧),`seq` 由 ACL 单调自增,`text` 由 TextDelta 累积:
```
chat:stream-chunk     {"conversationId":"r0-conv-1","delta":"pong","seq":0}
chat:stream-chunk     {"conversationId":"r0-conv-1","delta":" from the void","seq":1}
chat:stream-complete  {"conversationId":"r0-conv-1","text":"pong from the void","truncated":false}
```
- `MessageUpdate{TextDelta}` → `chat:stream-chunk{conversationId,delta,seq}`,seq=0,1 单调。
- `TurnEnd`/`AgentEnd` → `chat:stream-complete{conversationId,text,truncated}`(两者中最先到达者产出一次,避免重复)。
- 形状与 `ui/src/lib/chat-types.ts` 对齐(camelCase 键、`truncated` 字段就位)。

### 4) 运行时隔离审计(Done-when #4)——grep 结果(应为空,确为空)

```
[#4b] tokio::spawn          → <空:无 tokio::spawn>
[#4a] 任何 tokio 依赖/用法   → 仅注释里出现 "tokio"(解释架构);Cargo.toml 无 tokio 依赖
[#4c] .await 仅 2 处         → create_agent_session(opts).await、handle.prompt(...).await
                               两处都在 async fn engine_async 内,由 runtime.block_on 在
                               **专用 asupersync 线程**驱动;主线程 .await 数 = 0
[#4e] 桥 = std::mpsc::channel(纯数据),非 runtime 句柄
```
→ **硬运行时边界成立**:没有任何 tokio 任务 `.await` 或 `spawn` 一个 pi future;跨边界只过 `RawEvt`/payload 这类 owned 数据。

---

## F3 闸门显式回答(Done-when #6)

> 问:**「整条迁移走进程内,还是 uClaw 必须转 nightly / 重开 F3?」**

**答:走进程内(GO)。uClaw 不必转 nightly,不必重开 F3。** 依据:

1. **进程内 + 专用 asupersync 线程 + mpsc 桥**已端到端打通(编译 0 退出、桥成立、§3.3 seam 成立、运行时隔离审计干净)。架构方向(分析报告 §5.2 Engine Actor / 计划 §1)成立。
2. pi(+asupersync)**在 stable 完整编译运行**;nightly NO-GO 触发条件未发生。
3. **唯一须改的计划假设:工具链下限**。计划 §0B/§2.5/§7/§8 多处写「stable ≥1.85」——**实测错误**。真实下限是「较新的 stable」:
   - 下界 **> 1.88**(被 pi build-dep `vergen-gix 9.1.0` 与 asupersync 的 `Duration::from_mins` 共同抬高);
   - **1.95.0 实测可用**(build+run 退出 0)。
   - 建议:uClaw R1+ 把工具链基线钉为**一个较新的 stable**(例如随 R0 实测的 1.95,或 ≥ 稳定 `duration_constructors` 的那个发行),CI 同步。**这是 stable 内部的版本调整,不是转 nightly。**

### 对计划/分析两文档的勘误(R1 前需就地更正)
- ❌「消费端只需 rustc ≥1.85」/「edition 2024 于 1.85 稳定即够」——**不够**。edition 没问题,但 pi 的**依赖图**(build-dep vergen-gix、dep sysinfo/time)+ **asupersync 源码**(`Duration::from_mins`)把实际下限抬到「较新 stable(>1.88,实测 1.95)」。
- ✅「pi 锁 nightly 只是其自身开发约定」——**部分成立但有坑**:pi 自身代码确实没用 nightly 语言特性,但它依赖的 **asupersync 0.3.2 用了一个当时还 unstable、后来才在 stable 稳定的库 API**。结论仍是 stable-able,只是 stable 必须够新。
- 影响:F3「若 pi 需 nightly 则 uClaw 转 nightly」的对冲分支**不会被触发**;`panic="abort"` 风险条目不变。

---

## Done-when 勾稽

| # | 要求 | 状态 | 说明 |
|---|---|---|---|
| 1 | `cargo build` stable 退出 0 | ✅(已修正版本) | **1.85/1.88 不行**(均非 nightly 原因),**stable 1.95.0 退出 0**;`rust-toolchain.toml` 已钉 `stable` 并注明真实下限 |
| 2 | 专用 asupersync 线程 + mpsc 桥驱动、捕获事件序列 | ✅(注入真实类型) | 序列 AgentStart→TextDelta×2→TurnEnd→AgentEnd;**live prompt 仅缺环境 API key**,已留 `MODE=live` 现成路径 |
| 3 | §3.3 seam payload + 单调 seq | ✅ | chat:stream-chunk(seq 0/1)+ chat:stream-complete(累积 text、truncated) |
| 4 | grep 审计无 tokio poll pi future | ✅ | 见上;主线程零 `.await`,桥为 std::mpsc |
| 5 | 写 R0-VERDICT.md | ✅ | 本文件 |
| 6 | 显式回答 F3 闸门 | ✅ | GO·进程内·stable(须够新)·不转 nightly·不重开 F3 |

## 复现实验
```bash
cd r0-pi-spike && cargo build && cargo run        # 用 rust-toolchain.toml 钉的 stable(本机=1.95.0)
# 跑真实 live prompt(需凭证):
ANTHROPIC_API_KEY=sk-... cargo run
```

## 残留风险 / 给 R1 的输入
- **工具链下限**:R1 起把 uClaw 工具链/CI 钉到较新 stable(≥ 稳定 `duration_constructors` 的发行;实测 1.95 可用)。两份设计文档里所有「1.85」需改为该值。
- **凭证缺失**导致本轮未跑 live 流式;桥/seam 已用真实 pi 类型证伪式验证。R1 在有 key 的环境补一次 `MODE=live` 真流回归即可。
- **`panic="abort"`**(release)单进程崩溃风险按 F3 既定接受,缓解留二期。
- **冷构建 ~2.6 分钟 / 556 crate**:体积与时长按分析报告 §9 预期,可按 feature 裁剪(wasm-host 等默认已关)。
