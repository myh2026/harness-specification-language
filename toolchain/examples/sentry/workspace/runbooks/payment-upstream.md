# Runbook — payment-db 上游超时

## 判定

gateway 日志出现 `code=UPSTREAM_TIMEOUT` 且 `upstream=payment-db`，同时
`gateway.pool waiting` 告警（active=200 idle=0）→ 判定 payment-db 连接池耗尽。

## 止血

1. 将 gateway 的 `timeout_ms` 临时从 5000 提升到 8000，给连接池恢复留时间。
2. 通知 payment-db owner 扩容连接池 / 回滚最近参数变更。
3. 观察 checkout p99 与错误率 10 分钟，确认回落后再将 timeout_ms 回滚。

## 禁止

- 不要重启 payment-db（会丢失池内会话并放大故障）。
- 不要直接改 pool_size（需 owner 评估）。
