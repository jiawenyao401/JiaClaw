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
- ✅ **可选 HTTP 限流** - 进程内全局限流保护 `/api/*`、`/hooks/inbound`、`/hooks/telegram` 与 `/hooks/slack`
  - 配置 `rate_limit_per_minute` 或环境变量 `JIACLAW_RATE_LIMIT_PER_MINUTE`
  - 超限返回 429 + `Retry-After`；`GET /health` 始终不限流
- ✅ **可选 Session TTL** - 闲置超时自动清理内存会话（长时间 `serve` 防堆积）
  - 配置 `session_ttl_secs` 或环境变量 `JIACLAW_SESSION_TTL_SECS`（正整数才启用；`0`/非法=关闭）
  - create/chat/get/list 触达刷新；过期后 list 不返回，GET/DELETE 与不存在一致（404）
- ✅ **可选 Session 摘要压缩** - 接近消息条数上限时把旧消息折叠成一条摘要，避免硬截断丢上下文
  - 配置 `[session] summarize_on_overflow`（默认 `false`，保持现有丢弃最旧消息行为）或环境变量 `JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1/true` 强制开启
  - `keep_recent` 默认 10；复用当前 LLM provider，固定中英 prompt，无工具且限制 `max_tokens`；失败 warn 并回退硬截断，不让 chat 失败
  - HTTP / Telegram / Slack / webhook / heartbeat 走同一 session store 写入点
- ✅ **可选工具超时** - 单次 `shell_exec` / `http_get` 等不会无限卡住 tool loop
  - 配置 `[agent] tool_timeout_secs` 或环境变量 `JIACLAW_TOOL_TIMEOUT_SECS`（正整数才启用；`0`/非法=关闭）
  - 超时把 `Tool timed out after Ns` 写入 tool result，不 panic，继续循环
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
- ✅ **OpenAPI 草图** - `GET /api/openapi.json`（鉴权与 `/api/tools` 一致）
- ✅ **可选 SSE 流式** - `POST /api/chat` 在 `Accept: text/event-stream` 或 body `stream: true` 时返回事件流
  - 事件：`meta`（session_id / request_id）、`token`（文本增量）、`tool`（name + ok/error）、`done`（最终 reply）、`error`
  - **当前为整轮 tool loop 完成后的分块推送；Brokerrouter 真流式后续**
  - 未请求流式时 JSON 响应完全不变；鉴权失败仍为 JSON 401
- ✅ **Session 查询 API** - `GET /api/sessions` 列表、`GET /api/sessions/:id` 读取历史（不存在 404）
- ✅ **工作区 MEMORY.md** - 跨会话长期记忆注入系统提示
  - 默认 `{workspace}/MEMORY.md`，可用 `[memory] path` 覆盖
  - 每次 `chat` 重读；过大截断（32KiB）并 warn
  - 本地工具 `memory_append` 追加/覆盖；`jiaclaw doctor` / `jiaclaw memory show`
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

# CORS 允许的来源列表（空或 ["*"] 表示允许所有来源）
cors_allow_origins = ["*"]
# 或限制特定来源：
# cors_allow_origins = ["https://example.com"]

# Session 持久化配置（可选）
persist = true  # 启用 session 持久化
persist_path = ".jiaclaw/sessions.json"

# HTTP 限流（可选，环境变量 JIACLAW_RATE_LIMIT_PER_MINUTE 优先）
# 正整数：对 /api/* 与 /hooks/inbound、/hooks/telegram、/hooks/slack 做进程内全局限流（次/分钟）
# 未设置或 0：不限流。GET /health 始终不限流；超限返回 429 + Retry-After。
# rate_limit_per_minute = 60

# Session 闲置 TTL（可选，环境变量 JIACLAW_SESSION_TTL_SECS 优先）
# 正整数：闲置超过该秒数后从内存 store 删除（落盘开启时同步 save）
# 未设置或 0：不启用。create/chat/get/list 触达会刷新。
# session_ttl_secs = 3600

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
```

或使用环境变量：

```bash
export JIACLAW_API_KEY=brk_live_your_key_here
# 可选：HTTP 限流（次/分钟，优先于配置文件）
# export JIACLAW_RATE_LIMIT_PER_MINUTE=60
# 可选：Session 闲置 TTL（秒，优先于配置文件）
# export JIACLAW_SESSION_TTL_SECS=3600
# 可选：接近消息上限时摘要压缩（1/true 强制开启，优先于 [session] summarize_on_overflow）
# export JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1
# 可选：单次工具调用超时（秒，优先于配置文件）
# export JIACLAW_TOOL_TIMEOUT_SECS=30
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

# 列出已发现的技能
cargo run --bin jiaclaw -- skills
cargo run --bin jiaclaw -- skills --verbose  # 显示详细信息

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

# OpenAPI 3 草图（未配置 API token 时可匿名访问；已启用 token 时需 Bearer，与 /api/tools 一致）
curl http://127.0.0.1:8080/api/openapi.json

# 列出已注册的工具
curl http://127.0.0.1:8080/api/tools

# 列出已发现的技能
curl http://127.0.0.1:8080/api/skills

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
```

**注意**：
- 使用 Brokerrouter 需要有效的虚拟密钥（`brk_live_...`）
- 无 API key 时自动回退到存根模式（演示功能）
- StateKnot 持久化功能尚未集成
- 可选 HTTP 限流：设置 `JIACLAW_RATE_LIMIT_PER_MINUTE` 或 `[http] rate_limit_per_minute` 后，`/api/*` 与 `/hooks/inbound`、`/hooks/telegram`、`/hooks/slack` 超限返回 `429` + `Retry-After`；`GET /health` 不限流
- 可选 Session TTL：设置 `JIACLAW_SESSION_TTL_SECS` 或 `[http] session_ttl_secs`（正整数）后，闲置超时的会话会从 store 删除；`GET /api/sessions` 只返回未过期项，过期 id 的 GET 为 404
- 可选 Session 摘要压缩：设置 `[session] summarize_on_overflow = true` 或 `JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1` 后，接近上限时把旧消息折叠为一条 `[session-summary]` system 消息并保留最近 `keep_recent`（默认 10）条；未开启则仍硬截断。摘要失败会 warn 并回退截断，chat 不失败
- 可选工具超时：设置 `JIACLAW_TOOL_TIMEOUT_SECS` 或 `[agent] tool_timeout_secs`（正整数）后，单次 tool 超过该秒数会把 `Tool timed out after Ns` 写入 tool result 并继续循环；未设置则不限制
- 请求追踪：所有响应回写 `X-Request-Id`；请求未携带时服务端生成 UUID。chat/webhook 日志带上该 ID
- OpenAPI 草图：`GET /api/openapi.json`（鉴权与 `GET /api/tools` 一致）
- 可选 SSE：`POST /api/chat` 在 `Accept: text/event-stream` 或 `"stream": true` 时返回 `text/event-stream`（`meta` / `token` / `tool` / `done` / `error`）。未请求流式时 JSON 不变。**当前为分块推送；Brokerrouter 真流式后续**
- Session 查询：`GET /api/sessions` 列出 `{id, message_count}`；`GET /api/sessions/:id` 返回消息；不存在 404。读接口反映内存当前状态（落盘开启时与 store 一致）
- Telegram Bot 入站：`POST /hooks/telegram` 解析 Bot API Update（`message.text` / `edited_message.text`），会话键 `telegram:{chat.id}`；无文本返回 200 + 跳过说明。可选 `JIACLAW_TELEGRAM_SECRET`。配置 `JIACLAW_TELEGRAM_BOT_TOKEN` 后会调用 `sendMessage` 出站（文本超 4096 截断）；出站失败仍返回 200 + 原 `reply`，避免 Telegram 重试。用 `setWebhook` 把公网 `https://…/hooks/telegram` 登记到 Bot，并可带 `secret_token`
- Slack Events API 入站：`POST /hooks/slack` 处理 `url_verification`（回传 `{ challenge }`）与 `event_callback`（仅 `message` 且 `subtype` 为空）；会话键 `slack:{team_id}:{channel}`（无 team 则为 `slack:{channel}`）。可选 `JIACLAW_SLACK_SIGNING_SECRET`（官方 v0 HMAC-SHA256，先取 raw body）。配置 `JIACLAW_SLACK_BOT_TOKEN` 后会调用 `chat.postMessage` 出站；出站失败仍 200 + 原 `reply`。在 Slack 应用的 Event Subscriptions 把 Request URL 指到公网 `https://…/hooks/slack`
- 可选 HEARTBEAT.md：`[heartbeat] enabled = true` 后仅 `jiaclaw serve` 按间隔读取约定文件全文并跑一轮 chat（固定 session，默认 `heartbeat`）。`JIACLAW_HEARTBEAT_INTERVAL_SECS` 可覆盖间隔。文件缺失/空则跳过；CLI `chat` 不跑心跳
- 可选 web_search：默认注册。设置 `JIACLAW_BRAVE_API_KEY` 或 `[tools.web_search] brave_api_key` 后调用 Brave Search；未配置 key 时返回友好错误。`enabled = false` 不注册。doctor 不打印 key
- 可选 web_fetch：默认注册。GET `http`/`https` URL，HTML 转为可读文本；默认拒绝 localhost/私网（`[tools.web_fetch] allow_private = true` 可放开）。`enabled = false` 不注册

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
- ✅ **Optional HTTP rate limiting** - process-wide limit for `/api/*`, `/hooks/inbound`, `/hooks/telegram`, and `/hooks/slack`
  - Configure `rate_limit_per_minute` or `JIACLAW_RATE_LIMIT_PER_MINUTE`
  - Over-limit returns 429 + `Retry-After`; `GET /health` is never limited
- ✅ **Optional Session TTL** - idle sessions are expired to avoid unbounded memory growth during long `serve`
  - Configure `session_ttl_secs` or `JIACLAW_SESSION_TTL_SECS` (positive integer enables; `0`/invalid disables)
  - create/chat/get/list refresh last access; expired ids are omitted from list and GET/DELETE match not-found (404)
- ✅ **Optional session summary compression** - fold older messages into one summary near the session cap instead of dropping them
  - Configure `[session] summarize_on_overflow` (default `false`, keeps hard truncation) or `JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1/true` to force-enable
  - `keep_recent` defaults to 10; reuses the current LLM provider with a fixed bilingual prompt, no tools, and a small `max_tokens`; on failure, warn and fall back to truncation without failing chat
  - HTTP / Telegram / Slack / webhook / heartbeat share the same session-store write path
- ✅ **Optional tool timeout** - a long `shell_exec` / `http_get` cannot stall the whole tool loop
  - Configure `[agent] tool_timeout_secs` or `JIACLAW_TOOL_TIMEOUT_SECS` (positive integer enables; `0`/invalid disables)
  - Timeout writes `Tool timed out after Ns` into the tool result, does not panic, and continues the loop
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
- ✅ **OpenAPI sketch** - `GET /api/openapi.json` (auth matches `/api/tools`)
- ✅ **Optional SSE** - `POST /api/chat` returns `text/event-stream` when `Accept: text/event-stream` or body `stream: true`
  - Events: `meta` (session_id / request_id), `token` (text chunks), `tool` (name + ok/error), `done` (final reply), `error`
  - **Currently chunked after the full tool loop; Brokerrouter true streaming comes later**
  - JSON responses stay unchanged when streaming is not requested; auth failures remain JSON 401
- ✅ **Session query API** - `GET /api/sessions` list, `GET /api/sessions/:id` history (404 if missing)
- ✅ **Workspace MEMORY.md** - cross-session facts injected into the system prompt
  - Default `{workspace}/MEMORY.md`, overridable via `[memory] path`
  - Re-read on every `chat`; truncate at 32KiB with a warning
  - Local tool `memory_append`; `jiaclaw doctor` / `jiaclaw memory show`
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

# CORS allowed origins (empty or ["*"] allows all origins)
cors_allow_origins = ["*"]
# Or restrict to specific origins:
# cors_allow_origins = ["https://example.com"]

# Session persistence config (optional)
persist = true  # Enable session persistence
persist_path = ".jiaclaw/sessions.json"

# Optional HTTP rate limit (JIACLAW_RATE_LIMIT_PER_MINUTE env var takes priority)
# Positive integer: process-wide limit for /api/* and /hooks/inbound, /hooks/telegram, /hooks/slack (requests/minute)
# Unset or 0: disabled. GET /health is never limited; over-limit returns 429 + Retry-After.
# rate_limit_per_minute = 60

# Optional session idle TTL (JIACLAW_SESSION_TTL_SECS env var takes priority)
# Positive integer: expire idle sessions after this many seconds (saved to disk when persist is on)
# Unset or 0: disabled. create/chat/get/list refresh last access.
# session_ttl_secs = 3600

[session]
# When approaching the message cap (50), summarize older turns instead of dropping them (default false)
# JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1/true force-enables this
# summarize_on_overflow = true
# keep_recent = 10

# Per-tool timeout lives on [agent] (JIACLAW_TOOL_TIMEOUT_SECS env var takes priority)
# Positive integer enables; unset or 0 is unlimited (default). Timeout writes into the tool result and the loop continues.
# [agent]
# tool_timeout_secs = 30

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
```

Or use environment variable:

```bash
export JIACLAW_API_KEY=brk_live_your_key_here
# Optional: HTTP rate limit (requests/minute, overrides config file)
# export JIACLAW_RATE_LIMIT_PER_MINUTE=60
# Optional: session idle TTL in seconds (overrides config file)
# export JIACLAW_SESSION_TTL_SECS=3600
# Optional: summarize older session messages near the cap (1/true force-enables)
# export JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1
# Optional: per-tool timeout in seconds (overrides config file)
# export JIACLAW_TOOL_TIMEOUT_SECS=30
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

# Run single chat (requires API key)
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

# OpenAPI 3 sketch (anonymous when no API token; Bearer required when token is enabled, same as /api/tools)
curl http://127.0.0.1:8080/api/openapi.json

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
```

**Note**:
- Brokerrouter requires a valid virtual key (`brk_live_...`)
- Falls back to stub mode without API key (demo functionality)
- StateKnot persistence features not yet integrated
- Optional HTTP rate limiting: set `JIACLAW_RATE_LIMIT_PER_MINUTE` or `[http] rate_limit_per_minute`; `/api/*`, `/hooks/inbound`, `/hooks/telegram`, and `/hooks/slack` return `429` + `Retry-After` when exceeded; `GET /health` is never limited
- Optional Session TTL: set `JIACLAW_SESSION_TTL_SECS` or `[http] session_ttl_secs` (positive integer); idle sessions are removed from the store; `GET /api/sessions` omits expired ids; GET of an expired id returns 404
- Optional session summary compression: set `[session] summarize_on_overflow = true` or `JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1` to fold older messages into one `[session-summary]` system message while keeping the latest `keep_recent` (default 10); unset keeps hard truncation. Summary failure warns and falls back; chat still succeeds
- Optional tool timeout: set `JIACLAW_TOOL_TIMEOUT_SECS` or `[agent] tool_timeout_secs` (positive integer); a tool that exceeds the limit writes `Tool timed out after Ns` into the tool result and the loop continues; unset means unlimited
- Request tracing: every response writes `X-Request-Id`; a UUID is generated when the request omits it. chat/webhook logs include the id
- OpenAPI sketch: `GET /api/openapi.json` (auth matches `GET /api/tools`)
- Optional SSE: `POST /api/chat` returns `text/event-stream` when `Accept: text/event-stream` or `"stream": true` (`meta` / `token` / `tool` / `done` / `error`). JSON is unchanged when streaming is not requested. **Currently chunked after the full loop; Brokerrouter true streaming comes later**
- Session query: `GET /api/sessions` lists `{id, message_count}`; `GET /api/sessions/:id` returns messages (404 if missing). Reads reflect in-memory state (same store when disk persistence is on)
- Telegram Bot inbound: `POST /hooks/telegram` parses Bot API Updates (`message.text` / `edited_message.text`) into session `telegram:{chat.id}`; updates without text return 200 + a skip reason. Optional `JIACLAW_TELEGRAM_SECRET`. With `JIACLAW_TELEGRAM_BOT_TOKEN`, replies are also sent via `sendMessage` (text truncated at 4096); outbound failure still returns 200 + the original `reply` so Telegram does not retry. Point `setWebhook` at the public `https://…/hooks/telegram` URL, optionally with `secret_token`
- Slack Events API inbound: `POST /hooks/slack` handles `url_verification` (echo `{ challenge }`) and `event_callback` (plain `message` with empty `subtype` only); session key `slack:{team_id}:{channel}` (or `slack:{channel}` if team is missing). Optional `JIACLAW_SLACK_SIGNING_SECRET` (official v0 HMAC-SHA256 over the raw body). With `JIACLAW_SLACK_BOT_TOKEN`, replies are also sent via `chat.postMessage`; outbound failure still returns 200 + the original `reply`. Point the Slack app Event Subscriptions Request URL at the public `https://…/hooks/slack`
- Optional HEARTBEAT.md: with `[heartbeat] enabled = true`, only `jiaclaw serve` reads the file on an interval and runs one chat turn (fixed session, default `heartbeat`). `JIACLAW_HEARTBEAT_INTERVAL_SECS` overrides the interval. Missing/empty files skip the tick; CLI `chat` does not run heartbeats
- Optional web_search: registered by default. Set `JIACLAW_BRAVE_API_KEY` or `[tools.web_search] brave_api_key` to call Brave Search; without a key the tool returns a friendly error. `enabled = false` skips registration. Doctor never prints the key
- Optional web_fetch: registered by default. GET `http`/`https` URLs and convert HTML to readable text; localhost/private ranges are blocked unless `[tools.web_fetch] allow_private = true`. `enabled = false` skips registration

### Documentation

- [Architecture Overview](docs/architecture.md) - System design and components
- [StateKnot Capability Gaps](docs/stateknot-gaps.md) - Current limitations and tracked upstream issues
- [Roadmap](docs/roadmap.md) - Development plan and milestones

### License

This project is dual-licensed:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

You may choose either license at your option.
