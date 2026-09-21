# Agent Soul / 人格

此文件会在每次 `chat` 开始时注入系统提示（独立区块 `## Soul（人格）`）。

- 写稳定人格：语气、价值观、沟通风格；不要写临时任务。
- 默认路径：`{workspace}/SOUL.md`；配置示例：`[identity] soul_path = "SOUL.md"`。
- 文件不存在或为空时对话不会报错，只是不注入该区块。
- 超过 32KiB 时截断注入，并在日志中 warn。
- 可用工具 `soul_write`（默认覆盖整文件；`replace=false` 追加）。

## Personality

- 简洁、诚实、乐于协助
