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
```

或使用环境变量：

```bash
export JIACLAW_API_KEY=brk_live_your_key_here
```

### 运行示例

```bash
# 显示版本信息
cargo run --bin jiaclaw -- version

# 初始化工作空间
cargo run --bin jiaclaw -- init

# 运行单次聊天（需要配置 API key）
export JIACLAW_API_KEY=brk_live_...
cargo run --bin jiaclaw -- chat "你好，JiaClaw"

# 或使用配置文件
cargo run --bin jiaclaw -- chat --config config/jiaclaw.toml "你好，JiaClaw"

# 启动 HTTP 服务
cargo run --bin jiaclaw -- serve --bind 127.0.0.1:8080

# 测试 HTTP API
curl http://127.0.0.1:8080/health

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
```

**注意**：
- 使用 Brokerrouter 需要有效的虚拟密钥（`brk_live_...`）
- 无 API key 时自动回退到存根模式（演示功能）
- StateKnot 持久化功能尚未集成

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
```

Or use environment variable:

```bash
export JIACLAW_API_KEY=brk_live_your_key_here
```

#### Run Examples

```bash
# Show version info
cargo run --bin jiaclaw -- version

# Initialize workspace
cargo run --bin jiaclaw -- init

# Run single chat (requires API key)
export JIACLAW_API_KEY=brk_live_...
cargo run --bin jiaclaw -- chat "Hello, JiaClaw"

# Or use config file
cargo run --bin jiaclaw -- chat --config config/jiaclaw.toml "Hello, JiaClaw"

# Start HTTP service
cargo run --bin jiaclaw -- serve --bind 127.0.0.1:8080

# Test HTTP API
curl http://127.0.0.1:8080/health

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
```

**Note**:
- Brokerrouter requires a valid virtual key (`brk_live_...`)
- Falls back to stub mode without API key (demo functionality)
- StateKnot persistence features not yet integrated

### Documentation

- [Architecture Overview](docs/architecture.md) - System design and components
- [StateKnot Capability Gaps](docs/stateknot-gaps.md) - Current limitations and tracked upstream issues
- [Roadmap](docs/roadmap.md) - Development plan and milestones

### License

This project is dual-licensed:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

You may choose either license at your option.
