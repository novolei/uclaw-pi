# skills.sh 技能市场接入 — 设计文档

**日期**: 2026-06-01
**状态**: 设计已批准(brainstorming Q&A),待用户 review → writing-plans
**作者**: brainstorming with Claude

---

## 1. 目标

让 uClaw 用户从 [skills.sh](https://skills.sh)(Agent Skills Directory)**发现 / 安装 / 管理**技能:

1. **聊天内**:agent 本地无匹配时搜 skills.sh,候选以优雅卡片呈现,用户**一键安装**(全局 / 本工作区)。
2. **万花筒技能页**:可浏览 / 搜索 / 安装 skills.sh,并管理已装技能(更新 / 卸载)。

后端**纯 Rust 直连 skills.sh HTTP API**;**legacy + pi 两条引擎**都支持。

## 2. 背景 / 现状

**skills.sh** — Agent Skills Directory("npm for agent skills")。HTTP API v1(`https://skills.sh/api/v1/`,Bearer `sk_live_` 认证,600 req/min):
- `GET /skills/search?q=&limit=` — 语义搜索 → `data:[{id, slug, name, source, installs, sourceType, installUrl, url}]`
- `GET /skills?view=trending|hot|all-time&page&per_page` — 排行/列表
- `GET /skills/curated` — 精选(按 owner 分组)
- `GET /skills/{source}/{slug}` — **detail,内联文件** `{id, source, slug, hash, files:[{path, contents}]}`
- `GET /skills/audit/{source}/{slug}` — 安全审计 `{audits:[{provider, status, riskLevel, summary}]}`

SKILL.md 格式(YAML frontmatter + body)与 uClaw **完全一致**。

**pi-web 蓝本**(参考,`agegr/pi-web/app/api/skills`):Next.js;search 调 skills.sh + 回退 `npx skills find`;install **shell `npx skills add <pkg> -y --agent pi`**(`-g`=global)。node web app 走 npx 很自然;**uClaw 是 Rust 桌面端,改用 HTTP API**(决策 Q1)。

**uClaw 现状**(已有大半套):
- `skill_marketplace` agent 工具(GitHub Code Search 搜 + 装到 `~/.uclaw-pi/skills/_marketplace/`)— 本设计**升级其搜索后端**为 skills.sh。
- staging→commit→reload 安装机制(`automation/marketplace/skill_install.rs`)— **复用**。
- `marketplace_standalone_installs` 表(V25)— **复用**(记录独立安装)。
- 万花筒 → 技能 管理页(`ui/src/features/`,具体组件写计划时映射)。
- 技能为**全局单 `SkillsRegistry`**;workspace 仅靠 tag 过滤(V19 `spaces.skill_tags` × `skill_matches_workspace`),**无 per-workspace 技能目录**。

## 3. 决策摘要(Q&A)

| # | 决策点 | 选择 |
|---|---|---|
| Q1 | 接入方式 | **HTTP API(纯 Rust)** — 无 node/npx 依赖,detail 内联文件直接写盘 |
| Q2 | global × workspace | **全局文件 + workspace tag**(复用 V19,无新 scan dir) |
| Q3 | 发现+安装模型 | **远程搜索独立工具 + 用户点安装卡片**(安装非 agent 静默) |
| Q4 | 落地引擎 | **legacy + pi 两条同时** |
| ① | workspace 激活判据 | **tag 为准;软链接纯可见性** |

## 4. 架构

### 4.1 存储 & global/workspace 模型

```
~/.uclaw-pi/skills/<slug>/SKILL.md + 附件        ← 真实文件,唯一真相源(User tier)

<workspace>/.uclaw/skills/<slug> ─→ ~/.uclaw-pi/skills/<slug>/   ← 软链接(可见性,不参与激活)
```

- 安装(任意 scope)→ 真实文件**永远落全局** `~/.uclaw-pi/skills/<slug>/`(零重复)。
- **workspace 安装** = ① 用当前 workspace 的 tag 标记技能(V19 激活)+ ② 建软链接 `<workspace>/.uclaw/skills/<slug>`(文件树可见;agent 可经 cwd 直接 `read .uclaw/skills/<slug>/SKILL.md`)。**若该 workspace 尚无 tag,安装时自动为其创建一个 workspace tag(如 workspace slug),写入 `spaces.skill_tags` 并打到该技能上。**
- **global 安装** = 不打 tag(到处可用),不建软链接。
- **卸载**:workspace = 删 tag + 删软链接(全局文件保留);global = 删真实目录 + 所有软链接。
- **激活 100% 由 tag 决定**;`SkillsRegistry` 仍跳过软链接扫描(不引入软链接扫描攻击面)。

### 4.2 组件

1. **skills.sh 客户端**(`src-tauri/src/skills_marketplace/` 新模块):reqwest 调 `/api/v1`。`search(q,limit)` / `list(view,page)` / `curated()` / `detail(id)->files` / `audit(id)`。API key 从配置读;无 key 优雅降级。
2. **安装服务**:`install(id, scope)`(detail → 校验路径穿越 + 审计高危拦截 → staging → commit 到全局 → workspace 则 tag+软链接 → `registry.reload()` → 记 V25);`uninstall(slug, scope)`;`check_update(slug)->bool`(对比 V25 存的 hash vs detail.hash)。复用现有 staging→commit。
3. **Tauri 命令**(扩展 `commands/skills.rs` + 注册 main.rs handler):`search_skill_marketplace` / `list_skill_marketplace` / `get_skill_marketplace_detail` / `install_skill_from_marketplace(id,scope)` / `uninstall_skill(slug,scope)` / `check_skill_update(slug)`。
4. **Agent 工具**:`skill_search_marketplace(query,limit)`(新,调客户端 search,返回候选结构供卡片渲染)。`skill_search` / `load_skill` 不变。废弃旧 agent 工具 `skill_install_from_marketplace`(安装改卡片命令,符合 Q3)。
5. **前端**:
   - **安装卡片**:`skill_search_marketplace` 工具结果渲染器(放 `ui/src/shared/tool-rendering/`,聊天+agent 共用)。top-N 列表 + `[安装 ▾]`(全局/本工作区)+ 审计徽章;装好原地变「已安装(scope)」。
   - **万花筒技能页**:加「市场」tab(热门/精选/搜索 + detail 抽屉 SKILL.md 预览 + 审计)+「已安装」区(provenance 徽章**补 `marketplace`** + 检查更新 + 卸载;保留启用/禁用/详情)。
   - **skills bridge**(`ui/src/lib/bridge/skills.ts`):加上述命令。

### 4.3 数据流(聊天内)

```
本地 skill_search 无匹配
 → agent 调 skill_search_marketplace(q)  →(legacy: dispatcher / pi: IO 桥)→ skills.sh /search → 候选[]
 → 前端渲染【安装卡片】(top-N + 审计徽章 + [安装 ▾])
 → 用户点 [本工作区]
 → install_skill_from_marketplace(id, "workspace")
     → detail+files → 校验 → 写 ~/.uclaw-pi/skills/<slug>/ → 打 tag + 建软链接 → reload → 记 V25
 → 技能立即对 skill_search / load_skill 可见(本工作区)
```

## 5. 错误处理 & 安全

- **API 失败**(超时/5xx):卡片显示「skills.sh 暂不可用」可重试;不阻塞会话;本地 skill_search 不受影响。
- **无 API key**:marketplace 搜索返回提示「请在设置填 skills.sh API key」。
- **安装失败**(staging/写盘):回滚 staging,报错,不动已装技能。
- **审计**:徽章来自 audit 端点,**对展示的候选(top-N≤5)惰性并行拉取**(不阻塞 search;search 端点本身不返回审计),或在 detail/install 时拉;`⚠未审计` / `✓ 低` / `⚠ 高`;**HIGH 风险安装前二次确认**(卡片/抽屉弹警告)。
- **路径穿越**:slug 与文件 `path` 校验,拒绝 `/`、`\`、`..`(沿用现有 `_marketplace` 安装校验)。
- **软链接**:仅 workspace 安装时建,指向全局只读真相源;registry 不跟随软链接。

## 6. 两引擎落地

- **后端 / 命令 / UI**:引擎无关,一次实现两边共用。
- **agent 工具 `skill_search_marketplace`**:
  - **legacy**:注册进 `ToolRegistry` / dispatcher(普通 `Tool`)。
  - **pi**:`RealToolRequestSink`(R4 IO 桥)把 `skill_search` / `load_skill` / `skill_search_marketplace` 一起声明进 `io_tool_specs` + dispatch。→ **本功能顺带交付 R4 Slice 1 的 skills IO 桥**(纯按需:工具即 awareness,不注入 manifest)。

## 7. 测试策略

- **客户端**(mockito mock skills.sh HTTP):search/detail/audit 解析 + 无 key 降级。
- **安装服务**(临时目录):global / workspace(tag+软链接)安装、回滚、卸载、check_update;路径穿越拒绝。
- **工具**:`skill_search_marketplace` 返回结构(mock 客户端单测)。
- **前端**:安装卡片渲染(`renderWithProviders` + mock bridge);万花筒 市场/已安装 tab。
- ⚠️ **UI 验证检查点**(无法自动驱动原生窗口):安装卡片 + 万花筒页 + pi 路径,需人工 spot-check。

## 8. 分阶段实现(供 writing-plans 切片;每阶段独立可合 PR)

| 阶段 | 内容 | 验证 |
|---|---|---|
| **P1** | 后端 skills.sh 客户端 + 安装服务(install/uninstall/update)+ 命令 + V25 记录 | cargo 单测 |
| **P2** | legacy agent 工具 `skill_search_marketplace` 注册进 dispatcher | cargo |
| **P3** | 聊天内安装卡片(工具结果渲染器 + bridge + install 命令接线) | UI |
| **P4** | 万花筒技能页(市场 tab + 已安装区改造 + `marketplace` 徽章) | UI |
| **P5** | pi IO 桥 `RealToolRequestSink`(skill_search/load_skill/skill_search_marketplace)+ main.rs 接线 | pi UI |

## 9. 待办 / 依赖

- **skills.sh API key**:确认有无免 key 公开 tier;否则设置页加「skills.sh API key」配置项(加密存,沿用现有 secrets 机制)。
- **万花筒现有技能页**:写计划时精确映射其组件 / 布局 / IPC。
- **workspace 文件树 `.uclaw`**:确认文件 tab 是否渲染隐藏目录 + 软链接显示是否正常。
- **现有 `skill_marketplace` 工具**:精确定位其 search/install 代码,P2 替换搜索后端 + 废弃旧安装工具。
- **workspace tag 附着机制**:确定「给已装技能打 workspace tag」是改其 SKILL.md frontmatter `activation.tags`,还是用 DB sidecar 映射(避免改下载的文件)—— 写计划时定。

## 10. 范围外(YAGNI)

- 技能**发布/上传**到 skills.sh(只消费,不生产)。
- 版本锁定 / 依赖解析(skills.sh 不强制)。
- per-workspace **真实**技能目录(Q2 选 tag 方案;软链接只做可见性)。
