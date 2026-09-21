# Heartbeat / 定时心跳

此文件由 `jiaclaw serve` 按间隔读取全文，作为 user 消息跑一轮 chat（固定 session，默认 `heartbeat`）。

- 默认关闭：配置 `[heartbeat] enabled = true` 后才会在 **serve 进程内** 启动后台任务。
- 默认路径：`{workspace}/HEARTBEAT.md`；可用 `[heartbeat] path` 覆盖。间隔默认 3600 秒，可用 `JIACLAW_HEARTBEAT_INTERVAL_SECS` 覆盖。
- 文件缺失或为空时跳过本轮（debug 日志），不会让 serve 退出。
- CLI `jiaclaw chat` 不跑心跳。不要在这里写密钥。

## 示例

- 检查未完成提醒，必要时用 `memory_write` 记下结果。
- 若无需动作，回复一句简短确认即可。
