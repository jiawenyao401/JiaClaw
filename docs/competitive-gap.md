# JiaClaw 竞争差距分析

> 对比分析 JiaClaw 相对于 OpenClaw 和 Hermes Agent 的能力差距、优势和路线图

**更新时间**: 2026-09-20  
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
| **CLI 界面** | ✅ 完整 | ✅ 完整 | ✅ 基础 | P1 | - |
| **HTTP REST API** | ✅ FastAPI | ✅ Flask/FastAPI | ✅ **基础（axum）** | P1 | - |
| **Session（内存）** | ✅ 支持 | ⏳ 部分 | ✅ **已实现** | P1 | - |
| **Session（落盘）** | ✅ 支持 | ⏳ 部分 | ✅ **可选落盘** | P1 | - |
| **工具列表 API** | ✅ 支持 | ⏳ 部分 | ✅ **GET /api/tools** | P1 | - |
| **技能列表 API** | ✅ 支持 | ❌ 无 | ✅ **GET /api/skills** | P1 | - |
| **Webhook 入站** | ✅ 支持 | ⏳ 部分 | ✅ **POST /hooks/inbound** | P1 | - |
| **SSE 事件流** | ✅ 支持 | ⏳ 部分 | ⏳ 计划中 | P1 | [#92](https://github.com/StateKnot/StateKnot/issues/92) AgentServiceV1 |
| **WebSocket** | ⏳ 社区贡献 | ❌ 无 | ⏳ 计划中（M4） | P2 | - |
| **Discord/Slack** | ✅ 插件支持 | ❌ 无 | ⏳ 计划中（通过 MCP） | P2 | [#95](https://github.com/StateKnot/StateKnot/issues/95) MCP 集成 |
| **gRPC** | ❌ 无 | ❌ 无 | ⏳ 可选（通过 StateKnot） | P2 | - |

**JiaClaw 现状**:
- ✅ CLI 已实现（`jiaclaw chat`, `jiaclaw serve`）
- ✅ HTTP 服务已实现（GET /health, POST /api/chat, GET /api/tools, GET /api/skills）
- ✅ Session 内存支持（可选 `session_id` 实现多轮对话历史，自动截断超长历史）
- ✅ **Session 可选落盘**（`[http] persist = true`，进程重启后可恢复历史，原子写入，自动处理损坏文件）
- ✅ 工具列表 API（GET /api/tools 列出已注册工具名称和描述）
- ✅ 技能列表 API（GET /api/skills 列出已发现技能）
- ✅ Webhook 入站 API（POST /hooks/inbound，支持可选鉴权，自动 session 管理）
- ❌ SSE 事件流需要 `DurableAgentRuns` 的事件订阅 API

**目标方案**:
- **P1 HTTP/SSE**: 等待 StateKnot [#92](https://github.com/StateKnot/StateKnot/issues/92) 稳定后实现 `AgentHost` 集成
- **P2 Discord/Slack**: 通过 MCP 协议适配器，复用 StateKnot 的 `McpRemoteTool` 机制

---

### 2. Memory（记忆系统）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **短期记忆** | ✅ 会话历史 | ✅ 上下文窗口 | ✅ 基础（ChatRequest） | P0 | - |
| **长期记忆** | ✅ 向量数据库 (ChromaDB/Pinecone) | ✅ 嵌入检索 | ⏳ 计划中（M3） | P1 | StateKnot 持久化层 |
| **语义搜索** | ✅ 完整 | ✅ 完整 | ⏳ 计划中（M3） | P1 | 通过 MCP 或本地工具 |
| **记忆编辑** | ✅ API 支持 | ⏳ 部分 | ⏳ 计划中（M3） | P2 | - |
| **时间旅行** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | StateKnot Checkpoint |
| **Fork/Branch** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | StateKnot Graph Fork |

**JiaClaw 现状**:
- ✅ 短期记忆（`ChatRequest.messages`）已实现
- ❌ 长期记忆需要向量数据库集成和 StateKnot 持久化配置
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
| **技能定义** | ✅ Python 类 + 装饰器 | ✅ 函数 + 描述 | ⏳ SKILL.md（计划） | P1 | [#96](https://github.com/StateKnot/StateKnot/issues/96) 技能组合 |
| **技能发现** | ✅ 自动扫描 | ⏳ 手动注册 | ⏳ 计划中 | P1 | - |
| **技能激活** | ✅ 动态加载 | ✅ 运行时选择 | ⏳ 计划中（M3） | P1 | - |
| **技能依赖** | ⏳ 部分支持 | ❌ 无 | ⏳ 计划中（M3） | P2 | - |
| **技能市场** | ✅ 社区生态 | ❌ 无 | ⏳ 计划中（M3） | P2 | - |
| **技能隔离** | ❌ 无 | ❌ 无 | ✅ **租户策略** | P1 | StateKnot 租户隔离 |

**JiaClaw 现状**:
- ❌ 技能系统尚未实现，仅有 `ChatRequest.enabled_skills` 字段
- ❌ 需要设计技能清单格式和加载机制

**目标方案**:
- **P1 技能定义**: 采用 `SKILL.md` 格式（类似 Cursor Agent Skills）
  - 每个技能一个目录: `skills/<skill-name>/SKILL.md`
  - 清单包含: 名称、描述、工具列表、提示片段、依赖
- **P1 技能发现**: 运行时扫描 `skills/` 目录
- **P1 技能激活**: 根据用户请求或 Agent 配置动态加载
- **P2 技能隔离**: 利用 StateKnot 的资源策略限制技能的工具访问范围

**竞争优势**:
- 技能隔离和权限管理由 StateKnot 租户策略提供，更安全
- 技能执行自动获得持久化和恢复能力

---

### 4. Tools（工具系统）

#### 4.1 Shell / Filesystem / Browser / Web

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **Shell 命令** | ✅ 完整 | ✅ 完整 | ✅ **已实现** | P1 | [#95](https://github.com/StateKnot/StateKnot/issues/95) 本地工具 |
| **文件读写** | ✅ 完整 | ✅ 完整 | ✅ **已实现** | P1 | [#95](https://github.com/StateKnot/StateKnot/issues/95) 本地工具 |
| **浏览器控制** | ✅ Selenium/Playwright | ⏳ 部分 | ⏳ 计划中（M3） | P2 | MCP Browser Tool |
| **网页抓取** | ✅ BeautifulSoup | ✅ 完整 | ✅ **已实现（HTTP）** | P1 | Rust reqwest + MCP |
| **API 调用** | ✅ requests | ✅ httpx | ✅ **已实现（HTTP）** | P1 | Rust reqwest + MCP |
| **JSON 处理** | ✅ 完整 | ✅ 完整 | ✅ **已实现** | P1 | - |
| **日期时间** | ✅ 完整 | ✅ 完整 | ✅ **已实现** | P1 | - |
| **工具超时** | ✅ 配置化 | ⏳ 简单 | ✅ **StateKnot 原生** | P0 | StateKnot 预算控制 |
| **工具沙箱** | ⏳ Docker | ❌ 无 | ✅ **StateKnot 策略** | P1 | StateKnot 资源策略 |

**JiaClaw 现状**:
- ✅ 基础本地工具已实现（Shell、File、HTTP、JSON、DateTime）
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
- 工具超时和沙箱由 StateKnot 策略引擎控制，更安全

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
| **流式输出** | ✅ 完整 | ✅ 完整 | ⏳ 计划中（M4） | P1 | StateKnot SSE |
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
- **P1 流式输出**: 等待 Brokerrouter SSE 支持 + StateKnot AgentServiceV1

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
| **运维配置** | ⏳ 基础 | ⏳ 基础 | ✅ **HTTP/CORS/Webhook** | P1 | - |
| **开发模式** | ✅ 简单 | ✅ 简单 | ✅ `doctor` 诊断 | P1 | - |
| **Docker 镜像** | ✅ 官方 | ⏳ 社区 | ⏳ 计划中（M4） | P1 | - |
| **文档质量** | ✅ 优秀 | ⏳ 中等 | ✅ 持续改进 | P1 | - |

**JiaClaw 现状**:
- ✅ Cargo 工作空间已配置
- ✅ `jiaclaw init` 命令创建工作空间
- ✅ HTTP 配置支持（bind、webhook_secret、cors_allow_origins）
- ✅ `jiaclaw doctor` 诊断命令（检查配置、工具、技能、HTTP 设置）
- ⏳ 文档持续改进中

**目标方案**:
- **P1 运维配置**: ✅ **已实现**
  - TOML 配置支持 `[http]` 段落
  - 环境变量覆盖（`JIACLAW_WEBHOOK_SECRET` 优先）
  - CORS 来源控制（空或 `["*"]` 保持 permissive，否则限制）
  - 启动日志打印配置摘要（不泄露 secret 明文）
- **P1 doctor 诊断**: ✅ **已实现**
  - 检查 workspace 可读性、工具数量、技能数量
  - HTTP 配置摘要（bind、webhook 鉴权状态、CORS 模式）
  - 明确提示 stub 模式（当 API key 缺失）
- **P1 文档改进**: 添加 Quick Start 和 Tutorial

---

### 8. Security & Sandbox（安全与沙箱）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **租户隔离** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P1 | StateKnot 租户系统 |
| **资源限制** | ⏳ 简单 | ❌ 无 | ✅ **StateKnot 原生** | P1 | 资源策略 |
| **工具白名单** | ✅ 配置化 | ⏳ 手动 | ✅ **StateKnot 原生** | P1 | 资源策略 |
| **API Key 管理** | ⏳ 环境变量 | ⏳ 环境变量 | ✅ **配置 + 策略** | P1 | - |
| **Docker 沙箱** | ✅ 可选 | ❌ 无 | ⏳ 计划中（M4） | P2 | - |
| **网络隔离** | ❌ 无 | ❌ 无 | ✅ **StateKnot 策略** | P2 | 资源策略 |

**JiaClaw 现状**:
- ✅ StateKnot 提供租户隔离和资源策略框架
- ❌ JiaClaw 尚未配置具体策略

**目标方案**:
- **P1 租户隔离**: 在配置中指定 `tenant_id`，隔离不同用户的运行
- **P1 资源限制**: 配置工具超时、最大并发调用、预算上限
- **P2 Docker 沙箱**: 可选地在 Docker 容器中运行工具

**竞争优势**:
- 企业级安全模型（多租户、策略引擎）
- 默认安全（工具需显式授权）

---

### 9. Scheduling & Cron（调度与定时任务）

| 维度 | OpenClaw | Hermes Agent | JiaClaw | 优先级 | StateKnot 关联 |
|------|----------|--------------|---------|--------|---------------|
| **定时触发** | ✅ APScheduler | ❌ 无 | ⏳ 计划中（M5） | P2 | - |
| **事件触发** | ✅ Webhook | ❌ 无 | ⏳ 计划中（M4） | P2 | AgentHost HTTP |
| **任务队列** | ⏳ 简单 | ❌ 无 | ✅ **StateKnot 原生** | P1 | Fair Scheduler |
| **并发控制** | ⏳ 简单 | ❌ 无 | ✅ **StateKnot 原生** | P1 | 调度器策略 |
| **优先级队列** | ❌ 无 | ❌ 无 | ✅ **StateKnot 原生** | P2 | Fair Scheduler |

**JiaClaw 现状**:
- ✅ StateKnot 的 Fair Scheduler 支持任务队列和并发控制
- ❌ 缺少定时触发和事件触发的上层封装

**目标方案**:
- **P2 定时触发**: 添加 cron 配置，定时提交 Agent 运行
- **P2 事件触发**: 通过 AgentHost HTTP API 接收 Webhook

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
| SSE 事件流 | M1 | [#92](https://github.com/StateKnot/StateKnot/issues/92) AgentServiceV1 |
| 技能系统 | M3 | [#96](https://github.com/StateKnot/StateKnot/issues/96) |
| MCP 工具集成 | M2 | [#95](https://github.com/StateKnot/StateKnot/issues/95) |
| 本地工具（Shell/File/HTTP） | M2 | [#95](https://github.com/StateKnot/StateKnot/issues/95) |
| PostgreSQL 持久化 | M1 | [#94](https://github.com/StateKnot/StateKnot/issues/94) |
| 长期记忆（向量数据库） | M3 | - |

### P2 - 中优先级（Medium）
| 功能 | 计划时间 |
|------|---------|
| WebSocket 通道 | M4 |
| Discord/Slack 集成 | M4 |
| 浏览器控制 | M3 |
| Docker 沙箱 | M4 |
| 定时任务 | M5 |
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
