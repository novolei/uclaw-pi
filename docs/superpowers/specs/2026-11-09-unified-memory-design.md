# uClaw-pi 统一记忆系统升级方案（草案，待批准）

日期：2026-11-09 · 状态：**提案，未动代码** · 基线：`docs/eval/2026-11-09-memory-recall-baseline.md`

## 1. 一句话

把现在约 15 个各自为政的记忆存储、7 份"用户画像"、2 个外部运行时（Python memU、Bun gbrain），
收敛成 **一个 Rust 记忆内核（MemoryCore，挂在现有 `MemoryAdapter` 之后）+ 一条学习闭环**；
检索用社区成熟的 Rust 组件（fastembed-rs / jieba-rs / SQLite），架构照搬当前最强框架的设计
（Hindsight 检索、mem0 抽取、Graphiti 双时间、Letta/Anthropic 睡眠整理、ACE 增量手册、Hermes 技能闭环）。

## 2. 现状诊断（已核实）

### 2.1 召回基线（M0，37 条真实中英问题）

| 指标 | 数值 |
|---|---|
| 事实确实存过 | 94.6% |
| 图谱五层召回 top-10 命中 | 16.2% |
| bucket_seal 召回命中 | 0% |
| 和问题相关的记忆进入 pi 提示词 | **0 / 37** |

两条根因：
- **检索**：只拿整句做关键词匹配，没有向量和重排；英文存、中文问对不上。
- **预算**：每轮固定塞 5–10 条"已学技能"在最前面；pi 的上限按**字节**算，8000 字节≈2700 个汉字，相关记忆永远被截掉。

### 2.2 结构问题（摘自全量盘点，见附录 A）

- **7 份用户画像**：facets、user_model、图谱 UserProfile 节点、personality、PROFILE.md、memU profile、gbrain 用户页。其中 3 份同时进 prompt，互不协调。
- **只写不读**：bucket_seal（默认）、failure_memory、personality_model、drift、ConversationBridge。ConversationBridge 写进 uclaw.db 的队列，而 Memorization 读的是 memorization.db，写的东西永远没人处理。
- **死代码**：spaced_repetition、triangulation、temporal_classifier、task/code_memory、feedback、memory_policy 的执行部分、`memory_unified_*` IPC。
- **策略自相矛盾**：
  - ADR 说"冻结 memory_graph、以 gbrain 为主"，但图谱是唯一会自动进 prompt 的库；
  - gbrain 在 pi 路径上没有任何自动写入。
- **pi 路径缺口**：
  - pi 内置工具不发 `ToolExecuted`，技能抽取、工具记忆没有输入；
  - chat-pi 不发任何事件；
  - 没有通用的 `memory_search` / `remember` 工具；
  - 反思计数器和 FacetCache 重启后清零。
- **隐性成本**：每轮约 12k 字的记忆块拼在用户消息前面，并被 pi 存进会话历史，越聊越大。

## 3. 选型结论（2026-11 调研，附录 B/C）

**结论：社区里还没有成熟的、Rust 原生、进程内的"mem0"。** mem0 / Graphiti / Letta / MemOS / Hindsight / cognee
都是 Python 并且要外部服务；Rust 的移植都在个位数 star，Cortex-Mem 也需要 Qdrant 服务。
所以"不重复造轮子"分两层落地：

| 层 | 采用 | 理由 |
|---|---|---|
| 依赖（直接引入） | **fastembed-rs**（bge-m3 向量 + bge-reranker-v2-m3 重排，可选 Qwen3-Embedding-0.6B） | 进程内、Apache-2.0、活跃，替掉 Python memU |
| 依赖 | **jieba-rs** 中文分词 | 修中文关键词检索的根因 |
| 依赖 | **SQLite**（现有 rusqlite）：FTS5 存分词后的文本 + 向量 BLOB + Rust 余弦 + RRF 融合 | 不引第二个存储；万级到十万级条目时暴力检索约毫秒级（估算）；需要时再加 sqlite-vec/usearch |
| 设计（照搬思路） | Hindsight：多路召回（语义 / BM25 / 实体 / 时间）→ RRF → 交叉编码重排 | 召回不需要 LLM，直接修基线问题 |
| 设计 | mem0：对话 → 自包含短事实（带实体、时间、出处） | 2026 版本改为只追加，冲突留到读时和整理时处理 |
| 设计 | Graphiti/nacre：双时间列 `valid_at / invalid_at`，新事实使旧事实失效而不是删除 | 冲突消解是各基准最弱的一项 |
| 设计 | Letta 睡眠计算 / Anthropic "dreaming"：空闲时后台整理 | Anthropic 实测：少而精的笔记胜过多而杂 |
| 设计 | ACE：反思 → 策展 → 增量条目确定性合并，带 helpful/harmful 计数 | 防止整段重写导致的"上下文坍塌" |
| 设计 | Hermes：注入层小且有上限、会话开始时冻结快照；技能只从**验证过的成功**里产生，复用时打补丁 | SkillsBench：自写技能 −1.3pp，精选技能 +16.2pp |

不采用：mem0/Graphiti/Letta/MemOS/Hindsight 本体（Python + 服务）、Cortex-Mem（需要 Qdrant）、
memvid（只支持英文）、rig/swiftide（没有记忆层）、各个 Rust mem0 移植（太不成熟）。

## 4. 目标架构

```
             ┌──────────── MemoryCore（Rust，uclaw.db，挂在 MemoryAdapter 后） ────────────┐
 T0 原文     │ agent_messages / agent_turns（已存在，唯一的对话真相源）                     │
 T1 记忆条目 │ memory_items(kind: fact|preference|episode|lesson, content, entities,       │
             │   valid_at, invalid_at, source_turn_ids, confidence, helpful, harmful,      │
             │   status: candidate|active|superseded|rejected, version) + 分词FTS + 向量    │
 T2 技能/手册│ SKILL.md（渐进披露）+ ACE 手册条目（memory_items kind=lesson）              │
 T3 注入层   │ USER 卡（≤500 token）+ 手册索引（≤800 token）+ 技能名单，会话开始时冻结     │
             └─────────────────────────────────────────────────────────────────────────────┘
 写：  每轮：只存原文 + 低成本抓取 ─► 任务后：抽取事实/教训（候选）─► 睡眠整理：去重、合并、
       对照原文校正、升降级、失效旧事实、重建 T3、修技能
 读：  会话开始注入 T3 快照（缓存友好）；每轮注入 top-k 相关条目（严格按字符预算）；
       工具 memory_search / memory_write(add|patch|remove) / skill_* 供 agent 按需深挖
 反馈：条目被用于成功轮次 → helpful+1；用户纠正 → harmful+1；分数低 → 降级/归档
```

迁移去向：

| 现有 | 去向 |
|---|---|
| memory_graph 节点、facets、user_model、偏好、personality、memU 条目、gbrain 页面 | 一次性迁移进 `memory_items`（保留出处），之后只读或删除 |
| bucket_seal | 保留分块/封存树作为 T0 的长文索引；摘要器改用真实 LLM（可用本地 MiniCPM） |
| reflections / daydream / consolidation / importance_decay | 合并为一个睡眠整理任务 |
| GEP genes | 冻结；有价值的内容转成手册条目 |
| memU（Python）、gbrain（Bun） | 移除；gbrain 留一次性导入工具 |
| 死代码、写错库的 ConversationBridge | 删除 |

安全与控制：
- 每条记忆必须带出处；来自工具输出（网页、文件、邮件）的内容，用户批准前不能升为 active；
- 写入时扫描注入、外泄和不可见字符；
- 新增"我学到了什么"面板，支持批准、拒绝、编辑、遗忘；遗忘会同时删除全文索引和向量。

## 5. 分阶段计划（每阶段用 M0 评测把关）

| 阶段 | 内容 | 验收 |
|---|---|---|
| **P0 快修**（小 PR，可先做） | `compose` 改为按字符计；召回块顺序改为"相关 > 触发 > 启动"，技能只列名字；bucket_seal 补充块不再被截掉 | pi 提示词里相关记忆 0/37 → 明显 >0，且全部由召回块贡献 |
| **P1 检索** | jieba 分词 FTS + fastembed bge-m3 + bge-reranker-v2-m3；提供统一的 `MemoryCore::search`，`load_context` 只调它；模型按需下载（ModelScope/镜像） | 召回 hit@5 ≥ 70%（中文 ≥ 65%）；p50 < 150ms |
| **P2 统一存储** | `memory_items` 表（双时间 + 出处 + 计数）+ 迁移脚本；统一写入口 `observe()` | 7 份画像 → 1 份；旧库改为只读 |
| **P3 学习闭环** | 任务后抽取 + 睡眠整理（本地 MiniCPM 或便宜模型）+ ACE 手册 + 技能只从验证过的成功中产生；删掉旧的各条学习回路 | 新增冲突/投毒/膨胀三类评测；注入层不超上限 |
| **P4 接入 pi** | T3 冻结快照放进系统提示；pi 增加 `memory_search` / `memory_write` 工具；不再把 12k 拼在用户消息前；legacy 路径跟进 | 提示词里相关记忆 / 已存 ≥ 80% |
| **P5 控制与主动** | 学习面板、写入审批；主动建议以"收件箱"形式提供（默认关），按采纳率衡量 | — |

移除 memU 和 gbrain 放在 P1/P2 完成之后，不提前删。

## 6. 需要你拍板的点

1. **gbrain**：是否整体退役？你如果在 gbrain 里手动浏览或编辑过页面，需要先导入。
2. ~~本地模型体积~~ **已定（2026-11-09）**：首次使用下载约 1.2GB（bge-m3 + bge-reranker-v2-m3）可以接受。
3. **后台整理用哪个模型**：本地 MiniCPM（免费、慢、质量一般），还是云端便宜模型（每天有 token 预算）？
4. ~~P0 快修~~ **已完成**：结果见 `docs/eval/2026-11-09-memory-recall-baseline.md`「After P0」一节。

## 附录
- A. 全量盘点（22 个子系统，含文件行号）：会话笔记 `2026-11-09-memory-system-map.md`
- B. Rust 框架与模型调研：会话笔记 `2026-11-09-rust-memory-frameworks.md`
- C. 自主学习模式调研：会话笔记 `2026-11-09-agent-self-learning.md`
