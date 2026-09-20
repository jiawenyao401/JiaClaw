# StateKnot 能力差距跟踪

本文档跟踪 JiaClaw 开发中遇到的 StateKnot 能力差距，以及已提交或计划提交的上游议题。

## 概述

StateKnot 目前处于 **pre-alpha** 阶段（版本 0.0.0，`publish = false`）。这意味着：

- ❌ 核心 crates 尚未发布到 crates.io
- ❌ 公共 API 尚未稳定（无兼容性承诺）
- ⚠️ 内部 API 可能随时更改
- ✅ 核心架构和合约已审查
- ✅ 实现了基础的持久化执行能力

JiaClaw 使用 **git 依赖** 来跟踪 StateKnot 开发进度，但完整功能需要等待稳定发布。

## 已识别的能力差距

### 1. 稳定公共 API 发布 (Critical)

**影响**: 无法作为 crates.io 依赖使用 StateKnot

**当前状态**: 
- StateKnot 所有 crates 的 `publish = false`
- 文档明确标注："Do not use it in production at this stage"

**JiaClaw 需求**:
- 发布的 `stateknot-core` crate（核心类型）
- 发布的 `stateknot-runtime` crate（Agent 运行时）
- 发布的 `stateknot-integrations` crate（模型和协议适配器）
- 语义版本化承诺

**临时方案**:
```toml
# Cargo.toml
stateknot-core = { git = "https://github.com/StateKnot/StateKnot", rev = "main" }
```

**上游议题**: 待创建
- 标题: "Track: Stable public API release for v0.1.0"
- 描述: JiaClaw 需要稳定的公共 API 来构建个人智能体运行时
- 接受标准:
  - [ ] `stateknot-core` 发布到 crates.io
  - [ ] `stateknot-runtime` 发布到 crates.io
  - [ ] 公开文档可访问（docs.rs）
  - [ ] 语义版本化策略文档化

---

### 2. 简化的 Agent 运行 API (High Priority)

**影响**: 需要低级 API 构建完整的运行流程

**当前状态**:
- 存在 `DurableAgentAdmission::admit()`
- 存在 `DurableAgentLoop`
- 存在 `AgentHost` 但要求完整的 HTTP/Worker/Maintenance 角色

**JiaClaw 需求**:
```rust
// 期望的简化 API
let agent = TypedAgent::<ChatRequest, ChatResponse>::new(config)?;
let result = agent.run(request).await?;
```

**当前方法**:
```rust
// 需要的步骤（来自文档）
1. 构建 AgentBuilder<I, O>
2. 注册 schemas
3. TypedAgent::prepare_request()
4. DurableAgentAdmission::admit() - 持久化准入
5. 手动协调 Graph Driver 执行
6. DurableAgentLoop 循环
7. TypedAgent::decode_result()
```

**临时方案**:
- JiaClaw 实现自己的协调逻辑（`JiaClawAgent` 包装器）
- 当前返回存根响应

**上游议题**: 待创建
- 标题: "Feature: Add convenience API for simple agent runs"
- 描述: 
  - JiaClaw 用例：快速启动的个人助手，不需要完整的多角色部署
  - 提议 API：`agent.run(request)` 或 `agent.run_local(request, store)`
  - 接受标准：
    - [ ] 提供进程内执行 API（用于开发/测试）
    - [ ] 保留持久化保证
    - [ ] 文档化生产部署路径

---

### 3. PostgreSQL 配置简化 (Medium Priority)

**影响**: 设置持久化存储需要大量样板代码

**当前状态**:
- 需要手动运行 PostgreSQL 迁移
- 需要手动构建连接池
- 需要配置租户、身份、策略

**JiaClaw 需求**:
```rust
// 期望的配置 API
let config = PersistenceConfig::from_env()?;
let store = PostgresStore::connect(config).await?;
```

**临时方案**:
- 暂时跳过持久化集成
- 使用存根实现进行开发

**上游议题**: 待创建
- 标题: "Feature: Add configuration helpers for PostgreSQL setup"
- 描述:
  - JiaClaw 用例：简化开发环境设置
  - 提议：配置构建器或环境变量支持
  - 接受标准：
    - [ ] 环境变量配置（`DATABASE_URL` 等）
    - [ ] 自动迁移运行选项
    - [ ] 开发模式默认值

---

### 4. 本地工具注册 API (Medium Priority)

**影响**: 不清楚如何注册 Rust 本地工具

**当前状态**:
- MCP 远程工具支持已实现（`McpRemoteTool`）
- A2A 远程 Agent 支持已实现（`A2aRemoteAgent`）
- 本地 Rust 工具注册 API 未文档化

**JiaClaw 需求**:
```rust
// 期望能注册本地工具
#[derive(Tool)]
struct SearchTool { /* ... */ }

agent_builder.add_tool(SearchTool::new())?;
```

**临时方案**:
- 使用 MCP 协议包装本地工具
- 或等待文档/示例

**上游议题**: 待创建
- 标题: "Documentation: How to register local Rust tools"
- 描述:
  - JiaClaw 用例：内置工具（搜索、文件操作等）
  - 需求：注册 trait、序列化、工具描述符
  - 接受标准：
    - [ ] 文档化工具 trait
    - [ ] 本地工具示例
    - [ ] 与 MCP 工具的互操作性

---

### 5. 技能系统集成 (Low Priority)

**影响**: 不确定如何映射"技能"到 StateKnot 概念

**当前状态**:
- StateKnot 支持 MCP Skills（SEP-2640）
- 有 `McpSkillBoundTool` 实现

**JiaClaw 需求**:
- 技能作为可组合的能力单元
- 技能可以包含工具、提示、资源
- 技能激活和权限管理

**临时方案**:
- 将技能建模为工具集合
- 在 JiaClaw 层面管理技能逻辑

**上游议题**: 待创建
- 标题: "Discussion: Best practices for skill composition"
- 描述:
  - JiaClaw 用例：用户定义的技能模块
  - 问题：技能 vs 图 vs 子图？
  - 接受标准：
    - [ ] 设计指南文档
    - [ ] 技能组合示例

---

## 上游议题状态

| 议题编号 | 标题 | 状态 | 优先级 | 链接 |
|---------|------|------|--------|------|
| TBD | Stable public API release tracking | 待创建 | Critical | - |
| TBD | Convenience API for simple agent runs | 待创建 | High | - |
| TBD | Configuration helpers for PostgreSQL | 待创建 | Medium | - |
| TBD | Documentation: Local Rust tools | 待创建 | Medium | - |
| TBD | Discussion: Skill composition | 待创建 | Low | - |

**创建议题计划**:
1. 完成 JiaClaw 初始 PR（展示用例和集成点）
2. 基于真实集成经验提炼议题描述
3. 在 StateKnot 仓库创建议题
4. 更新此文档链接

---

## 工作继续策略

尽管存在这些差距，JiaClaw 开发仍可继续：

### 1. 适配器模式

```rust
// jiaclaw/src/lib.rs
pub struct JiaClawAgent {
    config: AgentConfig,
    // StateKnot 集成字段将在 API 稳定后添加
    // typed_agent: TypedAgent<ChatRequest, ChatResponse>,
}
```

在 StateKnot API 边界处定义清晰的适配器，使未来集成更容易。

### 2. 存根和模拟

```rust
impl JiaClawAgent {
    pub async fn chat(&self, request: ChatRequest) 
        -> Result<ChatResponse, JiaClawError> 
    {
        // 当前：返回存根响应
        // 未来：调用 StateKnot TypedAgent
        Ok(stub_response())
    }
}
```

实现可编译的存根，保持类型正确，等待运行时集成。

### 3. 文档驱动开发

- 编写期望的 API 使用方式
- 记录当前限制和阻塞点
- 创建架构图展示集成点
- 提供清晰的"下一步"指引

### 4. 测试基础设施

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_agent_creation() { /* ... */ }

    #[tokio::test]
    async fn test_chat_stub() { /* ... */ }
}
```

编写测试验证接口契约，即使实现是存根。

---

## 预期时间线

基于 StateKnot README 的当前里程碑：

- **当前**: 架构合约和持久化运行时验证阶段
- **下一步**: 
  1. 保留已完成的 PostgreSQL 严格 MCP 恢复证明
  2. 完成 API 审查
  3. 限定角色隔离、故障转移/恢复
  4. 发布兼容性和性能证据

**JiaClaw 里程碑**:

- ✅ **M0**: 脚手架和架构（当前）
- ⏳ **M1**: StateKnot 稳定 API 集成（等待上游）
- ⏳ **M2**: 基本持久化执行（需要 M1）
- ⏳ **M3**: 工具和技能系统（需要 M1-M2）
- ⏳ **M4**: HTTP 服务和部署（需要 M1-M3）

---

## 贡献到 StateKnot

如果我们在 JiaClaw 开发中发现可以贡献回 StateKnot 的改进：

1. **遵循 StateKnot 的 RFC 流程**
   - 复制 `docs/rfcs/0000-template.md`
   - 说明可观察保证、失败行为、安全影响
   - 提供可执行的接受测试

2. **提交小的、可审查的 PR**
   - 保持 PR 可独立审查和回滚
   - 包含测试和文档
   - 运行 `cargo fmt` 和 `cargo clippy`

3. **使用 DCO 签名**
   ```bash
   git commit -s
   ```

---

## 更新日志

- **2026-09-20**: 初始文档，识别 5 个主要差距
- **待定**: 创建上游议题并更新链接

---

## 参考资料

- [StateKnot README](https://github.com/StateKnot/StateKnot/blob/main/README.md)
- [StateKnot CONTRIBUTING](https://github.com/StateKnot/StateKnot/blob/main/CONTRIBUTING.md)
- [StateKnot v1 Scope](https://github.com/StateKnot/StateKnot/blob/main/docs/v1-scope.md)
- [StateKnot Typed Agent Guide](https://github.com/StateKnot/StateKnot/blob/main/docs/typed-agent.md)
