# skillsmp.com — Second Marketplace Provider (design)

**日期**: 2026-06-01
**状态**: 设计已批准(AskUserQuestion);待执行
**关联**: 扩展 `2026-06-01-skills-sh-marketplace-design.md`(skills.sh 为第一个 provider)

---

## 1. 目标

把 [skillsmp.com](https://skillsmp.com) 作为与 skills.sh **同等地位的第二个技能市场**:用户可在聊天内安装卡片 + 万花筒市场页通过 **provider 选择器**选择 skills.sh 或 skillsmp.com 来搜索 / 安装,并为 skillsmp.com 提供一个 API key 设置卡片(可选 key,仅影响限流)。

**动机**:skills.sh 的 `/api/v1` 强制需要 Bearer key(只能邮件申请),用户尚未拿到;skillsmp.com 搜索**匿名可用**(50 req/day),立刻解锁 dogfooding。

## 2. skillsmp.com API(实测 2026-06-01)

Base `https://skillsmp.com/api/v1`。**仅有 search**(404 返回的 `availableEndpoints` 确认:`/api/v1/skills/search`, `/api/v1`, `/api/health`, `/api/llms.txt`, `/api/timeline` —— 无 detail / audit / list)。

`GET /api/v1/skills/search?q=&page=&limit=&sortBy=stars|recent&category=&occupation=`
- 认证:**可选** `Authorization: Bearer sk_live_…`。匿名 50/day · 10/min;认证 500/day · 30/min。限流头 `X-RateLimit-Daily-*`。
- 响应:
```json
{ "success": true, "data": {
  "skills": [ { "id": "tuyv-…-pdf-skill-md", "name": "pdf", "author": "TuYv",
    "description": "...", "githubUrl": "https://github.com/TuYv/ccpm/tree/main/preset-registry/skills/anthropics-skills-pdf",
    "skillUrl": "https://skillsmp.com/skills/…", "stars": 0, "updatedAt": "1779575012" } ],
  "pagination": { "page":1, "limit":2, "total":1000, "totalPages":2, "hasNext":true, "hasPrev":false },
  "filters": { "search":"pdf", "sortBy":"recent" } },
  "meta": { "requestId":"…", "responseTimeMs":8 } }
```
- 错误:`{ "success": false, "error": { "code":"INVALID_API_KEY", "message":"…" } }`。

**关键**:skillsmp 不返回内联文件;每条结果带 **`githubUrl`**(技能目录的 GitHub 地址)。安装 / 预览经 **GitHub** 完成 —— 正是现有 `SkillInstallFromMarketplaceTool`(`owner/repo/<path>` → GitHub contents API)已实现的机制。

## 3. 决策(AskUserQuestion)

| # | 决策点 | 选择 |
|---|---|---|
| Q1 | 两市场 UI 共存 | **provider 选择器,默认 skillsmp.com**(免 key,立刻可用;有 skills.sh key 时可切) |
| Q2 | 范围 | **完整对等**:搜索 + 安装 + 详情预览 + apikey 卡片(用户已有 skillsmp key) |

## 4. 架构 —— 在 `SkillDetail` 层统一

核心:每个 provider 产出**同一组规范化类型**,安装路径在 detail 之后**与 provider 无关**(复用 `write_skill_files` + tag/symlink/V25)。

```
enum MarketplaceProvider { SkillsSh, Skillsmp }   // serde snake_case: "skills_sh" | "skillsmp"

search(q, limit, provider, key)  → Vec<SkillSummary>     // 各家归一化
provider_detail(id, provider, source, key) → SkillDetail // skills.sh: client.detail; skillsmp: fetch_github_skill(githubUrl)
audit(id, provider, key)         → SkillAudit            // skills.sh: client.audit; skillsmp: 空(未审计)
install(id, provider, source, scope, ws) → provider_detail → write_skill_files → tag/symlink/V25  // provider 无关
```

- **`SkillSummary` 加 `#[serde(default)] description: String`**(skillsmp 填,skills.sh 留空)。`install_url` 对 skillsmp = `githubUrl`(安装/预览的 source)。
- **skillsmp client**(`skills_marketplace/skillsmp.rs`):`search` → 解析 `data.skills[]` → `SkillSummary { id, slug: 从 id 派生, name, source: author, installs: stars, source_type:"github", install_url: githubUrl, url: skillUrl, description }`。
- **`fetch_github_skill(github_url) → SkillDetail`**(`skills_marketplace/github.rs`):解析 `github.com/{owner}/{repo}/tree/{branch}/{path}` → GitHub contents API 列目录 → 拉每个文件 → `SkillDetail { files }`。从现有 `SkillInstallFromMarketplaceTool::execute` 抽取核心(文件数/大小上限、路径穿越防护原样保留)。
- **`provider` 参数**穿过 4 个命令 + 聊天工具 `skill_marketplace_search`(默认 skillsmp)。命令默认 provider = `skillsmp`(免 key)。

## 5. 后端改动

1. `mod.rs`:`MarketplaceProvider` 枚举(serde);`SkillSummary.description`。
2. `skillsmp.rs`:client(search,可选 key)+ 归一化 + mockito 测试。
3. `github.rs`:`fetch_github_skill` + githubUrl 解析(抽取现有 GitHub 逻辑;旧工具改调它,零行为变化)+ 单测(解析 + 防穿越)。
4. `commands/skills_marketplace.rs`:`provider` 参数 + `provider_detail` helper;`search`/`get_detail`/`get_audit`/`install`/`check_update` 按 provider 分支。
5. `agent/tools/builtin/skill_marketplace.rs`:`skill_marketplace_search` 加 `provider`(默认 skillsmp)+ 结果带 `provider` + `installUrl`(供卡片安装)。
6. settings:`skillsmp_api_key`(get-status / set)命令 + service(镜像 #23 的 `skills_sh_api_key`)。
7. main.rs:注册新命令。

## 6. 前端改动

1. 类型:`MarketplaceProvider = 'skills_sh' | 'skillsmp'`;`MarketplaceSkillSummary` 加 `description?`。
2. bridge:`provider` 参数加到 search/list/detail/audit/install/check-update;skillsmp apikey 的 get-status/set。
3. **聊天安装卡片**(`skill-marketplace-search-result.tsx`):从工具结果读 `provider` + 行的 `installUrl`,安装时传 `provider` + `source`。
4. **万花筒市场页**:**provider 选择器**(skills.sh / skillsmp.com,默认 skillsmp);搜索/详情/安装走选中的 provider。skillsmp 无 audit → 徽章「未审计」;无 trending → 空 query 时提示「输入关键词搜索」(skillsmp 无 list)。
5. **设置**:`SkillsmpApiKeyCard`(镜像 `SkillsApiKeyCard`,key 可选 → 文案「可选,提高限流」)接进 SystemTab。

## 7. 错误处理 & 边界

- skillsmp 匿名超限(429)→ 提示「skillsmp.com 已达匿名限流,填 API key 提高额度」。
- skillsmp 无 list/trending → 市场页 skillsmp 选中且空 query 时,显示提示而非拉取。
- skillsmp 无 audit → 详情徽章「未审计」(不阻塞安装;无 HIGH 二次确认,因为没有风险信号)。
- GitHub 安装:沿用现有文件数(≤32)/大小(≤512KB)/路径穿越防护。
- 私有/失效 githubUrl → 安装报错,不动已装技能。

## 8. 分阶段(每段独立可合 PR)

| 段 | 内容 | 验证 |
|---|---|---|
| **SM1 后端** | provider 枚举 + skillsmp client + github fetch + provider 参数(命令 + 工具)+ skillsmp apikey 命令 | cargo + mockito |
| **SM2 前端** | provider 参数(bridge/types)+ 聊天卡片 provider + 市场页 provider 选择器 + skillsmp apikey 卡片 | tsc + 人工 UI |

## 9. 范围外(YAGNI)

- skillsmp 的 list/trending(API 无)、audit(API 无)。
- 合并两家搜索结果(选了 provider 选择器)。
- 卸载(沿用 skills.sh 那条待办)。
- 发布技能到 skillsmp(只消费)。
