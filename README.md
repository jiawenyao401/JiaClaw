# JiaClaw

**JiaClaw** 是一个基于 [StateKnot](https://github.com/StateKnot/StateKnot) 框架构建的个人持久化智能体运行时。

[English](#english) | **简体中文**

## 简介

JiaClaw（"爪"）是一个个人"爪式"智能体助手，灵感来自 OpenClaw 风格的个人助手系统。它提供：

- 💬 **聊天驱动交互** - 自然语言对话界面
- 🔧 **工具使用能力** - 集成外部工具和服务
- 🎯 **技能系统** - 可扩展的技能模块
- 💾 **持久化运行** - 支持重启后恢复的持久化执行

## 架构

JiaClaw 构建在 StateKnot 之上，利用其：

- **类型化执行** - 显式状态和结构化输出
- **持久化保证** - 日志化事件、检查点、暂停与恢复
- **协议原生互操作** - MCP 和 A2A 一等公民支持
- **生产治理** - 租户隔离、策略执行、预算控制

详见 [架构文档](docs/architecture.md)。

## 相对 Hermes / OpenClaw

JiaClaw 与 [OpenClaw](https://github.com/openclaw/openclaw) 和 [Hermes Agent](https://github.com/NousResearch/hermes-agent) 的核心差异在于：

- ✅ **持久化优先** - 基于 StateKnot 的确定性图执行、检查点和崩溃恢复（at-least-once 语义）
- ✅ **类型安全** - Rust + TypedAgent<I,O> 提供编译时保证和 JSON Schema 验证
- ✅ **生产治理** - 租户隔离、资源策略、预算控制、审计日志
- ✅ **协议原生** - MCP 和 A2A 一等公民支持，非插件式集成
- ✅ **Gateway 抽象** - 通过 [Brokerrouter](https://github.com/StateKnot/Brokerrouter) 统一模型访问（规划中）
- ⚠️ **早期阶段** - 当前功能有限，等待 StateKnot 和 Brokerrouter 稳定 API

详细对比见 [竞争差距分析](docs/competitive-gap.md)，包含 10 个维度（通道、记忆、技能、工具、模型、持久化、安装、安全、调度、多智能体）的能力矩阵和优先级路线图。

## 当前状态

🚧 **开发中 - Pre-Alpha**

JiaClaw 目前处于早期脚手架阶段。StateKnot 本身也处于 pre-alpha 阶段，核心 API 尚未发布或稳定。

### 已实现

- ✅ Cargo 工作空间结构
- ✅ 核心领域类型（`jiaclaw-core`）
- ✅ StateKnot 集成框架（`jiaclaw`）
- ✅ 可执行宿主骨架（`jiaclaw-host`）
- ✅ 基本 CLI 界面（`serve`, `chat`, `version` 命令）
- ✅ 存根实现（可编译但功能有限）
- ✅ **Brokerrouter 集成** - 生产级 AI Gateway 提供商
  - Bearer 虚拟密钥认证
  - 自动幂等性密钥生成
  - 非流式聊天补全
  - 请求追踪和错误处理
- ✅ **Session 可选落盘** - 进程重启后可恢复多轮对话历史
  - 可配置的持久化开关
  - 原子写入保证数据安全
  - 自动处理文件损坏情况
- ✅ **可选 HTTP 限流** - 进程内全局限流保护 `/api/*`、`/hooks/inbound`、`/hooks/telegram`、`/hooks/slack` 与 `/hooks/discord`
  - 配置 `rate_limit_per_minute` 或环境变量 `JIACLAW_RATE_LIMIT_PER_MINUTE`
  - 启用时受保护路径带 `X-RateLimit-Limit` / `X-RateLimit-Remaining` / `X-RateLimit-Reset`（Unix 纪元秒，配额补满时刻）
  - 超限返回 429 + `Retry-After`（秒）；关闭限流时不发送这些头。`GET /health` 与 `GET /metrics` 始终不限流
- ✅ **可选 HTTP 请求体上限** - 防止超大 body 拖垮内存（对照生产 serve）
  - 配置 `[http] max_body_bytes`（默认 **1048576 / 1MiB**）或环境变量 `JIACLAW_MAX_BODY_BYTES`（正整数优先；`0`/非法回退配置或默认）
  - 超限返回 **413** Payload Too Large，JSON `{"error":"payload_too_large"}`，仍回写 `X-Request-Id`
  - `GET /health` 与 `GET /metrics` 不检查该上限；与鉴权 / 限流 / CORS 兼容。**不做上传 / multipart 存储**
- ✅ **可选 CORS** - 默认关闭（无 CORS 头）。本地浏览器前端可开 `[http.cors]`
  - `enabled` 默认 `false`；`JIACLAW_CORS_ENABLED` 覆盖。`allowed_origins` 精确匹配；`*` 仅在显式配置或 `JIACLAW_CORS_ORIGINS=*` 时
  - 默认方法 GET/POST/DELETE/OPTIONS；允许头 Authorization / Content-Type / X-Request-Id / Accept；暴露 `X-Request-Id` 与限流头（`X-RateLimit-*` / `Retry-After`）
  - OPTIONS preflight 不要求 API Bearer，仍回写 `X-Request-Id`；未匹配 Origin 不回声
- ✅ **可选 GET /metrics** - 进程内 Prometheus 文本（不引入 telemetry SDK）
  - 默认无需 API Bearer（便于 scrape）；`[http] metrics_public = false` 或 `JIACLAW_METRICS_REQUIRE_AUTH=1` 时与 `/api/*` 相同鉴权
  - 计数：`jiaclaw_http_requests_total{path,method,status}`（路由族）、`jiaclaw_sessions_active`、`jiaclaw_tool_calls_total{tool,result}`、`jiaclaw_build_info{version}`
- ✅ **可选 JSON 结构化日志** - `jiaclaw serve` 默认仍为人类可读 text；采集侧可切 JSON 行
  - `[logging] format = "text" | "json"`（默认 `text`，与当前 tracing fmt 完全一致）；可选 `level`
  - 环境变量优先：`JIACLAW_LOG_FORMAT=json`、`JIACLAW_LOG_LEVEL`（再回退 `RUST_LOG`）
  - `format=json` 时每行一条 JSON（`timestamp` / `level` / `target` / `fields` / `message`；`request_id` 作为 tracing 字段），不另起日志系统
  - `jiaclaw doctor` / `serve` 启动摘要打印生效 format（不打印 secret）
- ✅ **可选 Session TTL** - 闲置超时自动清理内存会话（长时间 `serve` 防堆积）
  - 配置 `session_ttl_secs` 或环境变量 `JIACLAW_SESSION_TTL_SECS`（正整数才启用；`0`/非法=关闭）
  - create/chat/get/list 触达刷新；过期后 list 不返回，GET/DELETE/export 与不存在一致（404）
  - `GET /api/sessions/:id/export` 只读导出（默认 JSONL），不刷新 TTL、不触发摘要
  - `POST /api/sessions/import` 导入 JSONL/JSON；已存在默认 409，`?overwrite=true` 替换；不调用 LLM
- ✅ **serve 优雅退出** - `jiaclaw serve` 支持 SIGINT/SIGTERM
  - 停止接受新连接，尽量完成进行中请求；宽限期 `[http] shutdown_timeout_secs`（默认 15 秒），`JIACLAW_SHUTDOWN_TIMEOUT_SECS` 优先（正整数；非法回退默认）
  - 落盘开启时关闭路径原子刷盘 sessions；失败打 error 日志仍退出。Heartbeat 后台任务会被 abort
- ✅ **技能热加载** - 改 `skills/**/SKILL.md` 后无需重启 `serve`
  - `POST /api/skills/reload` 重扫工作区并替换进程内注册表；失败保留旧表
  - Unix：向 serve 发送 `SIGHUP` 走同一路径；Windows 仅 HTTP
  - CLI：`jiaclaw skills reload` 只扫描当前工作区，不通知已运行的 serve
- ✅ **可选 Session 摘要压缩** - 接近消息条数上限时把旧消息折叠成一条摘要，避免硬截断丢上下文
  - 配置 `[session] summarize_on_overflow`（默认 `false`，保持现有丢弃最旧消息行为）或环境变量 `JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1/true` 强制开启
  - `keep_recent` 默认 10；复用当前 LLM provider，固定中英 prompt，无工具且限制 `max_tokens`；失败 warn 并回退硬截断，不让 chat 失败
  - HTTP / Telegram / Slack / Discord / webhook / heartbeat 走同一 session store 写入点
- ✅ **可选工具超时** - 单次 `shell_exec` / `http_get` 等不会无限卡住 tool loop
  - 配置 `[agent] tool_timeout_secs` 或环境变量 `JIACLAW_TOOL_TIMEOUT_SECS`（正整数才启用；`0`/非法=关闭）
  - 超时把 `Tool timed out after Ns` 写入 tool result，不 panic，继续循环
- ✅ **可配置工具循环上限** - 防止失控的工具风暴，同时允许复杂任务提高上限
  - 配置 `[agent] max_tool_iterations`（默认 **5**，与历史硬编码一致）或环境变量 `JIACLAW_MAX_TOOL_ITERATIONS`（正整数优先；`0`/非法忽略）
  - 生效值钳制到 1–32；达上限时写入清晰 tool/assistant 提示并结束本轮
  - `jiaclaw doctor` / `serve` 摘要显示生效值
- ✅ **可选 web_search** - 默认注册的联网检索工具（`query` 必填，`max_results` 默认 5、钳制 1..=10）
  - 配置 `[tools.web_search] enabled`（默认 `true`）与 `brave_api_key`；环境变量 `JIACLAW_BRAVE_API_KEY` 优先
  - 有 key 时调用 Brave Search API（HTTP 超时 10s，并遵守 `tool_timeout_secs`）；无 key 时工具返回友好错误（不访问网络）
  - `enabled = false` 时不注册；`GET /api/tools` / system prompt 只列出已注册工具
  - `jiaclaw doctor` 提示是否配置了 key，**不打印明文**
- ✅ **可选 web_fetch** - 默认注册的网页抓取工具（`url` 必填且仅 http/https，`max_chars` 默认 8000、钳制 500..=50000）
  - 配置 `[tools.web_fetch] enabled`（默认 `true`）与 `allow_private`（默认 `false`，拒绝 localhost/私网）
  - GET 页面；HTML 会去掉 script/style 与标签，返回纯文本（含最终 URL / 标题）；超长注明 `[truncated]`
  - 最多跟随 5 次重定向，HTTP 总体超时约 15s，并遵守 `tool_timeout_secs`；User-Agent 标明 JiaClaw
  - `enabled = false` 时不注册；`GET /api/tools` / system prompt 只列出已注册工具
- ✅ **可选 memory_search** - 默认注册的工作区记忆检索工具（`query` 必填，`max_results` 默认 5、钳制 1..=20）
  - 配置 `[tools.memory_search] enabled`（默认 `true`）；默认扫描配置的 MEMORY / SOUL / USER
  - 大小写不敏感子串 + 行窗匹配，返回 `{path, line, excerpt}`；单文件超过 512KiB 截断并 warn
  - 可选 `paths` 指定工作区相对路径；复用现有 resolve，禁止 `..` / 绝对路径 / symlink 逃逸
  - `enabled = false` 时不注册；**不引入向量数据库**
- ✅ **可选 memory_write** - 默认注册的工作区记忆写入工具（`content` 必填，`mode=append|overwrite` 默认 append）
  - 配置 `[tools.memory_write] enabled`（默认 `true`）；只写配置的 MEMORY.md / `[memory] path`
  - 结果文件超过 32KiB（与注入截断对齐）时明确报错且不落盘；tmp + rename 原子写
  - 忽略 `path` 参数，禁止穿越；返回 `{path, mode, bytes_written}`，不调用 LLM
  - `enabled = false` 时不注册；无危险 shell；**不引入向量库或远程 sync**
- ✅ **可选 read_file** - 默认注册的工作区只读文件工具（`path` 必填，工作区相对路径）
  - 可选 `offset` / `limit` **按行**切片（1-indexed；`offset` 默认 1，`limit` 缺省读到文件末尾）
  - 配置 `[tools.read_file] enabled`（默认 `true`）；超过 **256KiB** 明确报错；二进制（NUL / 非 UTF-8）拒绝读取
  - 路径安全与 MEMORY 对齐：禁止 `..` / 绝对路径 / symlink 逃逸；只读真实文件，无 shell、不调用 LLM
  - `enabled = false` 时不注册；`GET /api/tools` / system prompt / doctor 只反映已注册工具
- ✅ **可选 list_dir** - 默认注册的工作区列目录工具（`path` 默认 `.`）
  - 可选 `max_entries`（默认 200，钳制 1..=1000）、`recursive`（默认 `false`，且不跟随 symlink 目录）
  - 返回 `{path, recursive, truncated, entries:[{name, type, size?}]}`；`type` 为 `file` 或 `dir`
  - 配置 `[tools.list_dir] enabled`（默认 `true`）；禁止穿越 / 绝对路径 / symlink 逃逸
  - `enabled = false` 时不注册；无 shell、不调用 LLM
- ✅ **可选 write_file** - 默认注册的工作区文件写入工具（`path` / `content` 必填，工作区相对路径）
  - 可选 `mode=overwrite|append`（默认 `overwrite`）；可创建中间目录
  - 配置 `[tools.write_file] enabled`（默认 `true`）；结果文件超过 **256KiB**（与 read_file 对齐）明确报错且不落盘
  - tmp + rename 原子写；路径安全与 MEMORY / read_file 对齐：禁止 `..` / 绝对路径 / symlink 逃逸；只写工作区内常规文件
  - `enabled = false` 时不注册；无 shell、不调用 LLM；**不做 exec**
- ✅ **可选 delete_file** - 默认注册的工作区文件删除工具（`path` 必填，工作区相对路径）
  - 配置 `[tools.delete_file] enabled`（默认 `true`）；**只删常规文件**，拒绝目录；文件不存在时明确报错（不静默成功）
  - 路径安全与 MEMORY / read_file / write_file 对齐：禁止 `..` / 绝对路径 / symlink 逃逸
  - 不递归、无 `rm -rf`、无 shell、不调用 LLM；`enabled = false` 时不注册；**不做 exec**
- ✅ **可选 str_replace** - 默认注册的工作区精确字符串替换工具（`path` / `old_str` / `new_str` 必填）
  - 可选 `replace_all`（默认 `false`：必须恰好匹配 1 次，否则报错；`true` 替换全部非重叠匹配）
  - 配置 `[tools.str_replace] enabled`（默认 `true`）；读入或写出超过 **256KiB**（与 read/write 对齐）明确报错且不落盘
  - tmp + rename 原子写；拒绝二进制；路径安全与 MEMORY / read_file 对齐：禁止 `..` / 绝对路径 / symlink 逃逸
  - `enabled = false` 时不注册；无 shell、不调用 LLM；**不做 exec**
- ✅ **可选 grep** - 默认注册的工作区字面量文本搜索工具（`pattern` 必填）
  - 可选 `path`（相对目录或文件，默认 `.`）、`glob`（如 `*.rs`）、`case_insensitive`、`max_matches`（默认 50，钳制 1..=200）
  - **字面量子串**搜索（非正则，避免 ReDoS）；返回 `{path, line, snippet}`，过长行截断
  - 配置 `[tools.grep] enabled`（默认 `true`）；路径安全与其它 file 工具对齐：禁止 `..` / 绝对路径 / symlink 逃逸；不跟随逃逸 symlink；跳过二进制
  - 纯 Rust，无 shell / ripgrep 外部进程、不调用 LLM；`enabled = false` 时不注册；**不做 exec**
- ✅ **Telegram Bot 入站** - `POST /hooks/telegram` 把 Bot API Update 映射到 session `telegram:{chat.id}`
  - 支持 `message.text` / `edited_message.text`；无文本 update 返回 200 并跳过
  - 可选 `JIACLAW_TELEGRAM_SECRET` / `[http] telegram_secret`，校验 `X-Telegram-Bot-Api-Secret-Token`
  - 同步回传 `{ ok: true, reply }` 便于长轮询调试
  - 可选 `JIACLAW_TELEGRAM_BOT_TOKEN` / `[http] telegram_bot_token`：成功回复后调用 Bot `sendMessage` 推回聊天（文本按 4096 截断）；出站失败仍 200 + 原 `reply`（可带 `delivered` / `delivery_error`）
- ✅ **Slack Events API 入站** - `POST /hooks/slack` 把 Events API 映射到 session `slack:{team_id}:{channel}`（无 team 则为 `slack:{channel}`）
  - `type=url_verification` 返回 `{ challenge }`；仅处理 `message` 且 `subtype` 为空（忽略 bot_message / message_changed，避免环）
  - 可选 `JIACLAW_SLACK_SIGNING_SECRET` / `[http] slack_signing_secret`：官方 v0 HMAC-SHA256（`X-Slack-Signature` + `X-Slack-Request-Timestamp`，±5 分钟）；先取 raw body 再反序列化
  - 同步回传 `{ ok: true, reply, session_id }`；无文本/忽略事件 200 + skipped
  - 可选 `JIACLAW_SLACK_BOT_TOKEN` / `[http] slack_bot_token`：成功回复后 `chat.postMessage` 推回 channel；出站失败仍 200 + 原 `reply`
- ✅ **Discord Interactions 入站** - `POST /hooks/discord` 把 slash Chat Input Command 映射到 session `discord:{guild_id}:{channel_id}`（无 guild 则为 `discord:dm:{channel_id}`）
  - `type=1` PING 返回 `{ type: 1 }` PONG；仅处理 `type=2` APPLICATION_COMMAND 的 Chat Input（忽略 Message Component / User Command）
  - 文本取第一个 string option，否则用 `data.name` + 选项拼接
  - 可选 `JIACLAW_DISCORD_PUBLIC_KEY` / `[http] discord_public_key`：官方 Ed25519（`X-Signature-Ed25519` + `X-Signature-Timestamp`，消息为 timestamp + raw body）；先取 raw body 再反序列化。未配置则开放（开发友好，doctor 警告）
  - **deferred ACK**：立即返回 `{ type: 5 }`（Discord 要求 3s 内 ACK），后台跑 chat
  - 可选 `JIACLAW_DISCORD_BOT_TOKEN` / `[http] discord_bot_token`：完成后 `PATCH /webhooks/{application_id}/{interaction_token}/messages/@original` 编辑最终回复（文本按 2000 截断）；无 token 时仍记 session 并 warn，无法 follow-up
  - 不支持 Incoming Webhook 简化体；生产长任务必须走 deferred
- ✅ **OpenAPI 草图** - `GET /api/openapi.json`（鉴权与 `/api/tools` 一致）
- ✅ **可选 SSE 流式** - `POST /api/chat` 在 `Accept: text/event-stream` 或 body `stream: true` 时返回事件流
  - 事件：`meta`（session_id / request_id）、`token`（文本增量）、`tool`（name + ok/error）、`done`（最终 reply）、`error`
  - **当前为整轮 tool loop 完成后的分块推送；Brokerrouter 真流式后续**
  - 未请求流式时 JSON 响应完全不变；鉴权失败仍为 JSON 401
- ✅ **Session 查询 API** - `GET /api/sessions` 列表、`GET /api/sessions/:id` 读取历史、`GET /api/sessions/:id/export` 导出 JSONL/JSON（不存在或过期 404；导出不触发摘要、不改写 store）、`POST /api/sessions/import` 导入 JSONL/JSON（已存在 409，`?overwrite=true` 替换；不调用 LLM）
- ✅ **CLI 会话导出/导入** - `jiaclaw session export <id> [-o file]`；`jiaclaw session import <file> [--id ID] [--overwrite]`；读写落盘 session store
- ✅ **工作区 MEMORY.md** - 跨会话长期记忆注入系统提示
  - 默认 `{workspace}/MEMORY.md`，可用 `[memory] path` 覆盖
  - 每次 `chat` 重读；过大截断（32KiB）并 warn
  - 本地工具 `memory_write`（`mode=append|overwrite`，上限 32KiB）与 `memory_append` 写入；`memory_search` 按关键词检索片段；`jiaclaw doctor` / `jiaclaw memory show`
- ✅ **工作区 SOUL.md / USER.md** - 可选人格与用户画像注入系统提示
  - 默认 `{workspace}/SOUL.md`、`USER.md`，可用 `[identity] soul_path` / `user_path` 覆盖
  - 每次 `chat` 重读；存在且非空则注入独立区块；各文件独立 32KiB 截断并 warn
  - 本地工具 `soul_write` / `user_write`（默认覆盖）；`jiaclaw doctor` / `jiaclaw soul show` / `jiaclaw user show`
- ✅ **可选 HEARTBEAT.md 定时心跳** - 仅 `jiaclaw serve` 进程内按间隔注入一轮 chat
  - 默认 `{workspace}/HEARTBEAT.md`，可用 `[heartbeat] path` 覆盖；`enabled = false` 默认关
  - `interval_secs` 默认 3600，环境变量 `JIACLAW_HEARTBEAT_INTERVAL_SECS`（正整数）可覆盖
  - 固定 `session_id`（默认 `heartbeat`）；文件缺失或为空则跳过本轮；CLI `chat` 不跑心跳

### 待实现

完整的实现需要等待以下 StateKnot 能力：

- ⏳ 稳定的公共 API 发布
- ⏳ `DurableAgentAdmission` 边界
- ⏳ `AgentHost` 和 HTTP 服务集成
- ⏳ PostgreSQL 持久化配置
- ⏳ 模型和工具提供者注册

参见 [StateKnot 能力差距](docs/stateknot-gaps.md) 了解详细跟踪。

## 快速开始

### 前置要求

- Rust 1.88.0 或更高版本（由 StateKnot 要求）
- 可选：PostgreSQL 16+ (用于持久化执行)

### 构建

```bash
# 克隆仓库
git clone https://github.com/jiawenyao401/JiaClaw.git
cd JiaClaw

# 构建工作空间
cargo build

# 运行测试
cargo test
```

### 配置

JiaClaw 支持两种提供商：

1. **Brokerrouter**（推荐）- 通过 AI Gateway 路由模型调用
2. **Stub** - 离线演示模式（无需 API key）

配置示例（`config/jiaclaw.toml`）：

```toml
[provider]
type = "brokerrouter"
base_url = "https://api.brokerrouter.dev"
# API key 通过环境变量提供
model = "claude-3-5-sonnet-20241022"
temperature = 0.7
max_tokens = 4096

[http]
# HTTP 服务绑定地址
bind = "127.0.0.1:8080"

# Webhook 鉴权密钥（可选，环境变量 JIACLAW_WEBHOOK_SECRET 优先）
# webhook_secret = "your-secret-here"

# Telegram Bot secret token（可选，环境变量 JIACLAW_TELEGRAM_SECRET 优先）
# 若设置，/hooks/telegram 校验 X-Telegram-Bot-Api-Secret-Token；未设置则开放（开发友好）
# telegram_secret = "your-telegram-secret-token"

# Telegram Bot API token（可选，环境变量 JIACLAW_TELEGRAM_BOT_TOKEN 优先）
# 若设置，成功得到 assistant 回复后会 POST sendMessage 推回聊天；未设置则仅同步 JSON reply
# telegram_bot_token = "123456:ABC-your-bot-token"

# Slack Events API signing secret（可选，环境变量 JIACLAW_SLACK_SIGNING_SECRET 优先）
# 若设置，/hooks/slack 校验 X-Slack-Signature + X-Slack-Request-Timestamp（v0 HMAC-SHA256，±5 分钟）
# 未设置则开放（开发友好）。签名使用原始 body 字节。
# slack_signing_secret = "your-slack-signing-secret"

# Slack Bot token（可选，环境变量 JIACLAW_SLACK_BOT_TOKEN 优先）
# 若设置，成功得到 assistant 回复后会 POST chat.postMessage 推回 channel；未设置则仅同步 JSON reply
# slack_bot_token = "xoxb-your-bot-token"

# Discord Interactions 公钥（可选，环境变量 JIACLAW_DISCORD_PUBLIC_KEY 优先）
# 若设置，/hooks/discord 校验 X-Signature-Ed25519 + X-Signature-Timestamp（Ed25519，timestamp + raw body）
# 未设置则开放（开发友好）。签名使用原始 body 字节。
# discord_public_key = "your-discord-hex-public-key"

# Discord Bot token（可选，环境变量 JIACLAW_DISCORD_BOT_TOKEN 优先）
# 若设置，deferred ACK 后会 PATCH 编辑原始 Interaction；未设置则仅记 session
# discord_bot_token = "your-discord-bot-token"

# 可选浏览器 CORS（默认关闭，不发送 CORS 头）
# 本地 Web UI：enabled = true，并填写精确 Origin。`*` 仅在显式配置时允许所有来源。
# 环境变量 JIACLAW_CORS_ENABLED、JIACLAW_CORS_ORIGINS（逗号分隔）优先。
# [http.cors]
# enabled = true
# allowed_origins = ["http://localhost:5173"]
# allowed_methods = ["GET", "POST", "DELETE", "OPTIONS"]
# allowed_headers = ["Authorization", "Content-Type", "X-Request-Id", "Accept"]
# expose_headers = ["X-Request-Id", "X-RateLimit-Limit", "X-RateLimit-Remaining", "X-RateLimit-Reset", "Retry-After"]
# max_age_secs = 600

# Session 持久化配置（可选）
persist = true  # 启用 session 持久化
persist_path = ".jiaclaw/sessions.json"

# HTTP 限流（可选，环境变量 JIACLAW_RATE_LIMIT_PER_MINUTE 优先）
# 正整数：对 /api/* 与 /hooks/inbound、/hooks/telegram、/hooks/slack、/hooks/discord 做进程内全局限流（次/分钟）
# 未设置或 0：不限流。GET /health 与 GET /metrics 始终不限流。
# 启用时受保护路径带 X-RateLimit-Limit / Remaining / Reset（Unix 秒，配额补满时刻）；超限 429 + Retry-After（秒）。
# 关闭时不发送这些头。
# rate_limit_per_minute = 60

# HTTP 请求体上限（可选，环境变量 JIACLAW_MAX_BODY_BYTES 优先）
# 正整数：超过该字节数返回 413 Payload Too Large（JSON error=payload_too_large，仍带 X-Request-Id）
# 未设置、0 或非法回退默认 1048576（1MiB）。GET /health 与 GET /metrics 不检查。
# 不做上传 / multipart 存储。
# max_body_bytes = 1048576

# GET /metrics 是否公开（默认 true）。false 或 JIACLAW_METRICS_REQUIRE_AUTH=1 时与 /api/* 相同鉴权
# metrics_public = true

# Session 闲置 TTL（可选，环境变量 JIACLAW_SESSION_TTL_SECS 优先）
# 正整数：闲置超过该秒数后从内存 store 删除（落盘开启时同步 save）
# 未设置或 0：不启用。create/chat/get/list 触达会刷新。
# session_ttl_secs = 3600

# 优雅退出宽限期（可选，环境变量 JIACLAW_SHUTDOWN_TIMEOUT_SECS 优先）
# SIGINT/SIGTERM 后停止 accept，等待进行中请求；正整数生效，非法/0 回退默认 15 秒。
# 落盘开启时关闭路径会原子刷盘 sessions。
# shutdown_timeout_secs = 15

[logging]
# 日志格式: text（默认，人类可读，与当前 tracing 一致）或 json（每行一条 JSON）
# 环境变量 JIACLAW_LOG_FORMAT 优先
# format = "json"
# 日志级别指令（可选）。优先级：JIACLAW_LOG_LEVEL > RUST_LOG > 本字段 > info
# level = "info"

[session]
# 接近消息条数上限（50）时是否摘要压缩（默认 false，保持硬截断）
# 环境变量 JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1/true 可强制开启
# summarize_on_overflow = true
# 摘要后保留的最近消息条数（默认 10）
# keep_recent = 10

# 工具调用超时写在 [agent] 段（环境变量 JIACLAW_TOOL_TIMEOUT_SECS 优先）
# 正整数启用；未设置或 0 不限制（默认）。超时写入 tool result 并继续 loop。
# [agent]
# tool_timeout_secs = 30
# 工具循环上限（默认 5，与历史硬编码一致；JIACLAW_MAX_TOOL_ITERATIONS 优先；钳制 1–32）
# max_tool_iterations = 5

[memory]
# 工作区长期记忆（相对于 workspace，默认 MEMORY.md；缺省本段即可）
path = "MEMORY.md"

[identity]
# 人格 / 用户画像（相对于 workspace；缺省本段即为 SOUL.md / USER.md）
soul_path = "SOUL.md"
user_path = "USER.md"

[heartbeat]
# 可选定时心跳（仅 jiaclaw serve；默认关。CLI chat 不跑）
# enabled = true
# interval_secs = 3600
# path = "HEARTBEAT.md"
# session_id = "heartbeat"

[tools.web_search]
# 可选联网检索（默认启用并注册）。无 Brave key 时调用返回友好错误，不访问网络。
# 环境变量 JIACLAW_BRAVE_API_KEY 优先于 brave_api_key；doctor / 日志不打印明文。
# 申请: https://brave.com/search/api/
enabled = true
# brave_api_key = "BSA..."

[tools.web_fetch]
# 可选网页抓取（默认启用并注册）。GET 页面，HTML 去标签为可读文本。
# 默认拒绝 localhost / 私网；内网调试可设 allow_private = true。
enabled = true
# allow_private = false

[tools.memory_search]
# 可选工作区记忆检索（默认启用并注册）。按关键词在 MEMORY / SOUL / USER 中找片段。
# 返回 {path, line, excerpt}；单文件超过 512KiB 截断。禁止路径穿越。不引入向量库。
enabled = true

[tools.memory_write]
# 可选工作区记忆写入（默认启用并注册）。只写配置的 MEMORY.md / [memory] path。
# content 必填；mode=append（默认）追加，mode=overwrite 覆盖。结果上限 32KiB。原子写。
# 忽略 path 参数，禁止穿越。enabled = false 不注册。不引入向量库或远程 sync。
enabled = true

[tools.read_file]
# 可选工作区只读文件（默认启用并注册）。path 为工作区相对路径；可选 offset/limit 按行切片（1-indexed）。
# 超过 256KiB 明确报错；二进制拒绝读取。禁止 .. / 绝对路径 / symlink 逃逸。enabled = false 不注册。
enabled = true

[tools.list_dir]
# 可选工作区列目录（默认启用并注册）。path 默认 . ；可选 max_entries（默认 200）。默认不递归。
# 返回 name / type(file|dir) / 可选 size。禁止穿越。enabled = false 不注册。无 shell。
enabled = true

[tools.write_file]
# 可选工作区文件写入（默认启用并注册）。path / content 必填；mode=overwrite|append（默认 overwrite）。
# 结果超过 256KiB（与 read_file 对齐）报错且不落盘。原子写。禁止穿越 / symlink 逃逸。enabled = false 不注册。
enabled = true

[tools.delete_file]
# 可选工作区文件删除（默认启用并注册）。path 必填。只删常规文件，拒绝目录；缺文件明确报错。
# 禁止穿越 / 绝对路径 / symlink 逃逸。不递归、无 shell。enabled = false 不注册。
enabled = true

[tools.str_replace]
# 可选工作区精确字符串替换（默认启用并注册）。path / old_str / new_str 必填；replace_all 默认 false（必须恰好 1 次）。
# 读入或写出超过 256KiB（与 read/write 对齐）报错且不落盘。原子写。拒绝二进制。禁止穿越 / symlink 逃逸。
# enabled = false 不注册。
enabled = true

[tools.grep]
# 可选工作区字面量文本搜索（默认启用并注册）。pattern 必填（字面量子串，非正则，避免 ReDoS）。
# 可选 path（相对目录或文件，默认 .）、glob（如 *.rs）、case_insensitive、max_matches（默认 50，钳制 1..=200）。
# 返回 {path, line, snippet}；跳过二进制；禁止穿越 / symlink 逃逸。无 shell / ripgrep。enabled = false 不注册。
enabled = true
```

或使用环境变量：

```bash
export JIACLAW_API_KEY=brk_live_your_key_here
# 可选：浏览器 CORS（默认关闭；1/true 开启，优先于 [http.cors] enabled）
# export JIACLAW_CORS_ENABLED=1
# export JIACLAW_CORS_ORIGINS=http://localhost:5173,https://app.example
# 可选：HTTP 限流（次/分钟，优先于配置文件）
# export JIACLAW_RATE_LIMIT_PER_MINUTE=60
# 可选：GET /metrics 要求与 /api/* 相同鉴权（优先于 [http] metrics_public）
# export JIACLAW_METRICS_REQUIRE_AUTH=1
# 可选：serve 日志格式（text 默认；json 为每行一条 JSON，优先于 [logging] format）
# export JIACLAW_LOG_FORMAT=json
# 可选：日志级别指令（优先于 RUST_LOG 与 [logging] level）
# export JIACLAW_LOG_LEVEL=info
# 可选：Session 闲置 TTL（秒，优先于配置文件）
# export JIACLAW_SESSION_TTL_SECS=3600
# 可选：serve 优雅退出宽限期（秒，优先于 [http] shutdown_timeout_secs；正整数，非法/0 回退 15）
# export JIACLAW_SHUTDOWN_TIMEOUT_SECS=15
# 可选：接近消息上限时摘要压缩（1/true 强制开启，优先于 [session] summarize_on_overflow）
# export JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1
# 可选：单次工具调用超时（秒，优先于配置文件）
# export JIACLAW_TOOL_TIMEOUT_SECS=30
# 可选：工具循环上限（优先于 [agent] max_tool_iterations；正整数，0/非法忽略；钳制 1–32）
# export JIACLAW_MAX_TOOL_ITERATIONS=8
# 可选：Heartbeat 间隔（秒，优先于 [heartbeat] interval_secs；需 enabled = true）
# export JIACLAW_HEARTBEAT_INTERVAL_SECS=3600
# 可选：Brave Search API key（优先于 [tools.web_search] brave_api_key；不要把 key 提交到仓库）
# export JIACLAW_BRAVE_API_KEY=BSA...
```

### 运行示例

```bash
# 显示版本信息
cargo run --bin jiaclaw -- version

# 初始化工作空间
cargo run --bin jiaclaw -- init

# 检查配置和环境
cargo run --bin jiaclaw -- doctor

# 查看工作区长期记忆（MEMORY.md）
cargo run --bin jiaclaw -- memory show

# 查看人格 / 用户画像（可选；缺失不报错）
cargo run --bin jiaclaw -- soul show
cargo run --bin jiaclaw -- user show

# 导出会话（JSONL；默认 stdout。读 `[http] persist_path` 落盘 store）
cargo run --bin jiaclaw -- session export <session-id>
cargo run --bin jiaclaw -- session export <session-id> -o session.jsonl

# 导入会话（JSONL 或 `{id?, messages}` JSON；不调用 LLM。已存在需 --overwrite）
cargo run --bin jiaclaw -- session import session.jsonl
cargo run --bin jiaclaw -- session import session.jsonl --id restored-id --overwrite

# 列出已发现的技能
cargo run --bin jiaclaw -- skills
cargo run --bin jiaclaw -- skills --verbose  # 显示详细信息

# 重新扫描工作区 skills/（只扫描本进程；不会通知已运行的 serve）
cargo run --bin jiaclaw -- skills reload
# 运行中的 serve：POST /api/skills/reload，或 Unix 向进程发送 SIGHUP（Windows 仅 HTTP）

# 运行单次聊天（需要配置 API key）
export JIACLAW_API_KEY=brk_live_...
cargo run --bin jiaclaw -- chat "你好，JiaClaw"

# 或使用配置文件
cargo run --bin jiaclaw -- chat --config config/jiaclaw.toml "你好，JiaClaw"

# 启动交互式 REPL 模式
cargo run --bin jiaclaw -- chat

# REPL 模式支持多种选项
cargo run --bin jiaclaw -- chat --skill calculator --skill web_search  # 启用特定技能
cargo run --bin jiaclaw -- chat --session my-session-id                 # 使用会话 ID
cargo run --bin jiaclaw -- chat --no-auto-skill                         # 禁用自动技能激活

# 启动 HTTP 服务
cargo run --bin jiaclaw -- serve --bind 127.0.0.1:8080

### REPL 交互式对话示例

```bash
# 进入 REPL 模式（不提供消息参数）
cargo run --bin jiaclaw -- chat

# 在 REPL 中：
# 👤 > 你好
# 🤖 你好！我是 JiaClaw...
#
# 👤 > 列出工作空间
# 🔧 工具调用:
#    • workspace_list
# 🤖 根据工具执行结果，操作已完成...
#
# 👤 > exit
# 👋 再见！
```

**REPL 支持的选项：**
- `--skill <name>`: 启用特定技能（可多次使用）
- `--session <id>`: 指定会话 ID，便于续聊
- `--no-auto-skill`: 禁用技能自动激活
- 输入 `exit` 或 `quit` 退出，或按 `Ctrl+D`

### HTTP API 示例

# 测试 HTTP API
curl -D - http://127.0.0.1:8080/health
# 响应含 X-Request-Id；也可自行传入：
# curl -D - -H "X-Request-Id: my-trace-id" http://127.0.0.1:8080/health

# Prometheus 文本指标（默认无需 Bearer；metrics_public=false 或 JIACLAW_METRICS_REQUIRE_AUTH=1 时需鉴权）
curl -D - http://127.0.0.1:8080/metrics

# OpenAPI 3 草图（未配置 API token 时可匿名访问；已启用 token 时需 Bearer，与 /api/tools 一致）
curl http://127.0.0.1:8080/api/openapi.json

# 列出已注册的工具
curl http://127.0.0.1:8080/api/tools

# 列出已发现的技能（进程内注册表；启动或最近一次 reload 的结果）
curl http://127.0.0.1:8080/api/skills

# 热加载技能（改 skills/**/SKILL.md 后无需重启 serve）
curl -X POST http://127.0.0.1:8080/api/skills/reload

# 列出会话（创建后可见；message_count 为内存中当前条数）
curl http://127.0.0.1:8080/api/sessions

# 无状态聊天
curl -X POST http://127.0.0.1:8080/api/chat \
  -H "Content-Type: application/json" \
  -d '{"messages": [{"role": "user", "content": "你好"}]}'

# 带 session 的多轮对话
SESSION_ID=$(uuidgen)
curl -X POST http://127.0.0.1:8080/api/chat \
  -H "Content-Type: application/json" \
  -d "{\"session_id\": \"$SESSION_ID\", \"messages\": [{\"role\": \"user\", \"content\": \"我叫张三\"}]}"

curl -X POST http://127.0.0.1:8080/api/chat \
  -H "Content-Type: application/json" \
  -d "{\"session_id\": \"$SESSION_ID\", \"messages\": [{\"role\": \"user\", \"content\": \"我是谁？\"}]}"

# 可选 SSE 流式（Accept 或 body.stream=true；当前为完成后分块推送，Brokerrouter 真流式后续）
curl -N -X POST http://127.0.0.1:8080/api/chat \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{"messages": [{"role": "user", "content": "你好"}]}'

curl -N -X POST http://127.0.0.1:8080/api/chat \
  -H "Content-Type: application/json" \
  -d '{"stream": true, "messages": [{"role": "user", "content": "你好"}]}'

# 读取会话历史（不存在返回 404）
curl http://127.0.0.1:8080/api/sessions/$SESSION_ID

# 导出会话 JSONL（每行一条消息；可选 ?format=json 返回 {id, messages}）
curl -D - http://127.0.0.1:8080/api/sessions/$SESSION_ID/export
curl http://127.0.0.1:8080/api/sessions/$SESSION_ID/export?format=json

# 导入会话 JSONL（可选 ?id=；已存在默认 409，?overwrite=true 替换）
curl -X POST http://127.0.0.1:8080/api/sessions/import?id=$SESSION_ID \
  -H "Content-Type: application/x-ndjson" \
  --data-binary @session.jsonl
curl -X POST "http://127.0.0.1:8080/api/sessions/import?format=json&overwrite=true" \
  -H "Content-Type: application/json" \
  -d "{\"id\": \"$SESSION_ID\", \"messages\": [{\"role\": \"user\", \"content\": \"恢复\"}]}"

# 删除会话
curl -X DELETE http://127.0.0.1:8080/api/sessions/$SESSION_ID

# Webhook 入站（无需鉴权）
curl -X POST http://127.0.0.1:8080/hooks/inbound \
  -H "Content-Type: application/json" \
  -d '{"channel": "webhook", "chat_id": "user123", "text": "你好，通过 webhook"}'

# Webhook 入站（带鉴权）
export JIACLAW_WEBHOOK_SECRET=my_secret_key
cargo run --bin jiaclaw -- serve --bind 127.0.0.1:8080

curl -X POST http://127.0.0.1:8080/hooks/inbound \
  -H "Content-Type: application/json" \
  -H "X-Webhook-Secret: my_secret_key" \
  -d '{"channel": "webhook", "chat_id": "user456", "text": "认证的消息", "username": "alice"}'

# Telegram Bot 入站（样例 Update；同步回传 assistant 文本，便于长轮询调试）
curl -X POST http://127.0.0.1:8080/hooks/telegram \
  -H "Content-Type: application/json" \
  -d '{"update_id": 1, "message": {"message_id": 10, "chat": {"id": 4242, "type": "private"}, "text": "你好 Telegram"}}'

# Telegram 入站（配置 secret token 后校验官方头）
export JIACLAW_TELEGRAM_SECRET=my_tg_secret
curl -X POST http://127.0.0.1:8080/hooks/telegram \
  -H "Content-Type: application/json" \
  -H "X-Telegram-Bot-Api-Secret-Token: my_tg_secret" \
  -d '{"update_id": 2, "edited_message": {"message_id": 11, "chat": {"id": 4242}, "text": "编辑后的消息"}}'

# 配置 Bot Token 后，webhook 成功回复会再调用 Telegram sendMessage 推回聊天
export JIACLAW_TELEGRAM_BOT_TOKEN=123456:ABC-your-bot-token

# 将公网 HTTPS URL 登记为 Bot webhook（secret_token 对应 JIACLAW_TELEGRAM_SECRET）
curl "https://api.telegram.org/bot${JIACLAW_TELEGRAM_BOT_TOKEN}/setWebhook" \
  -d "url=https://your-host.example/hooks/telegram" \
  -d "secret_token=${JIACLAW_TELEGRAM_SECRET}"
# 查看 webhook 状态：curl "https://api.telegram.org/bot${JIACLAW_TELEGRAM_BOT_TOKEN}/getWebhookInfo"

# Slack Events API URL 验证（Request URL 指向 https://your-host.example/hooks/slack）
curl -X POST http://127.0.0.1:8080/hooks/slack \
  -H "Content-Type: application/json" \
  -d '{"type":"url_verification","challenge":"3eZbrw1aBm2rZgRNFdxV2595E9CY3gmdALWMmHkvFXO7tYXAYM8P"}'

# Slack 入站（普通 message；session slack:{team_id}:{channel}）
curl -X POST http://127.0.0.1:8080/hooks/slack \
  -H "Content-Type: application/json" \
  -d '{"type":"event_callback","team_id":"T123","event":{"type":"message","channel":"C456","user":"U789","text":"你好 Slack"}}'

# 配置 signing secret 后校验官方签名头（生产应开启；开发未配置则开放）
export JIACLAW_SLACK_SIGNING_SECRET=your-slack-signing-secret
# Slack 会带 X-Slack-Request-Timestamp 与 X-Slack-Signature: v0=<hmac-sha256-hex>

# 配置 Bot Token 后，成功回复会再调用 chat.postMessage 推回 channel
export JIACLAW_SLACK_BOT_TOKEN=xoxb-your-bot-token

# Discord Interactions Ping（Interactions Endpoint URL 指向 https://your-host.example/hooks/discord）
curl -X POST http://127.0.0.1:8080/hooks/discord \
  -H "Content-Type: application/json" \
  -d '{"type":1}'

# Discord Chat Input Command（立即 {type:5} deferred；session discord:{guild}:{channel}）
curl -X POST http://127.0.0.1:8080/hooks/discord \
  -H "Content-Type: application/json" \
  -d '{"type":2,"application_id":"APP","guild_id":"G123","channel_id":"C456","token":"interaction-token","data":{"name":"ask","type":1,"options":[{"name":"prompt","type":3,"value":"你好 Discord"}]}}'

# 配置公钥后校验官方签名头（生产应开启；开发未配置则开放，doctor 会警告）
export JIACLAW_DISCORD_PUBLIC_KEY=your-discord-hex-public-key
# Discord 会带 X-Signature-Timestamp 与 X-Signature-Ed25519（hex）

# 配置 Bot Token 后，deferred 完成会 PATCH 编辑原始 Interaction
export JIACLAW_DISCORD_BOT_TOKEN=your-discord-bot-token
```

**注意**：
- 使用 Brokerrouter 需要有效的虚拟密钥（`brk_live_...`）
- 无 API key 时自动回退到存根模式（演示功能）
- StateKnot 持久化功能尚未集成
- 可选 HTTP 限流：设置 `JIACLAW_RATE_LIMIT_PER_MINUTE` 或 `[http] rate_limit_per_minute` 后，`/api/*` 与 `/hooks/inbound`、`/hooks/telegram`、`/hooks/slack`、`/hooks/discord` 带 `X-RateLimit-Limit` / `X-RateLimit-Remaining` / `X-RateLimit-Reset`（Unix 纪元秒，表示剩余配额补满到 Limit 的时刻）；超限返回 `429` + `Retry-After`（秒）。未启用则不发送这些头。`GET /health` 与 `GET /metrics` 不限流
- 可选 HTTP 请求体上限：`[http] max_body_bytes` 默认 **1048576（1MiB）**，`JIACLAW_MAX_BODY_BYTES` 正整数优先（`0`/非法回退）。超限返回 `413` + `{"error":"payload_too_large"}`，仍回写 `X-Request-Id`。`GET /health` 与 `GET /metrics` 不检查。不做上传 / multipart 存储
- 可选 CORS：默认关闭、不发送 CORS 头。`[http.cors] enabled = true` 或 `JIACLAW_CORS_ENABLED=1` 后，匹配的 `Origin` 获得 `Access-Control-Allow-Origin`；`JIACLAW_CORS_ORIGINS` 逗号分隔覆盖 `allowed_origins`。`*` 仅在显式配置时允许所有来源。OPTIONS preflight 不要求 API Bearer，仍回写 `X-Request-Id`。未匹配 Origin 不回声
- 可选 Prometheus 指标：`GET /metrics` 默认公开；`[http] metrics_public = false` 或 `JIACLAW_METRICS_REQUIRE_AUTH=1` 时与 `/api/*` 相同鉴权。进程内计数（HTTP 路由族、session 数、工具调用、build_info），不引入 telemetry SDK
- 可选 JSON 结构化日志：默认 `[logging] format = "text"` 与当前人类可读 tracing 一致。`format = "json"` 或 `JIACLAW_LOG_FORMAT=json` 时每行一条 JSON（含 `timestamp` / `level` / `target` / `fields` / `message`，`request_id` 作为字段）。`JIACLAW_LOG_LEVEL` 优先于 `RUST_LOG`。doctor / serve 启动打印生效 format，不打印 secret
- 可选 Session TTL：设置 `JIACLAW_SESSION_TTL_SECS` 或 `[http] session_ttl_secs`（正整数）后，闲置超时的会话会从 store 删除；`GET /api/sessions` 只返回未过期项，过期 id 的 GET/export 为 404
- serve 优雅退出：`jiaclaw serve` 支持 SIGINT/SIGTERM；停止 accept 并等待进行中请求。`[http] shutdown_timeout_secs` 默认 15 秒，`JIACLAW_SHUTDOWN_TIMEOUT_SECS` 优先（正整数；非法回退默认）。落盘开启时关闭路径刷盘；Heartbeat 任务 abort
- 技能热加载：`POST /api/skills/reload` 重扫 `skills/` 并替换进程内表，失败保留旧表。Unix `SIGHUP` 同样路径；Windows 仅 HTTP。`jiaclaw skills reload` 只扫描当前工作区，不通知 serve
- 可选 Session 摘要压缩：设置 `[session] summarize_on_overflow = true` 或 `JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1` 后，接近上限时把旧消息折叠为一条 `[session-summary]` system 消息并保留最近 `keep_recent`（默认 10）条；未开启则仍硬截断。摘要失败会 warn 并回退截断，chat 不失败
- 可选工具超时：设置 `JIACLAW_TOOL_TIMEOUT_SECS` 或 `[agent] tool_timeout_secs`（正整数）后，单次 tool 超过该秒数会把 `Tool timed out after Ns` 写入 tool result 并继续循环；未设置则不限制
- 可配置工具循环上限：设置 `JIACLAW_MAX_TOOL_ITERATIONS` 或 `[agent] max_tool_iterations`（默认 5）；正整数生效，`0`/非法忽略，钳制 1–32。达上限时写入 tool/assistant 提示并结束本轮
- 请求追踪：所有响应回写 `X-Request-Id`；请求未携带时服务端生成 UUID。chat/webhook 日志带上该 ID
- OpenAPI 草图：`GET /api/openapi.json`（鉴权与 `GET /api/tools` 一致）
- 可选 SSE：`POST /api/chat` 在 `Accept: text/event-stream` 或 `"stream": true` 时返回 `text/event-stream`（`meta` / `token` / `tool` / `done` / `error`）。未请求流式时 JSON 不变。**当前为分块推送；Brokerrouter 真流式后续**
- Session 查询：`GET /api/sessions` 列出 `{id, message_count}`；`GET /api/sessions/:id` 返回消息；`GET /api/sessions/:id/export` 默认 JSONL（`?format=json` 整包）；不存在或过期 404。导出只读，不触发摘要、不改写 store。`POST /api/sessions/import` 导入 JSONL/JSON，已存在 409，`?overwrite=true` 替换，不调用 LLM。CLI：`jiaclaw session export <id> [-o file]`；`jiaclaw session import <file> [--id ID] [--overwrite]`
- Telegram Bot 入站：`POST /hooks/telegram` 解析 Bot API Update（`message.text` / `edited_message.text`），会话键 `telegram:{chat.id}`；无文本返回 200 + 跳过说明。可选 `JIACLAW_TELEGRAM_SECRET`。配置 `JIACLAW_TELEGRAM_BOT_TOKEN` 后会调用 `sendMessage` 出站（文本超 4096 截断）；出站失败仍返回 200 + 原 `reply`，避免 Telegram 重试。用 `setWebhook` 把公网 `https://…/hooks/telegram` 登记到 Bot，并可带 `secret_token`
- Slack Events API 入站：`POST /hooks/slack` 处理 `url_verification`（回传 `{ challenge }`）与 `event_callback`（仅 `message` 且 `subtype` 为空）；会话键 `slack:{team_id}:{channel}`（无 team 则为 `slack:{channel}`）。可选 `JIACLAW_SLACK_SIGNING_SECRET`（官方 v0 HMAC-SHA256，先取 raw body）。配置 `JIACLAW_SLACK_BOT_TOKEN` 后会调用 `chat.postMessage` 出站；出站失败仍 200 + 原 `reply`。在 Slack 应用的 Event Subscriptions 把 Request URL 指到公网 `https://…/hooks/slack`
- Discord Interactions 入站：`POST /hooks/discord` 处理 PING（`type=1` → `{ type: 1 }`）与 Chat Input Command（`type=2`）；会话键 `discord:{guild_id}:{channel_id}`（无 guild 则为 `discord:dm:{channel_id}`）。立即 `{ type: 5 }` deferred ACK，后台跑 chat。可选 `JIACLAW_DISCORD_PUBLIC_KEY`（官方 Ed25519，先取 raw body）。配置 `JIACLAW_DISCORD_BOT_TOKEN` 后 PATCH 编辑原始消息；无 token 时仍记 session。生产长任务需 deferred（3s ACK）。在 Discord 应用的 Interactions Endpoint URL 指到公网 `https://…/hooks/discord`
- 可选 HEARTBEAT.md：`[heartbeat] enabled = true` 后仅 `jiaclaw serve` 按间隔读取约定文件全文并跑一轮 chat（固定 session，默认 `heartbeat`）。`JIACLAW_HEARTBEAT_INTERVAL_SECS` 可覆盖间隔。文件缺失/空则跳过；CLI `chat` 不跑心跳
- 可选 web_search：默认注册。设置 `JIACLAW_BRAVE_API_KEY` 或 `[tools.web_search] brave_api_key` 后调用 Brave Search；未配置 key 时返回友好错误。`enabled = false` 不注册。doctor 不打印 key
- 可选 web_fetch：默认注册。GET `http`/`https` URL，HTML 转为可读文本；默认拒绝 localhost/私网（`[tools.web_fetch] allow_private = true` 可放开）。`enabled = false` 不注册
- 可选 memory_search：默认注册。在 MEMORY / SOUL / USER（或安全相对路径）中按关键词检索行窗片段；`enabled = false` 不注册。不引入向量数据库
- 可选 memory_write：默认注册。只写配置的 MEMORY.md；`mode=append|overwrite`（默认 append）；结果超过 32KiB 报错；`enabled = false` 不注册。无向量库/远程 sync
- 可选 read_file：默认注册。读取工作区相对路径文本文件；`offset`/`limit` 按行（1-indexed）；超过 256KiB 或二进制报错；禁穿越。`enabled = false` 不注册。无 shell
- 可选 list_dir：默认注册。列出工作区目录（默认 `.`，不递归）；返回 name/type/size；禁穿越。`enabled = false` 不注册。无 shell
- 可选 write_file：默认注册。写入工作区相对路径常规文件；`mode=overwrite|append`（默认 overwrite）；超过 256KiB 报错且不落盘；原子写；禁穿越。`enabled = false` 不注册。无 shell / exec
- 可选 delete_file：默认注册。删除工作区相对路径常规文件；拒绝目录；缺文件明确报错；禁穿越 / symlink 逃逸。`enabled = false` 不注册。无递归 / shell / exec
- 可选 str_replace：默认注册。单文件精确字符串替换；`replace_all` 默认 false（必须恰好 1 次）；超过 256KiB 报错且不落盘；原子写；禁穿越。`enabled = false` 不注册。无 shell / exec
- 可选 grep：默认注册。工作区字面量文本搜索（非正则）；`path` 默认 `.`；可选 `glob` / `case_insensitive` / `max_matches`（默认 50）；禁穿越。`enabled = false` 不注册。无 shell / ripgrep / exec

## 项目结构

```
JiaClaw/
├── crates/
│   ├── jiaclaw-core/     # 核心领域类型和契约
│   ├── jiaclaw/          # StateKnot 集成和 Agent 包装器
│   └── jiaclaw-host/     # 可执行宿主和 CLI
├── docs/                 # 架构文档和设计说明
│   ├── architecture.md
│   ├── stateknot-gaps.md
│   └── roadmap.md
├── examples/             # 示例工作空间（含 MEMORY.md / SOUL.md / USER.md / HEARTBEAT.md 说明）
├── Cargo.toml            # 工作空间清单
└── README.md
```

## 文档

- [架构概览](docs/architecture.md) - 系统设计和组件
- [竞争差距分析](docs/competitive-gap.md) - 相对 OpenClaw/Hermes 的能力对比和路线图
- [StateKnot 能力差距](docs/stateknot-gaps.md) - 当前限制和追踪的上游议题
- [Brokerrouter 能力差距](docs/brokerrouter-gaps.md) - AI Gateway 集成需求和议题
- [路线图](docs/roadmap.md) - 开发计划和里程碑

## 贡献

JiaClaw 使用与 StateKnot 兼容的许可证（Apache-2.0 或 MIT）。

在提交贡献前：

1. 阅读 StateKnot 的 [CONTRIBUTING.md](https://github.com/StateKnot/StateKnot/blob/main/CONTRIBUTING.md)
2. 检查 [StateKnot 能力差距](docs/stateknot-gaps.md) 了解当前限制
3. 遵循 Rust 2024 edition 惯例
4. 运行 `cargo fmt` 和 `cargo clippy`

## 许可证

本项目采用双许可证：

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE) 或 http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) 或 http://opensource.org/licenses/MIT)

根据您的选择，您可以选择其中一个许可证。

---

## English

**JiaClaw** is a personal durable agent runtime built on the [StateKnot](https://github.com/StateKnot/StateKnot) framework.

### Introduction

JiaClaw (meaning "claw" in Chinese) is a personal "claw-style" agent assistant inspired by OpenClaw-style personal assistant systems. It provides:

- 💬 **Chat-driven interaction** - Natural language conversation interface
- 🔧 **Tool-using capabilities** - Integration with external tools and services
- 🎯 **Skill system** - Extensible skill modules
- 💾 **Durable runs** - Persistent execution that survives restarts

### Current Status

🚧 **In Development - Pre-Alpha**

JiaClaw is currently in early scaffolding stage. StateKnot itself is also pre-alpha with unpublished and unstable core APIs.

#### Implemented

- ✅ Cargo workspace structure
- ✅ Core domain types (`jiaclaw-core`)
- ✅ StateKnot integration framework (`jiaclaw`)
- ✅ Executable host skeleton (`jiaclaw-host`)
- ✅ Basic CLI interface (`serve`, `chat`, `version` commands)
- ✅ Stub implementation (compiles but limited functionality)
- ✅ **Brokerrouter Integration** - Production-grade AI Gateway provider
  - Bearer virtual key authentication
  - Automatic idempotency key generation
  - Non-streaming chat completions
  - Request tracing and error handling
- ✅ **Optional Session Persistence** - Restore multi-turn conversations after restart
  - Configurable persistence toggle
  - Atomic writes for data safety
  - Automatic handling of corrupted files
- ✅ **Optional HTTP rate limiting** - process-wide limit for `/api/*`, `/hooks/inbound`, `/hooks/telegram`, `/hooks/slack`, and `/hooks/discord`
  - Configure `rate_limit_per_minute` or `JIACLAW_RATE_LIMIT_PER_MINUTE`
  - When enabled, protected paths send `X-RateLimit-Limit` / `X-RateLimit-Remaining` / `X-RateLimit-Reset` (Unix epoch seconds when remaining returns to Limit)
  - Over-limit returns 429 + `Retry-After` (seconds); headers are omitted when rate limiting is off. `GET /health` and `GET /metrics` are never limited
- ✅ **Optional HTTP max request body** - reject oversized bodies before they exhaust memory
  - Configure `[http] max_body_bytes` (default **1048576 / 1MiB**) or `JIACLAW_MAX_BODY_BYTES` (positive integer wins; `0`/invalid falls back to config or default)
  - Over-limit returns **413** Payload Too Large with JSON `{"error":"payload_too_large"}` and still writes `X-Request-Id`
  - `GET /health` and `GET /metrics` skip the check; compatible with auth / rate limiting / CORS. **No upload / multipart storage**
- ✅ **Optional CORS** - off by default (no CORS headers). Enable `[http.cors]` for a local browser UI
  - `enabled` defaults to `false`; override with `JIACLAW_CORS_ENABLED`. `allowed_origins` is exact-match; `*` only when configured explicitly or `JIACLAW_CORS_ORIGINS=*`
  - Default methods GET/POST/DELETE/OPTIONS; allowed headers Authorization / Content-Type / X-Request-Id / Accept; expose `X-Request-Id` and rate-limit headers (`X-RateLimit-*` / `Retry-After`)
  - OPTIONS preflight does not require an API Bearer and still writes `X-Request-Id`; unmatched origins are never echoed
- ✅ **Optional GET /metrics** - in-process Prometheus text (no telemetry SDK)
  - Public by default for scraping; `[http] metrics_public = false` or `JIACLAW_METRICS_REQUIRE_AUTH=1` uses the same auth as `/api/*`
  - Series: `jiaclaw_http_requests_total{path,method,status}` (route family), `jiaclaw_sessions_active`, `jiaclaw_tool_calls_total{tool,result}`, `jiaclaw_build_info{version}`
- ✅ **Optional JSON structured logs** - `jiaclaw serve` stays human-readable `text` by default; collectors can switch to JSON lines
  - `[logging] format = "text" | "json"` (default `text`, identical to the current tracing fmt); optional `level`
  - Env vars win: `JIACLAW_LOG_FORMAT=json`, `JIACLAW_LOG_LEVEL` (then `RUST_LOG`)
  - `format=json` emits one JSON object per line (`timestamp` / `level` / `target` / `fields` / `message`; `request_id` is a tracing field) on the existing tracing subscriber
  - `jiaclaw doctor` / `serve` startup prints the effective format (never secrets)
- ✅ **Optional Session TTL** - idle sessions are expired to avoid unbounded memory growth during long `serve`
  - Configure `session_ttl_secs` or `JIACLAW_SESSION_TTL_SECS` (positive integer enables; `0`/invalid disables)
  - create/chat/get/list refresh last access; expired ids are omitted from list and GET/DELETE/export match not-found (404)
  - `GET /api/sessions/:id/export` is a read-only dump (JSONL by default); it does not refresh TTL or trigger summarization
  - `POST /api/sessions/import` imports JSONL/JSON; existing ids return 409 unless `?overwrite=true`; does not call the LLM
- ✅ **Graceful serve shutdown** - `jiaclaw serve` handles SIGINT/SIGTERM
  - Stops accepting, drains in-flight requests; grace period `[http] shutdown_timeout_secs` (default 15s), `JIACLAW_SHUTDOWN_TIMEOUT_SECS` wins (positive integer; invalid falls back to default)
  - When persist is on, shutdown flushes sessions atomically; save failure is logged and the process still exits. Heartbeat background tasks are aborted
- ✅ **Skill hot-reload** - changing `skills/**/SKILL.md` does not require restarting `serve`
  - `POST /api/skills/reload` rescan the workspace and replace the in-process registry; failures keep the old table
  - Unix: `SIGHUP` uses the same path; Windows is HTTP-only
  - CLI: `jiaclaw skills reload` scans this process only and does not notify a running serve
- ✅ **Optional session summary compression** - fold older messages into one summary near the session cap instead of dropping them
  - Configure `[session] summarize_on_overflow` (default `false`, keeps hard truncation) or `JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1/true` to force-enable
  - `keep_recent` defaults to 10; reuses the current LLM provider with a fixed bilingual prompt, no tools, and a small `max_tokens`; on failure, warn and fall back to truncation without failing chat
  - HTTP / Telegram / Slack / Discord / webhook / heartbeat share the same session-store write path
- ✅ **Optional tool timeout** - a long `shell_exec` / `http_get` cannot stall the whole tool loop
  - Configure `[agent] tool_timeout_secs` or `JIACLAW_TOOL_TIMEOUT_SECS` (positive integer enables; `0`/invalid disables)
  - Timeout writes `Tool timed out after Ns` into the tool result, does not panic, and continues the loop
- ✅ **Configurable tool-loop cap** - stop runaway tool storms while allowing complex tasks a higher limit
  - Configure `[agent] max_tool_iterations` (default **5**, same as the former hardcoded cap) or `JIACLAW_MAX_TOOL_ITERATIONS` (positive integer wins; `0`/invalid ignored)
  - Effective value is clamped to 1–32; hitting the cap writes a clear tool/assistant hint and ends the turn
  - `jiaclaw doctor` / `serve` summaries show the effective value
- ✅ **Optional web_search** - registered by default (`query` required; `max_results` defaults to 5, clamped to 1..=10)
  - Configure `[tools.web_search] enabled` (default `true`) and `brave_api_key`; `JIACLAW_BRAVE_API_KEY` overrides the file
  - With a key, calls Brave Search (10s HTTP timeout, still honors `tool_timeout_secs`); without a key, the tool returns a friendly error and does not hit the network
  - `enabled = false` skips registration; `GET /api/tools` and the system prompt only list registered tools
  - `jiaclaw doctor` reports whether a key is set and **never prints the secret**
- ✅ **Optional web_fetch** - registered by default (`url` required, http/https only; `max_chars` defaults to 8000, clamped to 500..=50000)
  - Configure `[tools.web_fetch] enabled` (default `true`) and `allow_private` (default `false`, blocks localhost/private ranges)
  - GET the page; HTML is stripped of script/style/tags into plain text (final URL / title included); oversize results note `[truncated]`
  - Follows at most 5 redirects, ~15s HTTP timeout, still honors `tool_timeout_secs`; User-Agent identifies JiaClaw
  - `enabled = false` skips registration; `GET /api/tools` and the system prompt only list registered tools
- ✅ **Optional memory_search** - registered by default (`query` required; `max_results` defaults to 5, clamped to 1..=20)
  - Configure `[tools.memory_search] enabled` (default `true`); default scan is the configured MEMORY / SOUL / USER files
  - Case-insensitive substring + line-window matching; returns `{path, line, excerpt}`; files over 512KiB are truncated with a warning
  - Optional `paths` for workspace-relative files; reuses existing resolve (no `..` / absolute / symlink escape)
  - `enabled = false` skips registration; **no vector database**
- ✅ **Optional memory_write** - registered by default (`content` required; `mode=append|overwrite`, default `append`)
  - Configure `[tools.memory_write] enabled` (default `true`); writes only the configured MEMORY.md / `[memory] path`
  - Resulting file over 32KiB (aligned with prompt injection truncation) is rejected and not written; tmp + rename atomic write
  - Ignores a `path` argument (no traversal); returns `{path, mode, bytes_written}`; does not call an LLM
  - `enabled = false` skips registration; no dangerous shell; **no vector DB or remote sync**
- ✅ **Optional read_file** - registered by default (`path` required, workspace-relative)
  - Optional `offset` / `limit` slice **by line** (1-indexed; `offset` defaults to 1, omitted `limit` reads to EOF)
  - Configure `[tools.read_file] enabled` (default `true`); files over **256KiB** error; binary (NUL / non-UTF-8) is rejected
  - Same path policy as MEMORY: no `..` / absolute paths / symlink escape; real files only; no shell, no LLM
  - `enabled = false` skips registration; `GET /api/tools` / system prompt / doctor only show registered tools
- ✅ **Optional list_dir** - registered by default (`path` defaults to `.`)
  - Optional `max_entries` (default 200, clamped to 1..=1000) and `recursive` (default `false`; symlink dirs are not followed)
  - Returns `{path, recursive, truncated, entries:[{name, type, size?}]}`; `type` is `file` or `dir`
  - Configure `[tools.list_dir] enabled` (default `true`); traversal / absolute / symlink escape is rejected
  - `enabled = false` skips registration; no shell, no LLM
- ✅ **Optional write_file** - registered by default (`path` / `content` required, workspace-relative)
  - Optional `mode=overwrite|append` (default `overwrite`); intermediate directories may be created
  - Configure `[tools.write_file] enabled` (default `true`); resulting file over **256KiB** (aligned with read_file) is rejected and not written
  - tmp + rename atomic write; same path policy as MEMORY / read_file: no `..` / absolute / symlink escape; regular in-workspace files only
  - `enabled = false` skips registration; no shell, no LLM; **no exec**
- ✅ **Optional delete_file** - registered by default (`path` required, workspace-relative)
  - Configure `[tools.delete_file] enabled` (default `true`); **regular files only**; directories are rejected; missing files return an explicit error (never silent success)
  - Same path policy as MEMORY / read_file / write_file: no `..` / absolute / symlink escape
  - No recursion, no `rm -rf`, no shell, no LLM; `enabled = false` skips registration; **no exec**
- ✅ **Optional str_replace** - registered by default (`path` / `old_str` / `new_str` required)
  - Optional `replace_all` (default `false`: must match exactly once, otherwise error; `true` replaces all non-overlapping matches)
  - Configure `[tools.str_replace] enabled` (default `true`); read or write over **256KiB** (aligned with read/write) is rejected and not written
  - tmp + rename atomic write; binary files are refused; same path policy as MEMORY / read_file: no `..` / absolute / symlink escape
  - `enabled = false` skips registration; no shell, no LLM; **no exec**
- ✅ **Optional grep** - registered by default (`pattern` required)
  - Optional `path` (relative directory or file, default `.`), `glob` (e.g. `*.rs`), `case_insensitive`, `max_matches` (default 50, clamped to 1..=200)
  - **Literal substring** search (not regex, avoids ReDoS); returns `{path, line, snippet}` with long lines truncated
  - Configure `[tools.grep] enabled` (default `true`); same path policy as other file tools: no `..` / absolute / symlink escape; does not follow escaping symlinks; skips binary files
  - Pure Rust; no shell / ripgrep subprocess, no LLM; `enabled = false` skips registration; **no exec**
- ✅ **Telegram Bot inbound** - `POST /hooks/telegram` maps Bot API Updates onto session `telegram:{chat.id}`
  - Supports `message.text` / `edited_message.text`; updates without text return 200 and are skipped
  - Optional `JIACLAW_TELEGRAM_SECRET` / `[http] telegram_secret`, checked via `X-Telegram-Bot-Api-Secret-Token`
  - Sync JSON `{ ok: true, reply }` for long-poll debugging
  - Optional `JIACLAW_TELEGRAM_BOT_TOKEN` / `[http] telegram_bot_token`: after a successful reply, call Bot `sendMessage` (text truncated at 4096). Outbound failure still returns 200 + the original `reply` (may include `delivered` / `delivery_error`)
- ✅ **Slack Events API inbound** - `POST /hooks/slack` maps Events API payloads onto session `slack:{team_id}:{channel}` (or `slack:{channel}` if team is missing)
  - `type=url_verification` returns `{ challenge }`; only `message` events with empty `subtype` (ignore `bot_message` / `message_changed` to avoid loops)
  - Optional `JIACLAW_SLACK_SIGNING_SECRET` / `[http] slack_signing_secret`: official v0 HMAC-SHA256 (`X-Slack-Signature` + `X-Slack-Request-Timestamp`, ±5 minutes); raw body is captured before JSON parse
  - Sync JSON `{ ok: true, reply, session_id }`; no-text / ignored events return 200 + skipped
  - Optional `JIACLAW_SLACK_BOT_TOKEN` / `[http] slack_bot_token`: after a successful reply, call `chat.postMessage`; outbound failure still returns 200 + the original `reply`
- ✅ **Discord Interactions inbound** - `POST /hooks/discord` maps Chat Input Commands onto session `discord:{guild_id}:{channel_id}` (or `discord:dm:{channel_id}` if guild is missing)
  - `type=1` PING returns `{ type: 1 }` PONG; only Chat Input `APPLICATION_COMMAND` (`type=2`) is handled (Message Component / User Command are skipped)
  - Text comes from the first string option, otherwise `data.name` plus flattened options
  - Optional `JIACLAW_DISCORD_PUBLIC_KEY` / `[http] discord_public_key`: official Ed25519 (`X-Signature-Ed25519` + `X-Signature-Timestamp` over timestamp + raw body). Unset stays open (dev-friendly; doctor warns)
  - **Deferred ACK**: immediately returns `{ type: 5 }` (Discord requires an ACK within 3s) and runs chat in the background
  - Optional `JIACLAW_DISCORD_BOT_TOKEN` / `[http] discord_bot_token`: after chat, `PATCH /webhooks/{application_id}/{interaction_token}/messages/@original` (text truncated at 2000). Without a token the session is still recorded and a warning is logged
  - Incoming Webhook-style bodies are not accepted; production long-running work must use deferred
- ✅ **OpenAPI sketch** - `GET /api/openapi.json` (auth matches `/api/tools`)
- ✅ **Optional SSE** - `POST /api/chat` returns `text/event-stream` when `Accept: text/event-stream` or body `stream: true`
  - Events: `meta` (session_id / request_id), `token` (text chunks), `tool` (name + ok/error), `done` (final reply), `error`
  - **Currently chunked after the full tool loop; Brokerrouter true streaming comes later**
  - JSON responses stay unchanged when streaming is not requested; auth failures remain JSON 401
- ✅ **Session query API** - `GET /api/sessions` list, `GET /api/sessions/:id` history, `GET /api/sessions/:id/export` JSONL/JSON dump (404 if missing or expired; export does not summarize or rewrite the store), `POST /api/sessions/import` JSONL/JSON import (409 if the id exists, `?overwrite=true` replaces; does not call the LLM)
- ✅ **CLI session export/import** - `jiaclaw session export <id> [-o file]`; `jiaclaw session import <file> [--id ID] [--overwrite]`; reads/writes the on-disk session store
- ✅ **Workspace MEMORY.md** - cross-session facts injected into the system prompt
  - Default `{workspace}/MEMORY.md`, overridable via `[memory] path`
  - Re-read on every `chat`; truncate at 32KiB with a warning
  - Local tools `memory_write` (`mode=append|overwrite`, 32KiB cap) and `memory_append`; `memory_search` for keyword snippets; `jiaclaw doctor` / `jiaclaw memory show`
- ✅ **Workspace SOUL.md / USER.md** - optional persona and user-profile injection
  - Default `{workspace}/SOUL.md` and `USER.md`, overridable via `[identity] soul_path` / `user_path`
  - Re-read on every `chat`; independent 32KiB truncation per file
  - Local tools `soul_write` / `user_write` (replace by default); `jiaclaw soul show` / `jiaclaw user show`
- ✅ **Optional HEARTBEAT.md** - periodic self-check chat, only inside `jiaclaw serve`
  - Default `{workspace}/HEARTBEAT.md`, overridable via `[heartbeat] path`; `enabled = false` by default
  - `interval_secs` defaults to 3600; `JIACLAW_HEARTBEAT_INTERVAL_SECS` (positive integer) overrides
  - Fixed `session_id` (default `heartbeat`); missing/empty file skips the tick; CLI `chat` does not run heartbeats

#### Pending

Full implementation requires the following StateKnot capabilities:

- ⏳ Stable public API release
- ⏳ `DurableAgentAdmission` boundary
- ⏳ `AgentHost` and HTTP service integration
- ⏳ PostgreSQL persistence configuration
- ⏳ Model and tool provider registration

See [StateKnot Capability Gaps](docs/stateknot-gaps.md) for detailed tracking.

### Quick Start

#### Prerequisites

- Rust 1.88.0 or higher (required by StateKnot)
- Optional: PostgreSQL 16+ (for durable execution)

#### Build

```bash
# Clone the repository
git clone https://github.com/jiawenyao401/JiaClaw.git
cd JiaClaw

# Build workspace
cargo build

# Run tests
cargo test
```

#### Configuration

JiaClaw supports two providers:

1. **Brokerrouter** (recommended) - Routes model calls through AI Gateway
2. **Stub** - Offline demo mode (no API key required)

Example configuration (`config/jiaclaw.toml`):

```toml
[provider]
type = "brokerrouter"
base_url = "https://api.brokerrouter.dev"
# API key via environment variable
model = "claude-3-5-sonnet-20241022"
temperature = 0.7
max_tokens = 4096

[http]
# HTTP service bind address
bind = "127.0.0.1:8080"

# Webhook authentication secret (optional, JIACLAW_WEBHOOK_SECRET env var takes priority)
# webhook_secret = "your-secret-here"

# Telegram Bot secret token (optional, JIACLAW_TELEGRAM_SECRET env var takes priority)
# When set, /hooks/telegram checks X-Telegram-Bot-Api-Secret-Token; unset stays open (dev-friendly)
# telegram_secret = "your-telegram-secret-token"

# Telegram Bot API token (optional, JIACLAW_TELEGRAM_BOT_TOKEN env var takes priority)
# When set, a successful assistant reply is also POSTed via sendMessage; unset keeps sync JSON only
# telegram_bot_token = "123456:ABC-your-bot-token"

# Slack Events API signing secret (optional, JIACLAW_SLACK_SIGNING_SECRET env var takes priority)
# When set, /hooks/slack checks X-Slack-Signature + X-Slack-Request-Timestamp (v0 HMAC-SHA256, ±5 minutes)
# Unset stays open (dev-friendly). Signature uses the raw request body bytes.
# slack_signing_secret = "your-slack-signing-secret"

# Slack Bot token (optional, JIACLAW_SLACK_BOT_TOKEN env var takes priority)
# When set, a successful assistant reply is also POSTed via chat.postMessage; unset keeps sync JSON only
# slack_bot_token = "xoxb-your-bot-token"

# Discord Interactions public key (optional, JIACLAW_DISCORD_PUBLIC_KEY env var takes priority)
# When set, /hooks/discord checks X-Signature-Ed25519 + X-Signature-Timestamp (Ed25519 over timestamp + raw body)
# Unset stays open (dev-friendly). Signature uses the raw request body bytes.
# discord_public_key = "your-discord-hex-public-key"

# Discord Bot token (optional, JIACLAW_DISCORD_BOT_TOKEN env var takes priority)
# When set, deferred ACK is followed by PATCH of the original Interaction; unset only records the session
# discord_bot_token = "your-discord-bot-token"

# Optional browser CORS (disabled by default; no CORS headers)
# For a local Web UI set enabled = true and list exact origins. `*` allows all only when written explicitly.
# JIACLAW_CORS_ENABLED and JIACLAW_CORS_ORIGINS (comma-separated) take priority.
# [http.cors]
# enabled = true
# allowed_origins = ["http://localhost:5173"]
# allowed_methods = ["GET", "POST", "DELETE", "OPTIONS"]
# allowed_headers = ["Authorization", "Content-Type", "X-Request-Id", "Accept"]
# expose_headers = ["X-Request-Id", "X-RateLimit-Limit", "X-RateLimit-Remaining", "X-RateLimit-Reset", "Retry-After"]
# max_age_secs = 600

# Session persistence config (optional)
persist = true  # Enable session persistence
persist_path = ".jiaclaw/sessions.json"

# Optional HTTP rate limit (JIACLAW_RATE_LIMIT_PER_MINUTE env var takes priority)
# Positive integer: process-wide limit for /api/* and /hooks/inbound, /hooks/telegram, /hooks/slack, /hooks/discord (requests/minute)
# Unset or 0: disabled. GET /health and GET /metrics are never limited.
# When enabled, protected paths send X-RateLimit-Limit / Remaining / Reset (Unix seconds, when remaining returns to Limit); 429 adds Retry-After (seconds).
# Headers are omitted when rate limiting is off.
# rate_limit_per_minute = 60

# Optional HTTP max request body (JIACLAW_MAX_BODY_BYTES env var takes priority)
# Positive integer: over-limit returns 413 Payload Too Large (JSON error=payload_too_large, still writes X-Request-Id)
# Unset, 0, or invalid falls back to 1048576 (1MiB). GET /health and GET /metrics skip the check.
# No upload / multipart storage.
# max_body_bytes = 1048576

# Whether GET /metrics is public (default true). false or JIACLAW_METRICS_REQUIRE_AUTH=1 uses the same auth as /api/*
# metrics_public = true

# Optional session idle TTL (JIACLAW_SESSION_TTL_SECS env var takes priority)
# Positive integer: expire idle sessions after this many seconds (saved to disk when persist is on)
# Unset or 0: disabled. create/chat/get/list refresh last access.
# session_ttl_secs = 3600

# Graceful shutdown timeout (JIACLAW_SHUTDOWN_TIMEOUT_SECS env var takes priority)
# After SIGINT/SIGTERM, stop accepting and wait for in-flight requests; invalid/0 falls back to 15s.
# When persist is on, the shutdown path flushes sessions with the same atomic write.
# shutdown_timeout_secs = 15

[logging]
# Log format: text (default, human-readable, same as current tracing) or json (one JSON object per line)
# JIACLAW_LOG_FORMAT overrides this
# format = "json"
# Optional level directive. Precedence: JIACLAW_LOG_LEVEL > RUST_LOG > this field > info
# level = "info"

[session]
# When approaching the message cap (50), summarize older turns instead of dropping them (default false)
# JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1/true force-enables this
# summarize_on_overflow = true
# keep_recent = 10

# Per-tool timeout lives on [agent] (JIACLAW_TOOL_TIMEOUT_SECS env var takes priority)
# Positive integer enables; unset or 0 is unlimited (default). Timeout writes into the tool result and the loop continues.
# [agent]
# tool_timeout_secs = 30
# Tool-loop cap (default 5, same as the former hardcoded limit; JIACLAW_MAX_TOOL_ITERATIONS wins; clamped 1–32)
# max_tool_iterations = 5

[memory]
# Workspace long-term memory (relative to workspace, default MEMORY.md)
path = "MEMORY.md"

[identity]
# Persona / user profile (relative to workspace; omit this section for SOUL.md / USER.md defaults)
soul_path = "SOUL.md"
user_path = "USER.md"

[heartbeat]
# Optional periodic heartbeat (jiaclaw serve only; off by default. CLI chat does not run it)
# enabled = true
# interval_secs = 3600
# path = "HEARTBEAT.md"
# session_id = "heartbeat"

[tools.web_search]
# Optional web search (registered by default). Without a Brave key the tool returns a friendly error.
# JIACLAW_BRAVE_API_KEY overrides brave_api_key; doctor / logs never print the secret.
# Get a key: https://brave.com/search/api/
enabled = true
# brave_api_key = "BSA..."

[tools.web_fetch]
# Optional page fetch (registered by default). GET a URL and convert HTML to readable text.
# Localhost / private ranges are blocked by default; set allow_private = true for intranet use.
enabled = true
# allow_private = false

[tools.memory_search]
# Optional workspace memory search (registered by default). Keyword/substring scan of MEMORY / SOUL / USER.
# Returns {path, line, excerpt}; files over 512KiB are truncated. Path traversal is rejected. No vector DB.
enabled = true

[tools.memory_write]
# Optional workspace memory write (registered by default). Writes only the configured MEMORY.md / [memory] path.
# content is required; mode=append (default) or overwrite. Result cap 32KiB. Atomic write.
# Path arguments are ignored (no traversal). enabled = false skips registration. No vector DB / remote sync.
enabled = true

[tools.read_file]
# Optional workspace file read (registered by default). path is workspace-relative; optional offset/limit are 1-indexed lines.
# Over 256KiB is rejected; binary (NUL / non-UTF-8) is refused. No .. / absolute / symlink escape.
enabled = true

[tools.list_dir]
# Optional workspace directory listing (registered by default). path defaults to . ; max_entries default 200.
# recursive defaults to false. Returns name / type(file|dir) / optional size. Traversal is rejected. No shell.
enabled = true

[tools.write_file]
# Optional workspace file write (registered by default). path / content required; mode=overwrite|append (default overwrite).
# Result over 256KiB (aligned with read_file) is rejected and not written. Atomic write. No traversal / symlink escape.
enabled = true

[tools.delete_file]
# Optional workspace file delete (registered by default). path required. Regular files only; directories rejected; missing files error.
# No traversal / absolute / symlink escape. No recursion / shell. enabled = false skips registration.
enabled = true

[tools.str_replace]
# Optional workspace exact string replace (registered by default). path / old_str / new_str required; replace_all default false (exactly one match).
# Read or write over 256KiB (aligned with read/write) is rejected and not written. Atomic write. Binary refused. No traversal / symlink escape.
# enabled = false skips registration.
enabled = true

[tools.grep]
# Optional workspace literal text search (registered by default). pattern required (literal substring, not regex, avoids ReDoS).
# Optional path (relative directory or file, default .), glob (e.g. *.rs), case_insensitive, max_matches (default 50, clamped 1..=200).
# Returns {path, line, snippet}; skips binary; no traversal / symlink escape. No shell / ripgrep. enabled = false skips registration.
enabled = true
```

Or use environment variable:

```bash
export JIACLAW_API_KEY=brk_live_your_key_here
# Optional: browser CORS (off by default; 1/true enables, overrides [http.cors] enabled)
# export JIACLAW_CORS_ENABLED=1
# export JIACLAW_CORS_ORIGINS=http://localhost:5173,https://app.example
# Optional: HTTP rate limit (requests/minute, overrides config file)
# export JIACLAW_RATE_LIMIT_PER_MINUTE=60
# Optional: require /api/* auth for GET /metrics (overrides [http] metrics_public)
# export JIACLAW_METRICS_REQUIRE_AUTH=1
# Optional: serve log format (text default; json is one JSON object per line; overrides [logging] format)
# export JIACLAW_LOG_FORMAT=json
# Optional: log level directive (overrides RUST_LOG and [logging] level)
# export JIACLAW_LOG_LEVEL=info
# Optional: session idle TTL in seconds (overrides config file)
# export JIACLAW_SESSION_TTL_SECS=3600
# Optional: graceful shutdown timeout in seconds (overrides [http] shutdown_timeout_secs; invalid/0 falls back to 15)
# export JIACLAW_SHUTDOWN_TIMEOUT_SECS=15
# Optional: summarize older session messages near the cap (1/true force-enables)
# export JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1
# Optional: per-tool timeout in seconds (overrides config file)
# export JIACLAW_TOOL_TIMEOUT_SECS=30
# Optional: tool-loop cap (overrides [agent] max_tool_iterations; positive integer, 0/invalid ignored; clamped 1–32)
# export JIACLAW_MAX_TOOL_ITERATIONS=8
# Optional: heartbeat interval in seconds (overrides [heartbeat] interval_secs; requires enabled = true)
# export JIACLAW_HEARTBEAT_INTERVAL_SECS=3600
# Optional: Brave Search API key (overrides [tools.web_search] brave_api_key; do not commit secrets)
# export JIACLAW_BRAVE_API_KEY=BSA...
```

#### Run Examples

```bash
# Show version info
cargo run --bin jiaclaw -- version

# Initialize workspace
cargo run --bin jiaclaw -- init

# Check configuration and environment
cargo run --bin jiaclaw -- doctor

# Show workspace long-term memory (MEMORY.md)
cargo run --bin jiaclaw -- memory show

# Show persona / user profile (optional; missing files are fine)
cargo run --bin jiaclaw -- soul show
cargo run --bin jiaclaw -- user show

# Export a session (JSONL; stdout by default. Reads `[http] persist_path`)
cargo run --bin jiaclaw -- session export <session-id>
cargo run --bin jiaclaw -- session export <session-id> -o session.jsonl

# Import a session (JSONL or `{id?, messages}` JSON; does not call the LLM. Use --overwrite if the id exists)
cargo run --bin jiaclaw -- session import session.jsonl
cargo run --bin jiaclaw -- session import session.jsonl --id restored-id --overwrite

# List discovered skills
cargo run --bin jiaclaw -- skills
cargo run --bin jiaclaw -- skills --verbose

# Rescan workspace skills/ (this CLI process only; does not notify a running serve)
cargo run --bin jiaclaw -- skills reload
# For a running serve: POST /api/skills/reload, or send SIGHUP on Unix (HTTP only on Windows)
export JIACLAW_API_KEY=brk_live_...
cargo run --bin jiaclaw -- chat "Hello, JiaClaw"

# Or use config file
cargo run --bin jiaclaw -- chat --config config/jiaclaw.toml "Hello, JiaClaw"

# Start interactive REPL mode
cargo run --bin jiaclaw -- chat

# REPL mode supports various options
cargo run --bin jiaclaw -- chat --skill calculator --skill web_search  # Enable specific skills
cargo run --bin jiaclaw -- chat --session my-session-id                 # Use session ID
cargo run --bin jiaclaw -- chat --no-auto-skill                         # Disable auto skill activation

# Start HTTP service
cargo run --bin jiaclaw -- serve --bind 127.0.0.1:8080

#### REPL Interactive Chat Example

```bash
# Enter REPL mode (without message argument)
cargo run --bin jiaclaw -- chat

# In REPL:
# 👤 > Hello
# 🤖 Hello! I'm JiaClaw...
#
# 👤 > list workspace
# 🔧 Tool calls:
#    • workspace_list
# 🤖 Based on the tool execution results...
#
# 👤 > exit
# 👋 Goodbye!
```

**REPL Options:**
- `--skill <name>`: Enable specific skills (can be used multiple times)
- `--session <id>`: Specify session ID for conversation continuity
- `--no-auto-skill`: Disable automatic skill activation
- Type `exit` or `quit` to exit, or press `Ctrl+D`

# Test HTTP API
curl -D - http://127.0.0.1:8080/health
# Response includes X-Request-Id; you may also send your own:
# curl -D - -H "X-Request-Id: my-trace-id" http://127.0.0.1:8080/health

# Prometheus text metrics (public by default; auth required when metrics_public=false or JIACLAW_METRICS_REQUIRE_AUTH=1)
curl -D - http://127.0.0.1:8080/metrics

# OpenAPI 3 sketch (anonymous when no API token; Bearer required when token is enabled, same as /api/tools)
curl http://127.0.0.1:8080/api/openapi.json

# List loaded skills (in-process registry)
curl http://127.0.0.1:8080/api/skills

# Hot-reload skills without restarting serve
curl -X POST http://127.0.0.1:8080/api/skills/reload

# List sessions
curl http://127.0.0.1:8080/api/sessions

# Stateless chat
curl -X POST http://127.0.0.1:8080/api/chat \
  -H "Content-Type: application/json" \
  -d '{"messages": [{"role": "user", "content": "Hello"}]}'

# Multi-turn conversation with session
SESSION_ID=$(uuidgen)
curl -X POST http://127.0.0.1:8080/api/chat \
  -H "Content-Type: application/json" \
  -d "{\"session_id\": \"$SESSION_ID\", \"messages\": [{\"role\": \"user\", \"content\": \"My name is John\"}]}"

curl -X POST http://127.0.0.1:8080/api/chat \
  -H "Content-Type: application/json" \
  -d "{\"session_id\": \"$SESSION_ID\", \"messages\": [{\"role\": \"user\", \"content\": \"What is my name?\"}]}"

# Optional SSE (Accept or body.stream=true; currently chunked after the tool loop, Brokerrouter true streaming later)
curl -N -X POST http://127.0.0.1:8080/api/chat \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{"messages": [{"role": "user", "content": "Hello"}]}'

curl -N -X POST http://127.0.0.1:8080/api/chat \
  -H "Content-Type: application/json" \
  -d '{"stream": true, "messages": [{"role": "user", "content": "Hello"}]}'

# Read session history (404 if missing)
curl http://127.0.0.1:8080/api/sessions/$SESSION_ID

# Export session JSONL (one message per line; optional ?format=json for {id, messages})
curl -D - http://127.0.0.1:8080/api/sessions/$SESSION_ID/export
curl http://127.0.0.1:8080/api/sessions/$SESSION_ID/export?format=json

# Import session JSONL (optional ?id=; existing ids return 409 unless ?overwrite=true)
curl -X POST http://127.0.0.1:8080/api/sessions/import?id=$SESSION_ID \
  -H "Content-Type: application/x-ndjson" \
  --data-binary @session.jsonl
curl -X POST "http://127.0.0.1:8080/api/sessions/import?format=json&overwrite=true" \
  -H "Content-Type: application/json" \
  -d "{\"id\": \"$SESSION_ID\", \"messages\": [{\"role\": \"user\", \"content\": \"restore\"}]}"

# Delete session
curl -X DELETE http://127.0.0.1:8080/api/sessions/$SESSION_ID

# Webhook inbound (no auth)
curl -X POST http://127.0.0.1:8080/hooks/inbound \
  -H "Content-Type: application/json" \
  -d '{"channel": "webhook", "chat_id": "user123", "text": "Hello via webhook"}'

# Webhook inbound (with auth)
export JIACLAW_WEBHOOK_SECRET=my_secret_key
cargo run --bin jiaclaw -- serve --bind 127.0.0.1:8080

curl -X POST http://127.0.0.1:8080/hooks/inbound \
  -H "Content-Type: application/json" \
  -H "X-Webhook-Secret: my_secret_key" \
  -d '{"channel": "webhook", "chat_id": "user456", "text": "Authenticated message", "username": "alice"}'

# Telegram Bot inbound (sample Update; sync assistant text for long-poll debugging)
curl -X POST http://127.0.0.1:8080/hooks/telegram \
  -H "Content-Type: application/json" \
  -d '{"update_id": 1, "message": {"message_id": 10, "chat": {"id": 4242, "type": "private"}, "text": "Hello Telegram"}}'

# Telegram inbound with official secret token header
export JIACLAW_TELEGRAM_SECRET=my_tg_secret
curl -X POST http://127.0.0.1:8080/hooks/telegram \
  -H "Content-Type: application/json" \
  -H "X-Telegram-Bot-Api-Secret-Token: my_tg_secret" \
  -d '{"update_id": 2, "edited_message": {"message_id": 11, "chat": {"id": 4242}, "text": "Edited message"}}'

# With a Bot token, a successful reply is also pushed back with Telegram sendMessage
export JIACLAW_TELEGRAM_BOT_TOKEN=123456:ABC-your-bot-token

# Register the public HTTPS URL with Telegram setWebhook (secret_token maps to JIACLAW_TELEGRAM_SECRET)
curl "https://api.telegram.org/bot${JIACLAW_TELEGRAM_BOT_TOKEN}/setWebhook" \
  -d "url=https://your-host.example/hooks/telegram" \
  -d "secret_token=${JIACLAW_TELEGRAM_SECRET}"
# Inspect webhook status: curl "https://api.telegram.org/bot${JIACLAW_TELEGRAM_BOT_TOKEN}/getWebhookInfo"

# Slack Events API URL verification (Request URL → https://your-host.example/hooks/slack)
curl -X POST http://127.0.0.1:8080/hooks/slack \
  -H "Content-Type: application/json" \
  -d '{"type":"url_verification","challenge":"3eZbrw1aBm2rZgRNFdxV2595E9CY3gmdALWMmHkvFXO7tYXAYM8P"}'

# Slack inbound (plain message; session slack:{team_id}:{channel})
curl -X POST http://127.0.0.1:8080/hooks/slack \
  -H "Content-Type: application/json" \
  -d '{"type":"event_callback","team_id":"T123","event":{"type":"message","channel":"C456","user":"U789","text":"Hello Slack"}}'

# With a signing secret, Slack sends official signature headers (enable in production; unset stays open)
export JIACLAW_SLACK_SIGNING_SECRET=your-slack-signing-secret
# Slack sends X-Slack-Request-Timestamp and X-Slack-Signature: v0=<hmac-sha256-hex>

# With a Bot token, a successful reply is also pushed back with chat.postMessage
export JIACLAW_SLACK_BOT_TOKEN=xoxb-your-bot-token

# Discord Interactions Ping (Interactions Endpoint URL → https://your-host.example/hooks/discord)
curl -X POST http://127.0.0.1:8080/hooks/discord \
  -H "Content-Type: application/json" \
  -d '{"type":1}'

# Discord Chat Input Command (immediate {type:5} deferred; session discord:{guild}:{channel})
curl -X POST http://127.0.0.1:8080/hooks/discord \
  -H "Content-Type: application/json" \
  -d '{"type":2,"application_id":"APP","guild_id":"G123","channel_id":"C456","token":"interaction-token","data":{"name":"ask","type":1,"options":[{"name":"prompt","type":3,"value":"Hello Discord"}]}}'

# With a public key, Discord sends official signature headers (enable in production; unset stays open)
export JIACLAW_DISCORD_PUBLIC_KEY=your-discord-hex-public-key
# Discord sends X-Signature-Timestamp and X-Signature-Ed25519 (hex)

# With a Bot token, the deferred reply is PATCHed onto the original Interaction
export JIACLAW_DISCORD_BOT_TOKEN=your-discord-bot-token
```

**Note**:
- Brokerrouter requires a valid virtual key (`brk_live_...`)
- Falls back to stub mode without API key (demo functionality)
- StateKnot persistence features not yet integrated
- Optional HTTP rate limiting: set `JIACLAW_RATE_LIMIT_PER_MINUTE` or `[http] rate_limit_per_minute`; `/api/*`, `/hooks/inbound`, `/hooks/telegram`, `/hooks/slack`, and `/hooks/discord` send `X-RateLimit-Limit` / `X-RateLimit-Remaining` / `X-RateLimit-Reset` (Unix epoch seconds when remaining returns to Limit); over-limit returns `429` + `Retry-After` (seconds). Headers are omitted when disabled. `GET /health` and `GET /metrics` are never limited
- Optional HTTP max request body: `[http] max_body_bytes` defaults to **1048576 (1MiB)**; `JIACLAW_MAX_BODY_BYTES` (positive integer) wins (`0`/invalid falls back). Over-limit returns `413` + `{"error":"payload_too_large"}` and still writes `X-Request-Id`. `GET /health` and `GET /metrics` skip the check. No upload / multipart storage
- Optional CORS: off by default (no CORS headers). `[http.cors] enabled = true` or `JIACLAW_CORS_ENABLED=1` echoes `Access-Control-Allow-Origin` for matching origins; `JIACLAW_CORS_ORIGINS` (comma-separated) overrides `allowed_origins`. `*` allows all only when configured explicitly. OPTIONS preflight does not require an API Bearer and still writes `X-Request-Id`. Unmatched origins are never echoed
- Optional Prometheus metrics: `GET /metrics` is public by default; `[http] metrics_public = false` or `JIACLAW_METRICS_REQUIRE_AUTH=1` uses the same auth as `/api/*`. In-process counters (HTTP route family, session gauge, tool calls, build_info); no telemetry SDK
- Optional JSON structured logs: default `[logging] format = "text"` matches the current human-readable tracing fmt. `format = "json"` or `JIACLAW_LOG_FORMAT=json` emits one JSON object per line (`timestamp` / `level` / `target` / `fields` / `message`; `request_id` is a field). `JIACLAW_LOG_LEVEL` overrides `RUST_LOG`. doctor / serve startup prints the effective format and never prints secrets
- Optional Session TTL: set `JIACLAW_SESSION_TTL_SECS` or `[http] session_ttl_secs` (positive integer); idle sessions are removed from the store; `GET /api/sessions` omits expired ids; GET/export of an expired id returns 404
- Graceful serve shutdown: `jiaclaw serve` handles SIGINT/SIGTERM; stops accepting and drains in-flight requests. `[http] shutdown_timeout_secs` defaults to 15s; `JIACLAW_SHUTDOWN_TIMEOUT_SECS` wins (positive integer; invalid falls back to default). Persist flush on shutdown; heartbeat tasks are aborted
- Skill hot-reload: `POST /api/skills/reload` rescans `skills/` and replaces the in-process table; failures keep the old table. Unix `SIGHUP` uses the same path; Windows is HTTP-only. `jiaclaw skills reload` scans this process only and does not notify serve
- Optional session summary compression: set `[session] summarize_on_overflow = true` or `JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1` to fold older messages into one `[session-summary]` system message while keeping the latest `keep_recent` (default 10); unset keeps hard truncation. Summary failure warns and falls back; chat still succeeds
- Optional tool timeout: set `JIACLAW_TOOL_TIMEOUT_SECS` or `[agent] tool_timeout_secs` (positive integer); a tool that exceeds the limit writes `Tool timed out after Ns` into the tool result and the loop continues; unset means unlimited
- Configurable tool-loop cap: set `JIACLAW_MAX_TOOL_ITERATIONS` or `[agent] max_tool_iterations` (default 5); positive integers apply, `0`/invalid is ignored, clamped to 1–32. Hitting the cap writes a tool/assistant hint and ends the turn
- Request tracing: every response writes `X-Request-Id`; a UUID is generated when the request omits it. chat/webhook logs include the id
- OpenAPI sketch: `GET /api/openapi.json` (auth matches `GET /api/tools`)
- Optional SSE: `POST /api/chat` returns `text/event-stream` when `Accept: text/event-stream` or `"stream": true` (`meta` / `token` / `tool` / `done` / `error`). JSON is unchanged when streaming is not requested. **Currently chunked after the full loop; Brokerrouter true streaming comes later**
- Session query: `GET /api/sessions` lists `{id, message_count}`; `GET /api/sessions/:id` returns messages; `GET /api/sessions/:id/export` defaults to JSONL (`?format=json` for the full object); 404 if missing or expired. Export is read-only (no summary, no store rewrite). `POST /api/sessions/import` imports JSONL/JSON (409 if the id exists, `?overwrite=true` replaces; does not call the LLM). CLI: `jiaclaw session export <id> [-o file]`; `jiaclaw session import <file> [--id ID] [--overwrite]`
- Telegram Bot inbound: `POST /hooks/telegram` parses Bot API Updates (`message.text` / `edited_message.text`) into session `telegram:{chat.id}`; updates without text return 200 + a skip reason. Optional `JIACLAW_TELEGRAM_SECRET`. With `JIACLAW_TELEGRAM_BOT_TOKEN`, replies are also sent via `sendMessage` (text truncated at 4096); outbound failure still returns 200 + the original `reply` so Telegram does not retry. Point `setWebhook` at the public `https://…/hooks/telegram` URL, optionally with `secret_token`
- Slack Events API inbound: `POST /hooks/slack` handles `url_verification` (echo `{ challenge }`) and `event_callback` (plain `message` with empty `subtype` only); session key `slack:{team_id}:{channel}` (or `slack:{channel}` if team is missing). Optional `JIACLAW_SLACK_SIGNING_SECRET` (official v0 HMAC-SHA256 over the raw body). With `JIACLAW_SLACK_BOT_TOKEN`, replies are also sent via `chat.postMessage`; outbound failure still returns 200 + the original `reply`. Point the Slack app Event Subscriptions Request URL at the public `https://…/hooks/slack`
- Discord Interactions inbound: `POST /hooks/discord` handles PING (`type=1` → `{ type: 1 }`) and Chat Input Commands (`type=2`); session key `discord:{guild_id}:{channel_id}` (or `discord:dm:{channel_id}` if guild is missing). Immediately `{ type: 5 }` deferred ACK, then background chat. Optional `JIACLAW_DISCORD_PUBLIC_KEY` (official Ed25519 over the raw body). With `JIACLAW_DISCORD_BOT_TOKEN`, the original message is PATCHed; without a token the session is still recorded. Production long-running work must use deferred (3s ACK). Point the Discord app Interactions Endpoint URL at the public `https://…/hooks/discord`
- Optional HEARTBEAT.md: with `[heartbeat] enabled = true`, only `jiaclaw serve` reads the file on an interval and runs one chat turn (fixed session, default `heartbeat`). `JIACLAW_HEARTBEAT_INTERVAL_SECS` overrides the interval. Missing/empty files skip the tick; CLI `chat` does not run heartbeats
- Optional web_search: registered by default. Set `JIACLAW_BRAVE_API_KEY` or `[tools.web_search] brave_api_key` to call Brave Search; without a key the tool returns a friendly error. `enabled = false` skips registration. Doctor never prints the key
- Optional web_fetch: registered by default. GET `http`/`https` URLs and convert HTML to readable text; localhost/private ranges are blocked unless `[tools.web_fetch] allow_private = true`. `enabled = false` skips registration
- Optional memory_search: registered by default. Keyword/line-window search over MEMORY / SOUL / USER (or safe relative paths); `enabled = false` skips registration. No vector database
- Optional memory_write: registered by default. Writes only the configured MEMORY.md; `mode=append|overwrite` (default append); oversize >32KiB is rejected; `enabled = false` skips registration. No vector DB / remote sync
- Optional read_file: registered by default. Reads a workspace-relative text file; `offset`/`limit` are 1-indexed lines; over 256KiB or binary is rejected; no traversal. `enabled = false` skips registration. No shell
- Optional list_dir: registered by default. Lists a workspace directory (default `.`, non-recursive); returns name/type/size; no traversal. `enabled = false` skips registration. No shell
- Optional write_file: registered by default. Writes a workspace-relative regular file; `mode=overwrite|append` (default overwrite); over 256KiB is rejected and not written; atomic write; no traversal. `enabled = false` skips registration. No shell / exec
- Optional delete_file: registered by default. Deletes a workspace-relative regular file; directories are rejected; missing files return an explicit error; no traversal / symlink escape. `enabled = false` skips registration. No recursion / shell / exec
- Optional str_replace: registered by default. Exact in-file string replace; `replace_all` defaults to false (must match exactly once); over 256KiB is rejected and not written; atomic write; no traversal. `enabled = false` skips registration. No shell / exec
- Optional grep: registered by default. Workspace literal text search (not regex); `path` defaults to `.`; optional `glob` / `case_insensitive` / `max_matches` (default 50); no traversal. `enabled = false` skips registration. No shell / ripgrep / exec

### Documentation

- [Architecture Overview](docs/architecture.md) - System design and components
- [StateKnot Capability Gaps](docs/stateknot-gaps.md) - Current limitations and tracked upstream issues
- [Roadmap](docs/roadmap.md) - Development plan and milestones

### License

This project is dual-licensed:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

You may choose either license at your option.
