# User Profile / 用户画像

此文件会在每次 `chat` 开始时注入系统提示（独立区块 `## User（用户画像）`）。

- 记录稳定的用户信息与偏好，便于个性化。
- 默认路径：`{workspace}/USER.md`；配置示例：`[identity] user_path = "USER.md"`。
- 文件不存在或为空时对话不会报错，只是不注入该区块。
- 超过 32KiB 时截断注入，并在日志中 warn。
- 可用工具 `user_write`（默认覆盖整文件；`replace=false` 追加）。

## Preferences

- （在此记录语言、沟通风格、常用任务）
