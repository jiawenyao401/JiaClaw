---
name: web_search
description: 搜索互联网信息并提供相关结果
triggers:
  - search
  - 搜索
  - find
  - 查找
---

# Web Search Skill

这是一个网络搜索技能，可以帮助用户搜索互联网信息。

## 描述

当用户需要搜索信息时，这个技能可以提供帮助。它能够：
- 理解搜索意图
- 提供相关结果
- 总结搜索内容

## 使用场景

- "帮我搜索最新的 AI 新闻"
- "查找 Rust 编程教程"
- "search for weather in Beijing"

## 相关工具

- `web_search` - 联网检索（Brave Search；需配置 API key）
- `web_fetch` - 抓取网页为可读纯文本（默认拒绝 localhost/私网）
- `http_get` - 获取网页原始响应
- `json_query` - 解析 JSON 响应

## 注意事项

此技能引导模型使用本地 `web_search` 工具。未配置 Brave API key 时，工具会返回友好错误（设置 `JIACLAW_BRAVE_API_KEY` 或 `[tools.web_search] brave_api_key`）。可用 `[tools.web_search] enabled = false` 关闭注册。
