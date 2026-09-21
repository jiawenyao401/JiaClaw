# JiaClaw 架构

本文档描述 JiaClaw 的架构设计，以及它如何构建在 StateKnot 之上。

## 系统概览

```
┌─────────────────────────────────────────────────────────────┐
│                        JiaClaw 层                            │
├─────────────────────────────────────────────────────────────┤
│  用户界面层                                                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │   CLI    │  │   HTTP   │  │   SSE    │                  │
│  │  (Chat)  │  │  (REST)  │  │ (Events) │                  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘                  │
│       └─────────────┼─────────────┘                         │
│                     │                                        │
│  应用逻辑层          │                                        │
│  ┌──────────────────▼────────────────────┐                 │
│  │      JiaClawAgent (jiaclaw crate)     │                 │
│  │  - 聊天协调                            │                 │
│  │  - 工具管理                            │                 │
│  │  - 技能编排                            │                 │
│  │  - 会话状态                            │                 │
│  └──────────────────┬────────────────────┘                 │
│                     │                                        │
│  提供商适配层        │                                        │
│  ┌──────────────────▼────────────────────┐                 │
│  │    Provider Adapter (provider.rs)     │                 │
│  │  - StubProvider (离线模式)            │                 │
│  │  - BrokerrouterProvider (生产模式)    │                 │
│  │  - DirectProvider (临时，待废弃)      │                 │
│  └──────────────────┬────────────────────┘                 │
│                     │                                        │
│  领域模型层          │                                        │
│  ┌──────────────────▼────────────────────┐                 │
│  │    Core Types (jiaclaw-core crate)    │                 │
│  │  - ChatRequest / ChatResponse          │                 │
│  │  - ChatMessage / MessageRole           │                 │
│  │  - ToolCall / RunStatus                │                 │
│  │  - AgentConfig / ProviderConfig        │                 │
│  │  - SessionConfig（可选溢出摘要）        │                 │
│  └──────────────────┬────────────────────┘                 │
└────────────────────┬┬────────────────────────────────────┬─┘
                     ││                                    │
                     ││  集成边界                          │
                     ││                                    │
┌────────────────────▼▼────────────────────────────────────▼─┐
│                   Brokerrouter Gateway                      │
│              (AI Model Routing Layer)                       │
├─────────────────────────────────────────────────────────────┤
│  - OpenAI-compatible API (/v1/chat/completions)            │
│  - 多提供商路由（OpenAI, Anthropic, Ollama, etc.）         │
│  - 认证与授权                                               │
│  - 错误处理与重试                                           │
│  - 可观测性（日志、指标）                                   │
└────────────────────┬────────────────────────────────────────┘
                     │
                     │  上游提供商
                     │
         ┌───────────┼───────────┐
         ▼           ▼           ▼
    ┌────────┐  ┌────────┐  ┌────────┐
    │ OpenAI │  │Anthropic│  │ Ollama │
    └────────┘  └────────┘  └────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│                    StateKnot 框架                           │
├─────────────────────────────────────────────────────────────┤
│  Runtime 层                                                 │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  AgentBuilder<I,O> / TypedAgent<I,O>                 │  │
│  │  DurableAgentAdmission / DurableAgentRuns            │  │
│  │  ProviderNativeAgentGraph                            │  │
│  │  AgentServiceV1 / AgentHost                          │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  Execution 层                                               │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  DurableInvocationExecutor                           │  │
│  │  DurableAgentLoop                                    │  │
│  │  Graph Driver (checkpoints, barriers)                │  │
│  │  Fair Scheduler (tenant-aware)                       │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  Integration 层                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Model Adapters: (与 Brokerrouter 协同)              │  │
│  │  Protocol Adapters: MCP, A2A                         │  │
│  │  McpRemoteTool / A2aRemoteAgent                      │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  Persistence 层                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  PostgreSQL Store (runs, events, checkpoints)        │  │
│  │  Artifact Store (S3-compatible)                      │  │
│  │  Journal / Ledger (invocation records)               │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## 组件说明

### JiaClaw 层

#### 1. `jiaclaw-core` - 核心领域类型

定义 JiaClaw 的核心业务概念：

- **ChatMessage / ChatRequest / ChatResponse** - 聊天交互契约
- **ToolCall** - 工具调用记录
- **RunStatus** - 运行状态枚举
- **AgentConfig** - Agent 配置

这些类型是 JiaClaw 特定的，独立于 StateKnot 的实现细节。

#### 2. `jiaclaw` - StateKnot 集成层

提供 StateKnot 之上的应用层抽象：

- **JiaClawAgent** - 主要的 Agent 包装器
  - 封装 StateKnot 的 `TypedAgent<ChatRequest, ChatResponse>`
  - 管理聊天会话和对话历史
  - 协调工具调用和技能执行
  - 处理持久化和恢复逻辑

- **Provider 适配器** - 模型提供商抽象
  - `StubProvider` - 离线存根模式（无需外部服务）
  - `BrokerrouterProvider` - 生产模式（通过 Brokerrouter Gateway）
  - `DirectProvider` - 临时直连模式（待废弃）
  
- **Workspace** - 工作空间管理
  - 加载和管理 `AGENTS.md`, `SOUL.md`, `USER.md`, `MEMORY.md`；可选 `HEARTBEAT.md` 供 serve 定时自检
  - 每次对话重读 `SOUL.md` / `USER.md` / `MEMORY.md`（或 `[identity]` / `[memory]` 路径）并注入系统提示；各文件独立过大截断
  - 工具 `soul_write` / `user_write` / `memory_append` 只能写约定路径（禁止穿越）
  - 工具 `memory_search` 在 MEMORY / SOUL / USER（或安全相对路径）中按关键词检索行窗片段

#### 3. `jiaclaw-host` - 可执行宿主

提供运行时和用户界面：

- **CLI 接口**
  - `jiaclaw serve` - 启动 HTTP 服务
  - `jiaclaw chat <message>` - 单次聊天
  - `jiaclaw doctor` - 配置诊断（含 MEMORY / SOUL / USER / HEARTBEAT 是否存在及大小；Heartbeat 是否启用与间隔；工具循环上限生效值；web_search / web_fetch / memory_search 是否启用）
  - `jiaclaw memory show` - 显示工作区长期记忆
  - `jiaclaw soul show` / `jiaclaw user show` - 显示人格与用户画像
  - `jiaclaw version` - 版本信息

- **HTTP 服务**（计划）
  - REST API 端点
  - SSE 事件流
  - 身份验证和授权

### StateKnot 框架层

JiaClaw 依赖 StateKnot 的以下能力：

#### Runtime 层

- **AgentBuilder** - 构建类型化 Agent 定义
- **TypedAgent<I, O>** - 类型安全的 Agent 执行器
- **DurableAgentAdmission** - 原子化的持久化准入
- **DurableAgentRuns** - 运行状态和结果管理
- **AgentHost** - 协调 HTTP、Worker 和维护角色

#### Execution 层

- **Graph Driver** - 确定性图执行
- **DurableInvocationExecutor** - 持久化调用（模型/工具）
- **Fair Scheduler** - 跨租户公平调度
- **Checkpoint/Barrier** - 状态检查点和同步屏障

#### Integration 层

- **Model Adapters** - OpenAI、Anthropic 等模型提供者
- **MCP Support** - 工具协议（Model Context Protocol）
- **A2A Support** - Agent-to-Agent 协议

#### Persistence 层

- **PostgreSQL Store** - 运行日志、事件、检查点
- **Artifact Store** - S3 兼容的对象存储
- **Journal/Ledger** - 不可变调用记录

## 数据流

### 1. 聊天请求流程

```
用户输入
  │
  ▼
jiaclaw-host (CLI/HTTP)
  │
  ▼
JiaClawAgent.chat(request)
  │
  ├─> 1. 序列化 ChatRequest
  │   TypedAgent::prepare_request()
  │
  ├─> 2. 持久化准入
  │   DurableAgentAdmission::admit()
  │   - 分配 run/thread/invocation IDs
  │   - 提交初始状态和检查点
  │   - 进入调度器队列
  │
  ├─> 3. 图执行
  │   DurableAgentLoop 循环：
  │   - 恢复检查点
  │   - 执行模型调用（DurableInvocationExecutor）
  │   - 处理工具提议
  │   - 执行工具（MCP/A2A）
  │   - 提交检查点
  │   - 检查终止条件
  │
  ├─> 4. 终止和结果
  │   - AgentResult::Success/Failure
  │   - 验证输出 schema
  │
  └─> 5. 响应反序列化
      TypedAgent::decode_result()
      - 验证来源和预算证据
      - 反序列化 ChatResponse
  │
  ▼
返回 ChatResponse 给用户
```

### 2. 持久化和恢复

```
运行时崩溃或重启
  │
  ▼
AgentHost 启动
  │
  ├─> PostgreSQL 恢复
  │   - 加载 journal 和 checkpoint
  │   - 验证租约（lease fencing）
  │   - 重建待处理工作集
  │
  ├─> 调度器扫描
  │   - 发现可运行的 runs
  │   - 申领租约
  │
  ├─> 图驱动器恢复
  │   - plan_ready_nodes() 
  │   - 区分已完成/可调度/进行中的节点
  │   - 重放已完成的结果（不重新执行）
  │
  └─> 继续执行
      - 从最后的检查点恢复
      - 继续未完成的模型/工具调用
      - 保证 at-least-once 语义
```

## 关键设计决策

### 1. 类型化优先

JiaClaw 使用 StateKnot 的类型化 Agent API（`TypedAgent<ChatRequest, ChatResponse>`），而不是无类型的 map。这提供：

- 编译时安全
- JSON Schema 验证
- 清晰的输入/输出契约

### 2. 持久化优先

所有 Agent 运行都是持久化的：

- 每个状态转换提交到 PostgreSQL
- 检查点记录完整的执行状态
- 支持暂停、恢复、崩溃恢复

### 3. 协议原生互操作

通过 StateKnot 的适配器支持标准协议：

- **MCP** - 工具发现和调用
- **A2A** - Agent 间通信
- 避免协议类型泄露到核心领域模型

### 4. 生产就绪的治理

利用 StateKnot 的治理特性：

- 租户隔离
- 资源策略和预算
- 审计日志
- OpenTelemetry 跟踪

## 当前限制

由于 StateKnot 处于 pre-alpha 阶段，以下集成尚未完成：

1. **公共 API 稳定性** - StateKnot 的核心类型尚未发布
2. **完整的持久化配置** - 需要 PostgreSQL 连接和迁移
3. **模型提供者注册** - 需要 OpenAI/Anthropic API 密钥
4. **HTTP 服务集成** - 需要 AgentHost + 身份验证
5. **工具和技能注册** - 需要 MCP 客户端配置

参见 [StateKnot 能力差距](stateknot-gaps.md) 了解详细跟踪和上游议题。

## 下一步

1. **等待 Brokerrouter** - 作为 AI Gateway 层的优先路径
2. **监控 StateKnot 发布** - 等待稳定的公共 API
3. **实现持久化配置** - PostgreSQL 设置和迁移
4. **注册示例工具** - 基本 MCP 工具集成
5. **实现 HTTP 服务** - RESTful API 和 SSE 事件
6. **添加示例技能** - 可扩展的技能系统

## 参考资料

- [StateKnot 文档](https://stknot.com/docs/)
- [StateKnot 仓库](https://github.com/StateKnot/StateKnot)
- [Brokerrouter 仓库](https://github.com/StateKnot/Brokerrouter)
- [Brokerrouter 差距文档](brokerrouter-gaps.md)
- [StateKnot RFC 和设计文档](https://github.com/StateKnot/StateKnot/tree/main/docs)
