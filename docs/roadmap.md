# JiaClaw 路线图

本文档概述 JiaClaw 的开发计划和里程碑。

## 项目愿景

JiaClaw 致力于成为一个**生产就绪的个人持久化智能体运行时**，使个人用户能够：

- 运行持久化、可恢复的智能体工作流
- 通过自然语言交互控制工具和服务
- 构建和共享自定义技能
- 在本地或云端部署（保持隐私控制）

## 相对竞品的定位

与 [OpenClaw](https://github.com/openclaw/openclaw) 和 [Hermes Agent](https://github.com/NousResearch/hermes-agent) 相比，JiaClaw 聚焦于：

- **持久化优先**: 确定性图执行、检查点、崩溃恢复（at-least-once 语义）
- **类型安全**: Rust + TypedAgent<I,O> 编译时保证
- **生产治理**: 多租户、资源策略、预算控制、审计日志
- **协议原生**: MCP 和 A2A 一等公民支持

详见 [竞争差距分析](competitive-gap.md) 了解完整对比矩阵。

## 当前状态：M0 - 脚手架与架构 ✅

**完成日期**: 2026-09-20

已完成：
- ✅ Cargo 工作空间结构
- ✅ 三个 crates：`jiaclaw-core`、`jiaclaw`、`jiaclaw-host`
- ✅ 核心领域类型（ChatMessage、ChatRequest、ChatResponse）
- ✅ StateKnot 集成框架（存根实现）
- ✅ CLI 骨架（`version`、`chat`、`serve` 命令）
- ✅ 文档（README、架构、StateKnot 差距）
- ✅ 测试基础设施
- ✅ 许可证（Apache-2.0 OR MIT）

可交付物：
- PR #1 到 jiawenyao401/JiaClaw
- 可编译的代码库（`cargo build` 通过）
- 基本测试套件（`cargo test` 通过）

---

## M0.5 - 可用迭代（无需 StateKnot） 🚧

**目标日期**: 2026-09-20  
**依赖**: 无（独立于 StateKnot 稳定 API）

**目标**: 提供实际可用的最小功能集，支持本地开发和测试

**架构约束**: 
- ✅ 通过 [Brokerrouter](https://github.com/StateKnot/Brokerrouter) 作为 AI Gateway（规划中）
- ✅ 保持适配器边界清晰，支持离线存根模式
- ✅ 临时直连模式仅用于早期开发（将废弃）

### 任务

1. **竞争差距分析** (3/3) ✅
   - [x] 撰写 `docs/competitive-gap.md` 对比 OpenClaw/Hermes
   - [x] 更新 README 添加相对竞品章节
   - [x] 更新 roadmap.md 整合差距分析

2. **OpenAI-compatible 提供商** (5/5) ✅ *临时方案*
   - [x] 添加 `minreq` 依赖和 HTTP 客户端
   - [x] 实现 OpenAI Chat Completions API 调用
   - [x] 支持 `base_url` 配置（兼容 Ollama/LM Studio）
   - [x] 环境变量 `JIACLAW_API_KEY` / 配置文件
   - [x] 无 API key 时回退到存根（清晰文档说明）
   - ⚠️ **注意**: 这是临时直连方案，待 Brokerrouter 可用后将迁移

3. **Brokerrouter 集成准备** (2/5) 🚧
   - [x] 创建 `docs/brokerrouter-gaps.md` 需求文档
   - [x] 更新架构文档提及 Brokerrouter
   - [ ] 等待 Brokerrouter 仓库可用
   - [ ] 创建 Brokerrouter 集成议题 (#1-#10)
   - [ ] 实现 `BrokerrouterProvider` 适配器

4. **工作空间引导** (6/6) ✅
   - [x] 设计工作空间目录结构（`~/.jiaclaw/workspace/`）
   - [x] 创建 `AGENTS.md` - Agent 配置和元数据
   - [x] 创建 `SOUL.md` - Agent 性格和指令
   - [x] 创建 `USER.md` - 用户信息和偏好
   - [x] 创建 `MEMORY.md` - 长期记忆和上下文
   - [x] 实现 `jiaclaw init` 命令生成默认文件

5. **技能骨架** (5/5) ✅
   - [x] 设计 `SKILL.md` 格式（参考 agentskills）
   - [x] 实现技能发现（扫描 `skills/*/SKILL.md`）
   - [x] 列出启用的技能
   - [x] 注入技能摘要到系统提示
   - [x] 添加 1-2 个示例技能（search, calculator）

6. **CLI 改进** (4/4) ✅
   - [x] `jiaclaw init` 创建工作空间和配置
   - [x] `jiaclaw chat` 使用真实提供商（有 key 时）
   - [x] `jiaclaw serve` 添加健康检查端点（axum/hyper）
   - [x] 改进错误消息和日志输出

7. **测试和文档** (4/4) ✅
   - [x] 配置加载测试（TOML/JSON/环境变量）
   - [x] 工作空间引导测试
   - [x] 技能发现测试
   - [x] 存根 vs 真实提供商选择测试

### 验收标准

- [ ] `jiaclaw init` 创建完整的工作空间
- [ ] `jiaclaw chat "你好"` 在无 API key 时返回存根
- [ ] `jiaclaw chat "你好"` 在有 API key 时调用真实模型
- [ ] 技能目录被扫描并注入提示
- [ ] 工作空间文件（AGENTS/SOUL/USER/MEMORY）影响 Agent 行为
- [ ] 所有测试通过（`cargo test`）
- [ ] Clippy 无警告（`cargo clippy`）
- [ ] 代码格式化（`cargo fmt --check`）

### 相对 OpenClaw/Hermes 的进展

| 功能 | OpenClaw | Hermes | JiaClaw M0.5 | 差距 |
|------|----------|--------|--------------|------|
| 真实模型调用 | ✅ | ✅ | 🚧 本阶段 | P0 |
| 工作空间引导 | ✅ | ⏳ | 🚧 本阶段 | P1 |
| 技能系统 | ✅ 完整 | ⏳ 部分 | 🚧 骨架 | P1（完整实现在 M3） |
| 离线开发模式 | ✅ | ⏳ | 🚧 本阶段 | P1 |

---

## M1 - StateKnot 集成（基础） ⏳

**依赖**: 
- StateKnot 稳定公共 API 发布
- Brokerrouter 基础路由可用

**目标**: 实现基本的持久化 Agent 执行

### 任务

1. **StateKnot API 跟踪** (0/4)
   - [ ] 监控 StateKnot 发布到 crates.io
   - [ ] 更新 `Cargo.toml` 依赖（从 git 到版本）
   - [ ] 验证 API 稳定性
   - [ ] 更新 `docs/stateknot-gaps.md`

2. **PostgreSQL 持久化** (0/5)
   - [ ] 添加 PostgreSQL 配置（环境变量/配置文件）
   - [ ] 运行 StateKnot 迁移脚本
   - [ ] 实现 `PostgresStore` 集成
   - [ ] 配置租户和身份
   - [ ] 测试基本的持久化和恢复

3. **类型化 Agent 集成** (0/6)
   - [ ] 替换 `JiaClawAgent` 存根为真实实现
   - [ ] 使用 `AgentBuilder<ChatRequest, ChatResponse>`
   - [ ] 注册 JSON Schema
   - [ ] 实现 `prepare_request` / `decode_result`
   - [ ] 实现 `DurableAgentAdmission::admit()` 调用
   - [ ] 基本的 Agent 运行循环

4. **模型提供者** (0/3)
   - [ ] 迁移到 Brokerrouter Gateway（优先）
   - [ ] 配置 Brokerrouter 端点和路由规则
   - [ ] 集成 StateKnot Model Adapter（与 Brokerrouter 协同）
   - [ ] 测试 Brokerrouter → OpenAI/Anthropic/Ollama

5. **测试和文档** (0/3)
   - [ ] 端到端测试（提交 -> 执行 -> 结果）
   - [ ] 持久化恢复测试
   - [ ] 更新 README 和架构文档

### 验收标准

- [ ] `jiaclaw chat "你好"` 返回来自真实模型的响应
- [ ] 运行状态持久化到 PostgreSQL
- [ ] 进程重启后可以恢复运行
- [ ] 所有测试通过

### 预估工作量

- 核心集成：高度依赖 StateKnot API 的设计
- 持久化配置：中等复杂度
- 测试和文档：中等工作量

---

## M2 - 工具系统 ⏳

**依赖**: M1 完成

**目标**: 支持工具调用和 MCP 集成

### 任务

1. **本地工具注册** (0/4)
   - [ ] 设计 JiaClaw 工具 trait
   - [ ] 实现示例工具（搜索、计算、文件操作）
   - [ ] 集成到 Agent 执行循环
   - [ ] 工具调用持久化

2. **MCP 客户端集成** (0/5)
   - [ ] 配置 MCP 服务器连接
   - [ ] 使用 `McpRemoteTool` 适配器
   - [ ] 工具发现和列表
   - [ ] 测试远程工具调用
   - [ ] 处理 MCP 错误和重试

3. **工具选择逻辑** (0/3)
   - [ ] 根据用户请求选择相关工具
   - [ ] 并发工具调用（如果 Agent 配置允许）
   - [ ] 工具调用结果整合

4. **测试和文档** (0/3)
   - [ ] 本地工具测试
   - [ ] MCP 工具集成测试
   - [ ] 工具开发指南

### 验收标准

- [ ] Agent 可以调用本地 Rust 工具
- [ ] Agent 可以发现和调用 MCP 工具
- [ ] 工具调用持久化并可恢复
- [ ] 提供 3+ 个内置工具

---

## M3 - 技能系统 ⏳

**依赖**: M2 完成

**目标**: 可扩展的技能模块系统

### 任务

1. **技能定义** (0/4)
   - [ ] 设计技能清单格式（YAML/TOML）
   - [ ] 技能包含：工具、提示、资源
   - [ ] 技能依赖和版本管理
   - [ ] 技能加载和验证

2. **技能激活** (0/3)
   - [ ] 运行时技能激活/停用
   - [ ] 技能权限和安全策略
   - [ ] 技能状态管理

3. **示例技能** (0/5)
   - [ ] 文件管理技能
   - [ ] 网页搜索技能
   - [ ] 代码辅助技能
   - [ ] 日历和提醒技能
   - [ ] 数据分析技能

4. **技能市场（可选）** (0/3)
   - [ ] 技能共享格式
   - [ ] 技能发现机制
   - [ ] 社区技能仓库

### 验收标准

- [ ] 用户可以定义自定义技能
- [ ] Agent 可以动态加载/卸载技能
- [ ] 提供 5+ 个示例技能
- [ ] 技能文档和开发指南

---

## M4 - HTTP 服务与 API ⏳

**依赖**: M1 完成（M2/M3 可选）

**目标**: 生产就绪的 HTTP 服务

### 任务

1. **AgentHost 集成** (0/5)
   - [ ] 配置 HTTP / Worker / Maintenance 角色
   - [ ] 身份验证集成（OAuth2/OIDC）
   - [ ] 资源策略配置
   - [ ] 健康检查和就绪探针
   - [ ] 优雅关闭

2. **REST API** (0/6)
   - [ ] POST /api/v1/chat - 提交聊天请求
   - [ ] GET /api/v1/runs/:id - 查询运行状态
   - [ ] POST /api/v1/runs/:id/cancel - 取消运行
   - [ ] GET /api/v1/tools - 列出可用工具
   - [ ] GET /api/v1/skills - 列出可用技能
   - [ ] API 文档（OpenAPI/Swagger）

3. **SSE 事件流** (0/3)
   - [ ] GET /api/v1/runs/:id/events - 运行事件流
   - [ ] 断点续传支持
   - [ ] 心跳和连接管理

4. **部署** (0/4)
   - [ ] Docker 镜像
   - [ ] Docker Compose 示例
   - [ ] Kubernetes manifests（可选）
   - [ ] 部署文档

### 验收标准

- [ ] HTTP API 文档完整
- [ ] 支持身份验证和授权
- [ ] SSE 事件流稳定
- [ ] Docker 部署可用

---

## M5 - 高级特性（未来） 🔮

**依赖**: M1-M4 完成

潜在特性：

1. **多模态支持**
   - [ ] 图像输入/输出
   - [ ] 音频输入/输出
   - [ ] 文件和文档处理

2. **协作特性**
   - [ ] Agent-to-Agent 通信（A2A）
   - [ ] 多用户会话
   - [ ] 共享技能和工具

3. **高级持久化**
   - [ ] 时间旅行和 fork
   - [ ] 运行快照和导出
   - [ ] 审计日志和回放

4. **观察性**
   - [ ] OpenTelemetry 集成
   - [ ] Prometheus 指标
   - [ ] 分布式追踪

5. **性能优化**
   - [ ] 响应流式传输
   - [ ] 并行工具执行
   - [ ] 缓存和预热

---

## 里程碑时间线

由于 StateKnot 的 pre-alpha 状态，我们不提供具体的日期预估。相反，我们跟踪**技术依赖**和**阻塞因素**：

```
M0 (脚手架)
  ✅ 完成 (2026-09-20)
  │
  ▼
M1 (StateKnot 集成)
  ⏳ 阻塞：等待 StateKnot 稳定 API 发布
  │  依赖：PostgreSQL 16+ 环境
  │
  ├─── M2 (工具系统)
  │      依赖：M1 完成
  │      │
  │      └─── M3 (技能系统)
  │             依赖：M2 完成
  │
  └─── M4 (HTTP 服务)
         依赖：M1 完成（可与 M2/M3 并行）
         │
         ▼
       M5 (高级特性)
         依赖：M1-M4 完成
```

---

## 社区参与

我们欢迎社区贡献！以下是参与方式：

### 现在可以帮助

- ✅ 测试脚手架和报告问题
- ✅ 改进文档（README、架构）
- ✅ 提议新技能想法
- ✅ 翻译文档（英文改进）

### M1 后可以帮助

- 实现新的工具
- 开发示例技能
- 编写集成测试
- 性能基准测试

### 贡献指南

1. Fork 仓库
2. 创建特性分支（`git checkout -b feature/amazing-tool`）
3. 提交更改（`git commit -s`）
4. 推送到分支（`git push origin feature/amazing-tool`）
5. 开启 Pull Request

遵循：
- Rust 2024 edition 惯例
- 运行 `cargo fmt` 和 `cargo clippy`
- 添加测试覆盖
- 更新文档

---

## 风险和缓解

### 风险 1: StateKnot API 变更

**影响**: 高  
**概率**: 中（pre-alpha 性质）

**缓解**:
- 在 JiaClaw 层定义清晰的适配器边界
- 跟踪 StateKnot 上游变更
- 保持集成点最小化和模块化

### 风险 2: StateKnot 发布延迟

**影响**: 高  
**概率**: 中

**缓解**:
- 继续架构和设计工作
- 实现独立的 JiaClaw 特性（技能定义、CLI）
- 与 StateKnot 社区保持沟通

### 风险 3: 性能瓶颈

**影响**: 中  
**概率**: 低

**缓解**:
- 早期基准测试
- 优化关键路径
- 利用 StateKnot 的并发能力

---

## 成功指标

### M1 成功指标
- 完全功能的持久化聊天 Agent
- < 5 秒的响应时间（简单查询）
- 100% 的恢复成功率（崩溃后）

### M2 成功指标
- 至少 5 个工作的工具
- 支持 MCP 协议
- 工具调用成功率 > 95%

### M3 成功指标
- 至少 5 个示例技能
- 技能加载时间 < 1 秒
- 社区贡献的技能 > 3

### M4 成功指标
- API 响应时间 < 500ms (p95)
- 支持 100+ 并发用户
- 正常运行时间 > 99.9%

---

## 参考资料

- [StateKnot 路线图](https://github.com/StateKnot/StateKnot/blob/main/docs/roadmap.md)
- [StateKnot 研究和实现计划](https://github.com/StateKnot/StateKnot/blob/main/docs/research-and-implementation-plan.md)
- [JiaClaw 架构文档](architecture.md)
- [StateKnot 能力差距](stateknot-gaps.md)
