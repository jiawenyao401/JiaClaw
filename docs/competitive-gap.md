# JiaClaw 竞争差距分析

> 对比分析 JiaClaw 相对于 OpenClaw 和 Hermes Agent 的能力差距、优势和路线图

**更新时间**: 2026-09-21  
**状态**: 开发中 (Pre-Alpha)

---

## 执行摘要

JiaClaw 是基于 [StateKnot](https://github.com/StateKnot/StateKnot) 的持久化智能体运行时，聚焦于**生产就绪的持久化保证**和**类型安全的 Agent 执行**。相比 OpenClaw 和 Hermes Agent，JiaClaw 的核心差异化在于：

- ✅ **持久化优先架构** - 基于 StateKnot 的确定性图执行、检查点和恢复
- ✅ **类型化 Agent 合约** - TypedAgent<I,O> 提供编译时安全和 JSON Schema 验证
- ✅ **生产治理** - 租户隔离、资源策略、预算控制、审计日志
- ✅ **协议原生** - MCP 和 A2A 一等公民支持（待实现）
- ✅ **Gateway 层抽象** - 通过 [Brokerrouter](https://github.com/StateKnot/Brokerrouter) 统一模型访问（规划中）

当前 JiaClaw 处于**早期开发阶段**，许多功能待 StateKnot 和 Brokerrouter 稳定后实现。本文档对比三个系统的能力矩阵，标注优先级和实现计划。

---

## 对比系统简介

### OpenClaw
- **仓库**: https://github.com/openclaw/openclaw
- **文档**: https://docs.openclaw.ai
- **定位**: Personal AI agent framework（个人 AI 智能体框架）
- **技术栈**: Python，支持多种 LLM 提供商
- **特点**: 成熟的技能系统、丰富的工具集成、强调易用性

### Hermes Agent
- **仓库**: https://github.com/NousResearch/hermes-agent
- **组织**: NousResearch
- **定位**: Agentic framework for research and task execution
- **技术栈**: Python，基于 Hermes 模型系列
- **特点**: 强调推理能力、函数调用、研究任务支持

### JiaClaw
- **仓库**: https://github.com/jiawenyao401/JiaClaw
- **定位**: Production-ready durable agent runtime（生产就绪的持久化智能体运行时）
- **技术栈**: Rust，基于 StateKnot 框架
- **特点**: 持久化执行、类型安全、租户隔离、协议原生

---

## 能力对比矩阵

### 1. Channels & Gateway（通道与网关）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **CLI 界面** | ✅ 完整 | ✅ 完整 | ✅ **完整（REPL+参数）** | P1 | - |
| **HTTP REST API** | ✅ FastAPI | ✅ Flask/FastAPI | ✅ **基础（axum + 可选限流 + 优雅退出）** | P1 | - |
| **Session（内存）** | ✅ 支持 | ⏳ 部分 | ✅ **已实现** | P1 | - |
| **Session（落盘）** | ✅ 支持 | ⏳ 部分 | ✅ **可选落盘** | P1 | - |
| **Session 查询 API** | ✅ 支持 | ⏳ 部分 | ✅ **GET /api/sessions** | P1 | - |
| **Session 导出** | ✅ 支持 | ⏳ 部分 | ✅ **GET export + CLI JSONL** | P1 | - |
| **Session 导入** | ✅ 支持 | ⏳ 部分 | ✅ **POST import + CLI JSONL** | P1 | - |
| **Session TTL** | ✅ 支持 | ⏳ 部分 | ✅ **可选闲置过期** | P1 | - |
| **Session 摘要压缩** | ✅ 支持 | ⏳ 部分 | ✅ **可选溢出摘要** | P1 | - |
| **工具列表 API** | ✅ 支持 | ⏳ 部分 | ✅ **GET /api/tools** | P1 | - |
| **技能列表 API** | ✅ 支持 | ❌ 无 | ✅ **GET /api/skills** | P1 | - |
| **技能热加载** | ⏳ 常需重启 | ⏳ 常需重启 | ✅ **POST /api/skills/reload + Unix SIGHUP** | P1 | - |
| **Webhook 入站** | ✅ 支持 | ⏳ 部分 | ✅ **POST /hooks/inbound** | P1 | - |
| **Telegram Bot 入站** | ✅ 支持 | ⏳ 部分 | ✅ **POST /hooks/telegram** | P1 | - |
| **Telegram Bot 出站** | ✅ 支持 | ⏳ 部分 | ✅ **可选 sendMessage** | P1 | - |
| **Slack Events 入站** | ✅ 插件支持 | ❌ 无 | ✅ **POST /hooks/slack** | P1 | - |
| **Slack Bot 出站** | ✅ 插件支持 | ❌ 无 | ✅ **可选 chat.postMessage** | P1 | - |
| **Discord Interactions 入站** | ✅ 插件支持 | ❌ 无 | ✅ **POST /hooks/discord** | P1 | - |
| **Discord Bot 出站** | ✅ 插件支持 | ❌ 无 | ✅ **可选 deferred PATCH** | P1 | - |
| **Request ID** | ✅ 支持 | ⏳ 部分 | ✅ **X-Request-Id** | P1 | - |
| **OpenAPI** | ✅ FastAPI 自动 | ⏳ 部分 | ✅ **GET /api/openapi.json** | P1 | - |
| **Prometheus 指标** | ⏳ 部分 | ⏳ 部分 | ✅ **GET /metrics** | P1 | - |
| **SSE 事件流** | ✅ 支持 | ⏳ 部分 | ✅ **可选 SSE（完成后分块）** | P1 | [#92](https://github.com/StateKnot/StateKnot/issues/92) AgentServiceV1 |
| **WebSocket** | ⏳ 社区贡献 | ❌ 无 | ⏳ 计划中（M4） | P2 | - |
| **gRPC** | ❌ 无 | ❌ 无 | ⏳ 可选（通过 StateKnot） | P2 | - |

**JiaClaw 现状**:
- ✅ CLI 已实现（`jiaclaw chat`, `jiaclaw serve`, `jiaclaw session export/import`）
  - ✅ 单次聊天模式：`jiaclaw chat "消息"`
  - ✅ REPL 交互模式：`jiaclaw chat`（支持多轮对话）
  - ✅ 技能开关：`--skill <name>` 可重复使用，`--no-auto-skill` 禁用自动激活
  - ✅ 会话管理：`--session <id>` 续聊支持
  - ✅ 会话导出：`jiaclaw session export <id> [-o file]`，默认 stdout JSONL；读落盘 store，不触发摘要
  - ✅ 会话导入：`jiaclaw session import <file> [--id ID] [--overwrite]`，JSONL 或 `{id?, messages}` JSON；已存在需 `--overwrite`；不调用 LLM
- ✅ HTTP 服务已实现（GET /health, GET /metrics, POST /api/chat，可选 SSE, GET/POST /api/sessions, GET/DELETE /api/sessions/:id, GET /api/sessions/:id/export, POST /api/sessions/import, GET /api/tools, GET /api/skills, POST /api/skills/reload, GET /api/openapi.json）
- ✅ **可选 HTTP 限流**（`[http] rate_limit_per_minute` / `JIACLAW_RATE_LIMIT_PER_MINUTE`，进程内全局，超限 429 + Retry-After；GET /health 与 GET /metrics 不限流；覆盖 `/api/*` 与 `/hooks/inbound`、`/hooks/telegram`、`/hooks/slack`、`/hooks/discord`）
- ✅ **可选 Prometheus 指标**（`GET /metrics`，手写 Prometheus 0.0.4 文本；默认公开；`[http] metrics_public = false` / `JIACLAW_METRICS_REQUIRE_AUTH=1` 时与 `/api/*` 相同鉴权。HTTP 路由族计数、sessions_active、tool_calls、build_info；无 telemetry SDK）
- ✅ **请求追踪**（缺失则生成 UUID，响应回写 `X-Request-Id`；chat/webhook tracing 带 request_id）
- ✅ **OpenAPI 草图**（`GET /api/openapi.json`，手写 OpenAPI 3；鉴权与 `/api/tools` 一致）
- ✅ **可选 SSE**（`POST /api/chat`：`Accept: text/event-stream` 或 body `stream: true` → `text/event-stream`；事件 `meta` / `token` / `tool` / `done` / `error`。未请求流式时 JSON 不变；鉴权失败仍 JSON 401。**当前为整轮完成后分块推送；Brokerrouter 真流式后续**）
- ✅ Session 内存支持（可选 `session_id` 实现多轮对话历史；默认硬截断超长历史，可开启摘要压缩）
- ✅ **serve 优雅退出**（SIGINT/SIGTERM：停止 accept，宽限期等待进行中请求；`[http] shutdown_timeout_secs` 默认 15，`JIACLAW_SHUTDOWN_TIMEOUT_SECS` 优先；落盘开启时关闭路径原子刷盘；Heartbeat 任务 abort）
- ✅ **技能热加载**（`POST /api/skills/reload` 重扫工作区 `skills/` 并替换进程内注册表；失败保留旧表并返回明确错误；鉴权/限流/`X-Request-Id` 与其它 `/api/*` 一致。Unix `SIGHUP` 走同一路径；Windows 仅 HTTP。CLI `jiaclaw skills reload` 只扫描当前工作区，不通知已运行的 serve。进行中 chat 使用快照，不长时间持锁）
- ✅ **Session 查询 API**（`GET /api/sessions` 列出 `{id, message_count}`；`GET /api/sessions/:id` 返回消息，不存在 404；读内存当前状态，落盘开启时与 store 一致）
- ✅ **Session 导出**（`GET /api/sessions/:id/export` 默认 JSONL，`?format=json` 整包 `{id, messages}`；鉴权/限流/`X-Request-Id` 与其它 `/api/*` 一致；过期 TTL 与不存在同为 404；不触发摘要、不改写 store。CLI：`jiaclaw session export <id> [-o file]`）
- ✅ **Session 导入**（`POST /api/sessions/import` 默认 JSONL，`Content-Type: application/json` 或 `?format=json` 接受 `{id?, messages}`；可选 `?id=`；已存在默认 409，`?overwrite=true` 替换并刷新 last_accessed；非法行 400；不调用 LLM；超长走 MAX_SESSION_MESSAGES 硬截断/本地摘要。CLI：`jiaclaw session import <file> [--id ID] [--overwrite]`）
- ✅ **可选 Session TTL**（`[http] session_ttl_secs` / `JIACLAW_SESSION_TTL_SECS`，正整数启用闲置过期；create/chat/get/list 刷新；过期后 list 省略，GET/DELETE/export 与不存在一致）
- ✅ **可选 Session 摘要压缩**（`[session] summarize_on_overflow` / `JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1/true`，默认关保持硬截断；开启后把旧消息折叠为一条 `[session-summary]` system 消息并保留最近 `keep_recent`；失败 warn 回退截断。HTTP/Telegram/Slack/Discord/webhook/heartbeat 共用 store 写入点，摘要无工具循环）
- ✅ **Session 可选落盘**（`[http] persist = true`，进程重启后可恢复历史，原子写入，自动处理损坏文件）
- ✅ 工具列表 API（GET /api/tools 列出已注册工具名称和描述）
- ✅ 技能列表 API（GET /api/skills 列出进程内已加载技能）
- ✅ 技能热加载 API（POST /api/skills/reload；失败保留旧表）
- ✅ Webhook 入站 API（POST /hooks/inbound，支持可选鉴权，自动 session 管理）
- ✅ **Telegram Bot 入站**（POST /hooks/telegram，手写 serde 解析 Bot API Update 最小子集；session `telegram:{chat.id}`；可选 `JIACLAW_TELEGRAM_SECRET` / `[http] telegram_secret` 校验 `X-Telegram-Bot-Api-Secret-Token`；无文本 200 跳过；同步回传 `{ ok, reply }`）
- ✅ **Telegram Bot 可选出站**（`JIACLAW_TELEGRAM_BOT_TOKEN` / `[http] telegram_bot_token` 优先；成功回复后 POST `sendMessage`，文本按 4096 截断；出站失败 warn + 仍 200 原 JSON，避免 webhook 重试；未配置 token 时行为与仅入站一致。`setWebhook` 指向公网 `/hooks/telegram`）
- ✅ **Slack Events API 入站**（POST /hooks/slack，手写 serde 解析 Events API 最小子集；`url_verification` 回传 `{ challenge }`；仅 `message` 且 `subtype` 为空；session `slack:{team_id}:{channel}` 或 `slack:{channel}`；可选 `JIACLAW_SLACK_SIGNING_SECRET` / `[http] slack_signing_secret` 校验官方 v0 HMAC-SHA256（raw body，±5 分钟）；无文本/忽略事件 200 跳过；同步回传 `{ ok, reply, session_id }`）
- ✅ **Slack 可选出站**（`JIACLAW_SLACK_BOT_TOKEN` / `[http] slack_bot_token` 优先；成功回复后 POST `chat.postMessage`；出站失败 warn + 仍 200 原 JSON，避免 Events 重试；未配置 token 时仅同步 JSON）
- ✅ **Discord Interactions 入站**（POST /hooks/discord，手写 serde 解析 Interactions 最小子集；`type=1` PING → `{ type: 1 }`；仅 Chat Input Command；session `discord:{guild_id}:{channel_id}` 或 `discord:dm:{channel_id}`；可选 `JIACLAW_DISCORD_PUBLIC_KEY` / `[http] discord_public_key` 校验官方 Ed25519（raw body）；立即 `type=5` deferred ACK，后台 chat）
- ✅ **Discord 可选出站**（`JIACLAW_DISCORD_BOT_TOKEN` / `[http] discord_bot_token` 优先；deferred 完成后 PATCH 编辑原始 Interaction；无 token 时仍记 session + warn。不支持 Incoming Webhook 简化体）
- ⏳ Brokerrouter / StateKnot **真 token 流式**仍待上游 SSE API（当前 HTTP SSE 为完成后分块）

**目标方案**:
- **P1 HTTP/SSE**: ✅ HTTP 可选 SSE 已落地（完成后分块）。真流式等待 Brokerrouter `stream:true` + StateKnot [#92](https://github.com/StateKnot/StateKnot/issues/92) `AgentHost` 事件订阅
- **P2 Discord**: ✅ Interactions HTTP 入站已落地（不依赖 MCP）。更丰富的组件/Gateway 仍可通过后续切片或 StateKnot MCP 扩展

---

### 2. Memory（记忆系统）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **短期记忆** | ✅ 会话历史 | ✅ 上下文窗口 | ✅ 基础（ChatRequest） | P0 | - |
| **工作区 MEMORY.md** | ✅ 文件注入 | ✅ MEMORY.md 注入 | ✅ **每次 chat 注入 + memory_write + memory_search** | P1 | - |
| **工作区 SOUL.md / USER.md** | ✅ 人格与用户画像 | ✅ SOUL/USER 注入 | ✅ **每次 chat 注入 + soul_write/user_write** | P1 | - |
| **工作区 HEARTBEAT.md** | ✅ 定时注入 | ⏳ 部分 | ✅ **serve 内可选定时 chat** | P2 | - |
| **长期记忆（向量）** | ✅ 向量数据库 (ChromaDB/Pinecone) | ✅ 嵌入检索 | ⏳ 计划中（M3） | P1 | StateKnot 持久化层 |
| **语义搜索** | ✅ 完整 | ✅ 完整 | ✅ **本地 memory_search（子串/行窗，无向量库）** | P1 | 向量检索仍待 M3 |
| **记忆编辑** | ✅ API 支持 | ⏳ 部分 | ✅ **本地 memory_write（append/overwrite）** | P2 | 远程 API 仍待 M3 |
| **时间旅行** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | StateKnot Checkpoint |
| **Fork/Branch** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | StateKnot Graph Fork |

**JiaClaw 现状**:
- ✅ 短期记忆（`ChatRequest.messages`）已实现
- ✅ Session 超长历史硬截断（默认）；可选摘要压缩（`[session] summarize_on_overflow`）把旧消息折叠为一条 system 摘要并保留最近消息
- ✅ **工作区 `MEMORY.md` 长期记忆注入**（对照 Hermes/OpenClaw 文件记忆）
  - 约定路径：`{workspace}/MEMORY.md`，可用 `[memory] path` 覆盖（默认 `MEMORY.md`；缺省配置不破坏现有 TOML/JSON）
  - `JiaClawAgent::chat` / `build_system_prompt` 在每次对话开始时重读；存在且非空则注入固定区块（`## Long-term Memory（长期记忆）`，内容原样）
  - 超过 32KiB 截断并 `warn`
  - 工具 `memory_append`：`content` 追加 Markdown，可选 `replace`（默认 false）；只能写约定路径，禁止穿越；追加为读-改-写 + 原子 rename，带换行分隔
  - 工具 `memory_write`：可选、默认注册；`content` 必填，`mode=append|overwrite`（默认 append）；只写配置的 MEMORY.md / `[memory] path`，忽略 `path` 参数
  - 结果文件超过 32KiB（与注入截断对齐）时明确报错且不落盘；tmp + rename 原子写；返回 `{path, mode, bytes_written}`，不调用 LLM
  - 工具 `memory_search`：只读关键词/子串检索（大小写不敏感 + 行窗）；默认扫描配置的 MEMORY / SOUL / USER；可选 `paths` 为工作区安全相对路径
  - `query` 必填，`max_results` 默认 5、钳制 1..=20；返回 `{path, line, excerpt}` JSON；单文件超过 512KiB 截断并 warn
  - 路径安全复用现有 resolve（禁 `..`、绝对路径、symlink 逃逸）；`[tools.memory_search]` / `[tools.memory_write] enabled` 默认 true；**不引入向量数据库或远程 sync**
  - `jiaclaw doctor` 报告 MEMORY 是否存在及大小；`jiaclaw memory show` 查看内容
- ✅ **工作区 `SOUL.md` / `USER.md` 人格与用户画像注入**（对照 OpenClaw/Hermes 身份文件）
  - 约定路径：`{workspace}/SOUL.md`、`USER.md`，可用 `[identity] soul_path` / `user_path` 覆盖（缺省配置兼容）
  - 每次 `chat` / `build_system_prompt` 重读；存在且非空则分别注入 `## Soul（人格）`、`## User（用户画像）`
  - 各文件独立 32KiB 截断并 `warn`；无文件不报错
  - 路径安全复用 MEMORY 的 resolve（禁 `..`、绝对路径、symlink 逃逸）
  - 工具 `soul_write` / `user_write`：`content` + 可选 `replace`（默认 true 覆盖）；只能写约定路径
  - `jiaclaw doctor` 报告 SOUL/USER 是否存在及大小；`jiaclaw soul show` / `jiaclaw user show`
  - 与 MEMORY 并存，互不覆盖
- ✅ **工作区 `HEARTBEAT.md` 定时心跳**（对照 OpenClaw HEARTBEAT.md）
  - 约定路径：`{workspace}/HEARTBEAT.md`，可用 `[heartbeat] path` 覆盖（默认 `HEARTBEAT.md`；缺省配置兼容）
  - `[heartbeat] enabled = false` 默认关；`interval_secs` 默认 3600；`JIACLAW_HEARTBEAT_INTERVAL_SECS`（正整数）可覆盖间隔
  - `session_id` 默认 `heartbeat`（固定会话）
  - 仅 `jiaclaw serve` 进程内 tokio 后台任务；读取全文作为 user 消息复用现有 session/chat 路径；日志只打摘要
  - 文件缺失或空：跳过本轮并 debug，不退出；路径安全复用 MEMORY resolve（禁穿越）
  - `doctor` / serve 启动摘要显示是否启用、间隔、文件是否存在（不打印全文）
  - **不**引入外部 cron；CLI `chat` 不跑心跳
- ⏳ 向量检索式长期记忆仍待 M3（本切片仅为子串/行窗，无嵌入）
- ✅ 时间旅行和 Fork 能力由 StateKnot Checkpoint 提供（待实现）

**目标方案**:
- **P1 时间旅行**: 利用 StateKnot 的 `Checkpoint` 机制，提供运行快照和回放
- **P1 长期记忆**: 
  1. 通过 StateKnot `PostgresStore` 持久化会话历史
  2. 添加向量数据库工具（通过 MCP 或 Rust 本地工具）
  3. 在 Agent 提示中注入相关记忆检索结果

**竞争优势**: 
- JiaClaw 的时间旅行和 Fork 能力是**结构化保证**，而非后加特性
- StateKnot 的确定性图执行保证可重放性，优于简单的日志记录

---

### 3. Skills（技能系统）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **技能定义** | ✅ Python 类 + 装饰器 | ✅ 函数 + 描述 | ✅ **SKILL.md + YAML frontmatter** | P1 | [#96](https://github.com/StateKnot/StateKnot/issues/96) 技能组合 |
| **技能发现** | ✅ 自动扫描 | ⏳ 手动注册 | ✅ **自动扫描 skills/**  | P1 | - |
| **技能热加载** | ⏳ 常需重启 | ⏳ 常需重启 | ✅ **HTTP + Unix SIGHUP** | P1 | - |
| **技能激活** | ✅ 动态加载 | ✅ 运行时选择 | ✅ **显式 + 自动触发** | P1 | - |
| **触发器** | ⏳ 部分支持 | ❌ 无 | ✅ **关键词匹配** | P1 | - |
| **技能依赖** | ⏳ 部分支持 | ❌ 无 | ⏳ 计划中（M3） | P2 | - |
| **技能市场** | ✅ 社区生态 | ❌ 无 | ⏳ 计划中（M3） | P2 | - |
| **技能隔离** | ❌ 无 | ❌ 无 | ✅ **租户策略** | P1 | StateKnot 租户隔离 |

**JiaClaw 现状**:
- ✅ 技能定义：`SKILL.md` 格式（YAML frontmatter + Markdown 内容）
- ✅ 技能发现：启动时自动扫描 `skills/**/SKILL.md`
- ✅ 技能热加载：`POST /api/skills/reload` / Unix `SIGHUP` 重扫并替换进程内表；失败保留旧表；`jiaclaw skills reload` 仅扫描当前工作区
- ✅ 技能激活：支持显式启用（`enabled_skills` 字段）和自动触发（`triggers` 关键词）
- ✅ CLI 命令：`jiaclaw skills` 列出已发现技能；`jiaclaw skills reload` 扫描并打印（不通知 serve）
- ✅ API 端点：`GET /api/skills` 返回进程内技能列表；`POST /api/skills/reload` 热加载
- ✅ 示例技能：`search` 和 `calculator`（带触发器）
- ⏳ 技能市场 / 依赖解析：不在本切片（仍为 M3）

**技能格式示例**:
```markdown
---
name: web_search
description: 搜索互联网信息
triggers:
  - search
  - 搜索
  - find
---

# Web Search Skill
...
```

**目标方案**:
- **P1 技能定义** ✅ 已实现
  - 每个技能一个目录: `skills/<skill-name>/SKILL.md`
  - YAML frontmatter: `name`, `description`, `triggers`
  - Markdown 内容: 详细说明、使用示例
- **P1 技能发现** ✅ 已实现
  - 运行时扫描 `skills/` 目录
  - 解析 YAML frontmatter 和 Markdown 内容
  - 热加载：`reload_skills` 在锁外扫描，成功后短写锁替换；坏文件使 reload 失败并保留旧表
- **P1 技能激活** ✅ 已实现
  - 显式启用：通过 `ChatRequest.enabled_skills` 指定
  - 自动触发：用户消息匹配 `triggers` 时自动启用（可配置）
- **P2 技能隔离**: 利用 StateKnot 的资源策略限制技能的工具访问范围

**竞争优势**:
- 技能格式简单（Markdown + YAML），易于编辑和版本控制
- 自动触发机制降低用户学习成本
- 技能隔离和权限管理由 StateKnot 租户策略提供，更安全
- 技能执行自动获得持久化和恢复能力

---

### 4. Tools（工具系统）

#### 4.1 Shell / Filesystem / Browser / Web

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **Shell 命令** | ✅ 完整 | ✅ 完整 | ✅ **已实现** | P1 | [#95](https://github.com/StateKnot/StateKnot/issues/95) 本地工具 |
| **文件读写** | ✅ 完整 | ✅ 完整 | ✅ **已实现 + 沙箱 read_file/list_dir/write_file** | P1 | [#95](https://github.com/StateKnot/StateKnot/issues/95) 本地工具 |
| **浏览器控制** | ✅ Selenium/Playwright | ⏳ 部分 | ⏳ 计划中（M3） | P2 | MCP Browser Tool |
| **网页抓取** | ✅ BeautifulSoup | ✅ 完整 | ✅ **可选 web_fetch（去标签文本）** | P1 | 轻量 HTML 剥离，无头浏览器不做 |
| **API 调用** | ✅ requests | ✅ httpx | ✅ **已实现（HTTP）** | P1 | Rust reqwest + MCP |
| **JSON 处理** | ✅ 完整 | ✅ 完整 | ✅ **已实现** | P1 | - |
| **日期时间** | ✅ 完整 | ✅ 完整 | ✅ **已实现** | P1 | - |
| **网页搜索** | ✅ 完整 | ✅ 完整 | ✅ **可选 web_search（Brave）** | P1 | 本地工具，无头浏览器不做 |
| **工具超时** | ✅ 配置化 | ⏳ 简单 | ✅ **可选每调用超时** | P1 | tool loop `tokio::time::timeout` |
| **工具循环上限** | ✅ 配置化 | ⏳ 简单 | ✅ **可配置 max_tool_iterations** | P1 | 默认 5，钳制 1–32 |
| **工具沙箱** | ⏳ Docker | ❌ 无 | ✅ **StateKnot 策略** | P1 | StateKnot 资源策略 |

**JiaClaw 现状**:
- ✅ 基础本地工具已实现（Shell、File、HTTP、JSON、DateTime）
- ✅ **可选 web_search**（默认注册；`query` 必填，`max_results` 默认 5、钳制 1..=10）
  - 有 `JIACLAW_BRAVE_API_KEY` 或 `[tools.web_search] brave_api_key` 时调用 Brave Search API（HTTP 超时 10s，并遵守 `tool_timeout_secs`）
  - 无 key 时工具返回友好错误（不访问网络）；`enabled = false` 不注册
  - `GET /api/tools` 与 system prompt 只列出已注册工具；`jiaclaw doctor` 提示是否配置 key，**不打印明文**
- ✅ **可选 web_fetch**（默认注册；`url` 必填且仅 http/https，`max_chars` 默认 8000、钳制 500..=50000）
  - GET 页面；HTML 去掉 script/style 与标签后返回纯文本（含最终 URL / 标题）；超长注明 `[truncated]`
  - 默认拒绝 localhost / 私网 / 链路本地；`[tools.web_fetch] allow_private = true` 可放开
  - 最多 5 次重定向，HTTP 总体超时约 15s，并遵守 `tool_timeout_secs`；User-Agent 标明 JiaClaw
  - `enabled = false` 不注册；`GET /api/tools` / system prompt / doctor 与 web_search 并列
- ✅ **可选 memory_search**（默认注册；`query` 必填，`max_results` 默认 5、钳制 1..=20）
  - 默认扫描配置的 MEMORY / SOUL / USER；可选 `paths` 为工作区安全相对路径
  - 大小写不敏感子串 + 行窗；返回 `{path, line, excerpt}`；单文件 512KiB 上限
  - 路径安全复用 resolve（禁穿越 / 绝对路径 / symlink 逃逸）；**无向量数据库**
  - `enabled = false` 不注册；`GET /api/tools` / system prompt / doctor 与其它可选工具并列
- ✅ **可选 memory_write**（默认注册；`content` 必填，`mode=append|overwrite` 默认 append）
  - 只写配置的 MEMORY.md / `[memory] path`；忽略 `path` 参数，禁止穿越
  - 结果文件超过 32KiB 明确报错且不落盘；tmp + rename 原子写；返回 `{path, mode, bytes_written}`，不调用 LLM
  - `enabled = false` 不注册；无危险 shell；不引入向量库或远程 sync
- ✅ **可选 read_file**（默认注册；`path` 必填，工作区相对路径）
  - 可选 `offset` / `limit` **按行**切片（1-indexed；`offset` 默认 1）
  - 文件超过 256KiB 明确报错；二进制（NUL / 非 UTF-8）拒绝读取
  - 路径安全复用 resolve（禁穿越 / 绝对路径 / symlink 逃逸）；只读真实文件
  - `enabled = false` 不注册；`GET /api/tools` / system prompt / doctor 与其它可选工具并列；无 shell、不调用 LLM
- ✅ **可选 list_dir**（默认注册；`path` 默认 `.`）
  - 可选 `max_entries`（默认 200，钳制 1..=1000）、`recursive`（默认 false，不跟随 symlink 目录）
  - 返回 `{path, recursive, truncated, entries:[{name, type, size?}]}`
  - 路径安全复用 resolve；`enabled = false` 不注册；无 shell；**不做 exec**
- ✅ **可选 write_file**（默认注册；`path` / `content` 必填，工作区相对路径）
  - 可选 `mode=overwrite|append`（默认 `overwrite`）；可创建中间目录
  - 结果文件超过 256KiB（与 read_file 对齐）明确报错且不落盘；tmp + rename 原子写
  - 路径安全复用 resolve（禁穿越 / 绝对路径 / symlink 逃逸）；只写工作区内常规文件
  - `enabled = false` 不注册；`GET /api/tools` / system prompt / doctor 与其它可选工具并列；无 shell、不调用 LLM；**不做 exec**
- ✅ **可选每工具调用超时**（`[agent] tool_timeout_secs` / `JIACLAW_TOOL_TIMEOUT_SECS`，正整数启用；`0`/非法=关闭）
  - 对每次 `Tool::execute` 用 `tokio::time::timeout` 包裹；超时写入 `Tool timed out after Ns` 到 tool result，不 panic，继续 loop
  - 未配置时行为不变（不限制）
  - `shell_exec` / `http_get` 等同步工作已在工具内部 `spawn_blocking`；超时后 loop 立即继续，后台阻塞任务可能仍会跑完
  - `jiaclaw doctor` 显示是否启用及秒数
- ✅ **可配置工具循环上限**（`[agent] max_tool_iterations` / `JIACLAW_MAX_TOOL_ITERATIONS`，默认 **5** 与历史硬编码一致）
  - 环境变量正整数优先；`0`/非法忽略。生效值钳制到 1–32
  - tool loop 使用 `effective_max_tool_iterations()`；达上限时写入清晰 tool/assistant 提示并结束本轮
  - `jiaclaw doctor` / serve 摘要显示生效值
- ✅ StateKnot 提供 `DurableInvocationExecutor` 支持持久化工具调用
- ⏳ 更多工具类型待添加（浏览器控制等）

**目标方案**:
- **P1 本地工具**: 
  1. 等待 StateKnot [#95](https://github.com/StateKnot/StateKnot/issues/95) 本地工具注册文档
  2. 实现 Rust trait-based 工具（Shell, File, HTTP）
- **P1 MCP 工具**: 使用 StateKnot `McpRemoteTool` 适配器连接 MCP 服务器
- **P2 浏览器控制**: 通过 MCP Browser Tool 或 Rust headless_chrome

**竞争优势**:
- 工具调用自动持久化，崩溃恢复时不重复执行已完成的调用
- 可选每调用工具超时防止 `shell_exec` / `http_get` 卡住整轮 tool loop；可配置 `max_tool_iterations` 防止工具风暴；沙箱仍由 StateKnot 策略引擎规划

---

#### 4.2 MCP 协议支持

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **MCP 客户端** | ⏳ 社区贡献 | ❌ 无 | ✅ **StateKnot 原生** | P0 | StateKnot MCP 适配器 |
| **工具发现** | ⏳ 手动 | ❌ 无 | ✅ **StateKnot 原生** | P1 | `McpRemoteTool` |
| **Prompts** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | MCP SEP-2640 |
| **Resources** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | MCP SEP-2640 |
| **MCP 服务器** | ❌ 无 | ❌ 无 | ⏳ 可选（M4） | P2 | - |

**JiaClaw 现状**:
- ✅ StateKnot 已实现 MCP 客户端和 `McpRemoteTool`
- ❌ JiaClaw 尚未配置 MCP 服务器连接
- ❌ 需要等待 StateKnot [#92](https://github.com/StateKnot/StateKnot/issues/92) 稳定发布

**目标方案**:
- **P1 MCP 集成**: 在 Agent 配置中添加 MCP 服务器列表，运行时连接和发现工具
- **P2 MCP 服务器**: 可选地将 JiaClaw Agent 暴露为 MCP 服务器（供其他 Agent 调用）

**竞争优势**:
- **MCP 一等公民**: StateKnot 原生支持 MCP，无需额外适配层
- 支持 MCP Prompts 和 Resources，不仅是工具调用

---

### 5. Providers & Models（提供商与模型）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **OpenAI** | ✅ 完整 | ✅ 完整 | ⏳ 计划中（本 PR） | P0 | StateKnot Model Adapter |
| **Anthropic** | ✅ 完整 | ⏳ 部分 | ⏳ 计划中（M1） | P1 | StateKnot Model Adapter |
| **本地模型** | ✅ Ollama/LM Studio | ✅ Hermes 系列 | ⏳ 计划中（M1） | P1 | OpenAI-compatible |
| **多模态** | ⏳ 部分 | ❌ 无 | ⏳ 计划中（M5） | P2 | StateKnot 扩展 |
| **流式输出** | ✅ 完整 | ✅ 完整 | ✅ **HTTP SSE 分块**（真流式待 Brokerrouter） | P1 | StateKnot SSE |
| **模型切换** | ✅ 运行时 | ✅ 配置化 | ⏳ 计划中（M1） | P1 | StateKnot 适配器注册 |

**JiaClaw 现状**:
- ❌ 当前为存根实现，返回硬编码响应
- ❌ 需要等待 StateKnot [#92](https://github.com/StateKnot/StateKnot/issues/92) 模型适配器 API

**目标方案**:
- **P0 Brokerrouter 集成**: **优先路径**
  - 等待 Brokerrouter 仓库可用后，作为主要 Gateway 层
  - 配置 Brokerrouter 端点和路由规则
  - 透传标准 OpenAI-compatible 请求
  - 参见 [Brokerrouter 差距文档](brokerrouter-gaps.md)
- **P1 临时直连**: 当前 PR 已实现
  - 在 JiaClaw 层添加 OpenAI-compatible HTTP 客户端（临时方案）
  - 支持 `base_url` + `api_key` 配置（兼容 Ollama/LM Studio）
  - 无 API key 时回退到存根
  - ⚠️ 将在 Brokerrouter 可用后废弃，仅用于早期开发
- **P1 StateKnot 集成**: 等待 [#92](https://github.com/StateKnot/StateKnot/issues/92) 后迁移到 StateKnot Model Adapter（与 Brokerrouter 协同）
- **P1 流式输出**: ✅ `POST /api/chat` 可选 SSE（完成后按句/按块推送）。Brokerrouter 真 SSE + StateKnot AgentServiceV1 仍待后续

**竞争优势**:
- 模型调用自动持久化，失败自动重试（at-least-once 语义）
- 预算控制（tokens/cost）由 StateKnot 强制执行

---

### 6. Durability & Persistence（持久化与恢复）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **会话持久化** | ✅ SQLite/PostgreSQL | ⏳ 简单 JSON | ✅ **StateKnot 原生** | P0 | PostgresStore |
| **崩溃恢复** | ⏳ 手动 | ❌ 无 | ✅ **StateKnot 原生** | P0 | Checkpoint 恢复 |
| **检查点** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P0 | DurableAgentLoop |
| **暂停/恢复** | ⏳ 简单 | ❌ 无 | ✅ **StateKnot 原生** | P1 | AgentHost 生命周期 |
| **审计日志** | ⏳ 部分 | ❌ 无 | ✅ **StateKnot 原生** | P1 | Journal/Ledger |
| **时间旅行** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | 见 Memory 部分 |
| **分布式执行** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | 多角色部署 |

**JiaClaw 现状**:
- ✅ StateKnot 提供完整的持久化能力（理论上）
- ❌ JiaClaw 尚未配置 PostgreSQL 和持久化参数
- ❌ 需要等待 StateKnot [#92](https://github.com/StateKnot/StateKnot/issues/92) 和 [#94](https://github.com/StateKnot/StateKnot/issues/94)

**目标方案**:
- **P1 PostgreSQL 配置**: 
  - 添加环境变量 `DATABASE_URL`
  - 运行 StateKnot 迁移脚本
  - 配置租户和身份
- **P1 崩溃恢复测试**: 验证 Agent 崩溃后可从最后检查点恢复

**竞争优势**:
- **核心差异化**: 持久化是 JiaClaw（基于 StateKnot）的基石，OpenClaw/Hermes 是后加特性
- at-least-once 执行语义，不重复已完成的调用
- 确定性图执行，保证可重放性

---

### 7. Setup UX（安装与配置）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **安装方式** | `pip install` | `pip install` | `cargo install`（计划） | P1 | - |
| **依赖管理** | Poetry/pip | pip | Cargo | P0 | - |
| **初始化向导** | ✅ `openclaw init` | ⏳ 手动 | ✅ `jiaclaw init` | P1 | - |
| **配置文件** | YAML/TOML | JSON/YAML | ✅ TOML/JSON | P0 | - |
| **运维配置** | ⏳ 基础 | ⏳ 基础 | ✅ **HTTP/CORS/Webhook/限流/TTL/摘要压缩/工具超时/工具循环上限/Heartbeat/web_search/web_fetch/memory_search/memory_write/read_file/list_dir/write_file/metrics/优雅退出** | P1 | - |
| **开发模式** | ✅ 简单 | ✅ 简单 | ✅ `doctor` 诊断 | P1 | - |
| **Docker 镜像** | ✅ 官方 | ⏳ 社区 | ⏳ 计划中（M4） | P1 | - |
| **文档质量** | ✅ 优秀 | ⏳ 中等 | ✅ 持续改进 | P1 | - |

**JiaClaw 现状**:
- ✅ Cargo 工作空间已配置
- ✅ `jiaclaw init` 命令创建工作空间
- ✅ HTTP 配置支持（bind、webhook_secret、telegram_secret、telegram_bot_token、slack_signing_secret、slack_bot_token、discord_public_key、discord_bot_token、可选 `[http.cors]`、可选限流、可选 Session TTL、可选 metrics 公开开关）
- ✅ 可选工具超时（`[agent] tool_timeout_secs` / `JIACLAW_TOOL_TIMEOUT_SECS`）
- ✅ 可配置工具循环上限（`[agent] max_tool_iterations` / `JIACLAW_MAX_TOOL_ITERATIONS`，默认 5，钳制 1–32）
- ✅ 可选 `web_search`（`[tools.web_search] enabled` / `brave_api_key`，`JIACLAW_BRAVE_API_KEY` 优先；doctor 不打印 key）
- ✅ 可选 `web_fetch`（`[tools.web_fetch] enabled` / `allow_private`；默认拒绝私网；doctor 报告是否启用）
- ✅ 可选 `memory_search`（`[tools.memory_search] enabled` 默认 true；默认扫描 MEMORY / SOUL / USER；doctor 报告是否启用）
- ✅ 可选 `memory_write`（`[tools.memory_write] enabled` 默认 true；只写配置的 MEMORY.md；doctor 报告是否启用）
- ✅ 可选 `read_file`（`[tools.read_file] enabled` 默认 true；工作区相对路径只读；256KiB 上限；按行 offset/limit；doctor 报告是否启用）
- ✅ 可选 `list_dir`（`[tools.list_dir] enabled` 默认 true；工作区列目录，默认不递归；doctor 报告是否启用）
- ✅ 可选 `write_file`（`[tools.write_file] enabled` 默认 true；工作区相对路径写入；overwrite/append；256KiB 上限；原子写；doctor 报告是否启用）
- ✅ `jiaclaw doctor` 诊断命令（检查配置、工具、技能、HTTP 设置、限流状态、Metrics 是否公开、Session TTL、优雅退出宽限期、Session 摘要压缩、工具超时、工具循环上限、web_search key 是否配置、web_fetch 是否启用、memory_search / memory_write / read_file / list_dir / write_file 是否启用、MEMORY/SOUL/USER/HEARTBEAT 文件）
- ⏳ 文档持续改进中

**目标方案**:
- **P1 运维配置**: ✅ **已实现**
  - TOML 配置支持 `[http]` 段落
  - 环境变量覆盖（`JIACLAW_WEBHOOK_SECRET`、`JIACLAW_TELEGRAM_SECRET`、`JIACLAW_TELEGRAM_BOT_TOKEN`、`JIACLAW_SLACK_SIGNING_SECRET`、`JIACLAW_SLACK_BOT_TOKEN`、`JIACLAW_DISCORD_PUBLIC_KEY`、`JIACLAW_DISCORD_BOT_TOKEN`、`JIACLAW_CORS_ENABLED`、`JIACLAW_CORS_ORIGINS`、`JIACLAW_RATE_LIMIT_PER_MINUTE`、`JIACLAW_METRICS_REQUIRE_AUTH`、`JIACLAW_SESSION_TTL_SECS`、`JIACLAW_SHUTDOWN_TIMEOUT_SECS`、`JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW`、`JIACLAW_TOOL_TIMEOUT_SECS`、`JIACLAW_MAX_TOOL_ITERATIONS`、`JIACLAW_HEARTBEAT_INTERVAL_SECS`、`JIACLAW_BRAVE_API_KEY` 优先）
  - 可选 CORS（`[http.cors] enabled` 默认 `false` 不发送 CORS 头；`allowed_origins` 精确匹配，`*` 仅显式配置时；`JIACLAW_CORS_ENABLED` / `JIACLAW_CORS_ORIGINS` 可覆盖；OPTIONS preflight 不破坏鉴权/限流/`X-Request-Id`）
  - 可选进程内全局限流（`rate_limit_per_minute`，超限 429 + Retry-After）
  - 可选 Prometheus 文本指标（`GET /metrics`，默认公开；`metrics_public = false` / `JIACLAW_METRICS_REQUIRE_AUTH` 可要求 API 鉴权；不计入限流）
  - 可选会话闲置 TTL（`session_ttl_secs`，过期清理内存 store；落盘开启时同步 save）
  - serve 优雅退出（SIGINT/SIGTERM + `shutdown_timeout_secs`，默认 15s；落盘开启时关闭刷盘；Heartbeat abort）
  - 可选会话摘要压缩（`[session] summarize_on_overflow`，接近上限时折叠旧消息；失败回退硬截断）
  - 可选每工具调用超时（`tool_timeout_secs`，超时写入 tool result 并继续 loop）
  - 可配置工具循环上限（`max_tool_iterations`，默认 5，钳制 1–32；达上限结束本轮）
  - 可选 HEARTBEAT.md 心跳（`[heartbeat] enabled`，仅 serve 进程内按间隔跑一轮 chat）
  - 可选 web_search（`[tools.web_search]`，Brave Search；无 key 友好错误；doctor 不打印 key）
  - 可选 web_fetch（`[tools.web_fetch]`，HTML 去标签；默认拒绝私网；最多 5 次重定向 / ~15s）
  - 可选 memory_search（`[tools.memory_search]`，工作区子串/行窗检索；默认扫描 MEMORY / SOUL / USER；无向量库）
  - 可选 memory_write（`[tools.memory_write]`，只写配置的 MEMORY.md；append/overwrite；上限 32KiB；无向量库/远程 sync）
  - 可选 read_file（`[tools.read_file]`，工作区相对路径只读；按行 offset/limit；上限 256KiB；禁穿越）
  - 可选 list_dir（`[tools.list_dir]`，工作区列目录；默认不递归；禁穿越；无 shell）
  - 可选 write_file（`[tools.write_file]`，工作区相对路径写入；overwrite/append；上限 256KiB；原子写；禁穿越）
  - 启动日志打印配置摘要（不泄露 secret 明文）
- **P1 doctor 诊断**: ✅ **已实现**
  - 检查 workspace 可读性、工具数量、技能数量、MEMORY / SOUL / USER / HEARTBEAT 是否存在及大小
  - HTTP 配置摘要（bind、webhook / Telegram / Slack / Discord 鉴权状态、Telegram/Slack/Discord Bot Token 是否配置（不打印明文）、CORS 是否启用及允许来源、限流是否开启及数值、Metrics 是否公开或需鉴权、Session TTL 是否开启及秒数、优雅退出宽限期、Session 摘要压缩是否开启及 keep_recent、工具超时是否开启及秒数、工具循环上限生效值、web_search 是否启用及 Brave key 是否配置（不打印明文）、web_fetch 是否启用及是否允许私网、memory_search / memory_write / read_file / list_dir / write_file 是否启用、Heartbeat 是否开启及间隔/文件是否存在）
  - 明确提示 stub 模式（当 API key 缺失）
- **P1 文档改进**: 添加 Quick Start 和 Tutorial

---

### 8. Security & Sandbox（安全与沙箱）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **租户隔离** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | StateKnot 租户系统 |
| **资源限制** | ⏳ 简单 | ❌ 无 | ✅ **可选工具超时 + 循环上限** | P1 | StateKnot 资源策略 |
| **工具白名单** | ✅ 配置化 | ⏳ 手动 | ✅ **StateKnot 原生** | P1 | 资源策略 |
| **API Key 管理** | ⏳ 环境变量 | ⏳ 环境变量 | ✅ **配置 + 策略** | P1 | - |
| **HTTP 限流** | ⏳ 部分 | ❌ 无 | ✅ **可选全局** | P1 | - |
| **Prometheus 指标** | ⏳ 部分 | ⏳ 部分 | ✅ **GET /metrics** | P1 | - |
| **Docker 沙箱** | ✅ 可选 | ❌ 无 | ⏳ 计划中（M4） | P2 | - |
| **网络隔离** | ❌ 无 | ❌ 无 | ✅ **StateKnot 策略** | P2 | 资源策略 |

**JiaClaw 现状**:
- ✅ StateKnot 提供租户隔离和资源策略框架
- ✅ 可选 HTTP 全局限流（`rate_limit_per_minute` / `JIACLAW_RATE_LIMIT_PER_MINUTE`）
- ✅ 可选 Prometheus 文本指标（`GET /metrics`，默认公开；可要求 API 鉴权）
- ✅ 可选每工具调用超时（`tool_timeout_secs` / `JIACLAW_TOOL_TIMEOUT_SECS`）
- ✅ 可配置工具循环上限（`max_tool_iterations` / `JIACLAW_MAX_TOOL_ITERATIONS`，默认 5）
- ❌ JiaClaw 尚未配置具体租户策略

**目标方案**:
- **P1 租户隔离**: 在配置中指定 `tenant_id`，隔离不同用户的运行
- **P1 资源限制**: ✅ **可选工具超时与可配置循环上限已实现**（并发上限、预算仍待 StateKnot）
- **P2 Docker 沙箱**: 可选地在 Docker 容器中运行工具

**竞争优势**:
- 企业级安全模型（多租户、策略引擎）
- 默认安全（工具需显式授权）

---

### 9. Scheduling & Cron（调度与定时任务）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **定时触发** | ✅ HEARTBEAT.md / APScheduler | ❌ 无 | ✅ **可选 HEARTBEAT.md（serve 内）** | P2 | - |
| **事件触发** | ✅ Webhook | ❌ 无 | ✅ **Webhook / Telegram / Slack / Discord** | P2 | AgentHost HTTP |
| **任务队列** | ⏳ 简单 | ❌ 无 | ✅ **StateKnot 原生** | P1 | Fair Scheduler |
| **并发控制** | ⏳ 简单 | ❌ 无 | ✅ **StateKnot 原生** | P1 | 调度器策略 |
| **优先级队列** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P2 | Fair Scheduler |

**JiaClaw 现状**:
- ✅ StateKnot 的 Fair Scheduler 支持任务队列和并发控制
- ✅ **可选 `HEARTBEAT.md` 定时心跳**（对照 OpenClaw）：仅挂在 `jiaclaw serve` 生命周期，tokio 间隔任务读取约定文件并跑一轮 chat；默认关闭；无外部 cron 守护进程
- ⏳ 完整 cron 表达式 / 多任务调度仍待 M5
- ✅ 事件触发已有 Webhook / Telegram / Slack / Discord 入站

**目标方案**:
- **P2 定时触发**: ✅ **HEARTBEAT.md 已落地**；通用 cron 配置仍可后续扩展
- **P2 事件触发**: ✅ Webhook / Telegram / Slack / Discord 入站已落地；更丰富的通道仍可通过 AgentHost HTTP 扩展

**竞争优势**:
- 公平调度器支持跨租户的资源公平分配
- 任务持久化，重启后继续执行

---

### 10. Multi-Agent（多智能体协作）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **Agent 间通信** | ⏳ 手动 | ❌ 无 | ✅ **StateKnot A2A** | P1 | A2A 协议 |
| **子图调用** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | 子图节点 |
| **并发 Agent** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | Graph 并行 |
| **Agent 发现** | ❌ 无 | ❌ 无 | ⏳ 计划中（M5） | P2 | A2A 注册中心 |
| **协作策略** | ❌ 无 | ❌ 无 | ⏳ 计划中（M5） | P2 | Graph 编排 |

**JiaClaw 现状**:
- ✅ StateKnot 支持 A2A 协议和 `A2aRemoteAgent`
- ✅ StateKnot Graph 支持子图调用和并行执行
- ❌ JiaClaw 尚未暴露多智能体 API

**目标方案**:
- **P1 A2A 集成**: 配置 A2A 注册中心，发现和调用其他 Agent
- **P2 协作模式**: 设计常见协作模式（主从、流水线、投票）

**竞争优势**:
- **A2A 一等公民**: 与 MCP 同等地位的协议支持
- 多智能体编排自动获得持久化和恢复能力

---

## 优先级总结

### P0 - 关键阻塞（Critical）
| 功能 | 当前状态 | 计划 |
|------|---------|------|
| OpenAI-compatible 提供商 | ✅ 已实现 | 集成 Brokerrouter |
| 基础 CLI | ✅ 完善 | - |
| 配置管理 | ✅ HTTP 配置已添加 | - |
| StateKnot 稳定 API | ❌ pre-alpha | 等待 [#92](https://github.com/StateKnot/StateKnot/issues/92) |

### P1 - 高优先级（High）
| 功能 | 计划时间 | StateKnot 依赖 |
|------|---------|---------------|
| HTTP REST API | M1 | [#92](https://github.com/StateKnot/StateKnot/issues/92) AgentHost |
| SSE 事件流 | ✅ 本切片（完成后分块） | 真流式：[#92](https://github.com/StateKnot/StateKnot/issues/92) + Brokerrouter |
| 技能系统 | M3 | [#96](https://github.com/StateKnot/StateKnot/issues/96) |
| MCP 工具集成 | M2 | [#95](https://github.com/StateKnot/StateKnot/issues/95) |
| 本地工具（Shell/File/HTTP） | M2 | [#95](https://github.com/StateKnot/StateKnot/issues/95) |
| PostgreSQL 持久化 | M1 | [#94](https://github.com/StateKnot/StateKnot/issues/94) |
| 长期记忆（向量数据库） | M3 | - |
| 工作区 MEMORY.md 注入 | ✅ 本切片 | - |
| 工作区 memory_search 本地检索 | ✅ 本切片 | 无向量库 |
| 工作区 memory_write 本地写入 | ✅ 本切片 | 无向量库 / 远程 sync |
| 工作区 read_file / list_dir | ✅ 本切片 | 路径沙箱，无 exec |
| 工作区 write_file | ✅ 本切片 | 路径沙箱，原子写，无 exec |
| 工作区 SOUL.md / USER.md 注入 | ✅ 本切片 | - |
| 工作区 HEARTBEAT.md 定时心跳 | ✅ 本切片 | - |

### P2 - 中优先级（Medium）
| 功能 | 计划时间 |
|------|---------|
| WebSocket 通道 | M4 |
| Discord Interactions 入站 | ✅ 本切片 |
| Slack Events 入站 | ✅ 本切片 |
| 浏览器控制 | M3 |
| Docker 沙箱 | M4 |
| 定时任务 | M5（通用 cron）；HEARTBEAT.md ✅ 本切片 |
| 多智能体协作 | M5 |

---

## 相对优势（JiaClaw vs OpenClaw/Hermes）

### 1. 持久化保证
- **OpenClaw/Hermes**: 简单的会话保存，崩溃后需手动恢复
- **JiaClaw**: 确定性图执行 + 检查点，自动崩溃恢复，at-least-once 语义

### 2. 类型安全
- **OpenClaw/Hermes**: 动态类型（Python），运行时错误
- **JiaClaw**: 类型化 Agent（Rust），编译时检查 + JSON Schema 验证

### 3. 生产治理
- **OpenClaw/Hermes**: 单用户场景，无租户隔离
- **JiaClaw**: 多租户、资源策略、预算控制、审计日志

### 4. 协议原生
- **OpenClaw/Hermes**: 插件式集成第三方工具
- **JiaClaw**: MCP 和 A2A 一等公民，协议层互操作

---

## 相对劣势（JiaClaw vs OpenClaw/Hermes）

### 1. 成熟度
- **OpenClaw/Hermes**: 已有社区生态和大量示例
- **JiaClaw**: 早期开发，等待 StateKnot 稳定

### 2. 易用性
- **OpenClaw/Hermes**: Python 生态，上手快
- **JiaClaw**: Rust + 编译，学习曲线较陡

### 3. 技能/工具库
- **OpenClaw**: 丰富的内置工具和社区技能
- **JiaClaw**: 尚未实现工具系统

---

## 下一步行动（本 PR 后）

### 立即可做（不依赖 StateKnot/Brokerrouter）
1. ✅ **本 PR**: OpenAI-compatible 提供商（临时直连）、工作空间引导、技能骨架
2. 改进文档（Quick Start、Tutorial）
3. 添加更多示例技能
4. 社区参与（博客、视频、讨论）

### 等待 Brokerrouter（M0.5-M1 过渡）
1. 监控 Brokerrouter 仓库可用性
2. 创建 Brokerrouter 集成议题（#1-#10）
3. 实现 `BrokerrouterProvider` 适配器
4. 配置 Brokerrouter 作为默认 Gateway
5. 废弃临时直连模式（保留存根）

### 等待 StateKnot（M1）
1. 监控 [#92](https://github.com/StateKnot/StateKnot/issues/92) 稳定 API 发布
2. 实现 PostgreSQL 持久化配置
3. 集成 StateKnot Model Adapter（与 Brokerrouter 协同）
4. 实现 HTTP/SSE 服务

### 中期目标（M2-M3）
1. 工具系统（MCP + 本地工具）
2. 技能系统完整实现
3. 长期记忆（向量数据库）
4. 持久化执行测试和基准

### 长期目标（M4-M5）
1. 多智能体协作
2. 定时任务和事件触发
3. Docker 部署和 Kubernetes
4. 性能优化和生产部署

---

## 附录：上游依赖清单

### StateKnot 议题

| 议题 | 标题 | 状态 | 链接 |
|------|------|------|------|
| #92 | Stable public API release tracking | Open | https://github.com/StateKnot/StateKnot/issues/92 |
| #93 | Convenience API for simple agent runs | Open | https://github.com/StateKnot/StateKnot/issues/93 |
| #94 | Configuration helpers for PostgreSQL | Open | https://github.com/StateKnot/StateKnot/issues/94 |
| #95 | Documentation: Local Rust tools | Open | https://github.com/StateKnot/StateKnot/issues/95 |
| #96 | Discussion: Skill composition | Open | https://github.com/StateKnot/StateKnot/issues/96 |

### Brokerrouter 议题

详见 [Brokerrouter 差距文档](brokerrouter-gaps.md)

| 议题 | 标题 | 优先级 | 状态 | 链接 |
|------|------|--------|------|------|
| #1 | OpenAI-compatible API 实现 | P0 | 待创建 | [Brokerrouter#1](https://github.com/StateKnot/Brokerrouter/issues/1) |
| #2 | 多提供商支持和自动路由 | P0 | 待创建 | [Brokerrouter#2](https://github.com/StateKnot/Brokerrouter/issues/2) |
| #3 | 认证与授权 | P1 | 待创建 | [Brokerrouter#3](https://github.com/StateKnot/Brokerrouter/issues/3) |
| #4 | 流式响应支持 | P1 | 待创建 | [Brokerrouter#4](https://github.com/StateKnot/Brokerrouter/issues/4) |
| #5 | 工具调用支持 | P1 | 待创建 | [Brokerrouter#5](https://github.com/StateKnot/Brokerrouter/issues/5) |
| ... | *(更多见 brokerrouter-gaps.md)* | - | - | - |

---

**维护者**: JiaClaw contributors  
**更新频率**: 每个 PR 或重大里程碑后更新  
**反馈**: 欢迎提交 Issue 或 PR 补充对比维度
