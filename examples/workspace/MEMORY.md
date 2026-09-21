# Long-term Memory / 长期记忆

此文件会在每次 `chat` 开始时注入系统提示（内容原样）。

- 可手动编辑，或让 Agent 调用 `memory_append`（`replace=false` 追加 / `replace=true` 覆盖）。
- 默认路径：`{workspace}/MEMORY.md`；配置示例：`[memory] path = "MEMORY.md"`。
- 文件不存在或为空时对话不会报错，只是不注入该区块。
- 超过 32KiB 时截断注入，并在日志中 warn。

## Key Facts

- （在此记录跨会话稳定事实）
