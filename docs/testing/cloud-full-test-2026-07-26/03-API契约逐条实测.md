# Stats Code 云端测试 · 记录 03：API 契约逐条实测（21 条路由 × 成功/错误/边界）

- 执行方式：Python requests 直连云端部署 (127.0.0.1:8080)，共 **65 个用例**（首轮 56 + 复核 9）
- 完整逐用例证据：`证据/api-contract.jsonl`、`证据/api-contract-fixup.jsonl`（含每次请求/响应原文）
- 结果：**PASS 58 / FAIL(确认为缺陷) 6 / 观察项 1**（首轮 9 个 FAIL 中 4 个经复核系 harness 对语义预期有误，已用正确姿势重测通过；其余确认为真缺陷，见 §3）

## 1. 路由级结论总览

| # | 路由 | 成功路径 | 错误/边界路径 | 结论 |
|---|---|---|---|---|
| 1 | GET /api/health | `{"status":"ok"}` | — | PASS |
| 2 | POST /api/sessions | 201，UUID/Active/settings 齐全 | — | PASS |
| 3 | GET /api/sessions/:sid | 200 | 不存在→404 SessionNotFound；非 UUID→404 | PASS |
| 4 | GET /api/sessions | 200 数组含新建会话 | — | PASS |
| 5 | DELETE /api/sessions/:sid | 204；再查 404 | 重复删除→404（非幂等 204，可接受） | PASS |
| 6 | PATCH /:sid/settings | 200 关闭 decision_assistant | 类型错误→422 | PASS |
| 7 | GET /api/llm-status | 未配置 `configured:false` | — | PASS |
| 8 | POST /api/llm-config | —（云端无真 key，见记录05 mock 测试） | 无效 key→422 LLM_PROBE_FAILED 且**不落盘**；缺 key→422 | PASS |
| 9 | POST /:sid/datasets | 201：240 行 9 列，sha256/preview 齐 | 空 CSV→422；仅表头→201(0 行)；坏 base64→**201 接受(缺陷 D1)**；GBK→**误判 Utf8(缺陷 D2)** | PARTIAL |
| 10 | GET /:sid/datasets/:did | 200 含 preview_rows | 不存在→404 | PASS |
| 11 | PATCH /:sid/protocol | Draft 保存 / Approved 签发 approval_id | Approved 缺必填→422；expected_version 过期→409；字段>4000→422 | PASS |
| 12 | POST /:sid/protocol/compile | （需 LLM）未配置→502 LlmUnavailable | brief<20 字→422 | PASS |
| 13 | POST /:did/audit | 200 passed/warning + findings | 列不存在→200 blocked(ANALYSIS_COLUMN_MISSING)；缺必填 arg→422 SkillInvalidArgs；协议版本错→409 ResearchVersionConflict；Draft 协议→428 | PASS |
| 14 | POST /analysis-plans/approve | 201 plan_id + run_spec_sha256 | 篡改 audit_sha256→409 ResearchApprovalStale；**blocked 审计→409 ResearchAuditBlocked** | PASS |
| 15 | POST /:sid/run | 200 SkillResult(payload+analysis 元数据) | 无协议→428；无 plan→428；假 plan→409/404；args 与 plan 不符→428；未知 skill→422；strict 多余字段→422；**同一 plan 二次 run→428（一次性 plan，防重放）** | PASS |
| 16 | POST /:sid/messages (SSE) | text/event-stream；LLM 未配置→`event:error(LlmUnavailable)`+`event:done`，不挂死 | 空 body→413 **错误码语义不当(缺陷 D6)** | PARTIAL |
| 17 | POST /:sid/audio | 未配置 LLM→502，提示可用浏览器本地语音（符合设计） | 假 WAV 同上 | PASS |
| 18 | GET /api/coverage-matrix | 200，**17 个算法**、schema_version 1 | — | PASS |
| 19 | POST /api/sidecar/:algo | 带 columns 时 R/SAS/Python/SPSS 四语言 200 + 模板文本 | columns 为空(schema 允许)→**500(缺陷 D3)**；未知算法→**500 InternalError(缺陷 D4)**；非法 software→400 | PARTIAL |
| 20 | POST /api/snapshot/export | destination=文件路径→200，zip 11 项，**响应 sha256=落盘文件 sha256**；同 run 两次导出 **sha 完全一致（确定性 ✔）**；download=true→200 zip 流(25,112B, content-disposition 正确) | destination=已存在目录→500 EISDIR 裸 errno(缺陷 D5)；destination 空串→500 ENOENT(同 D5)；未知 run→404 | PASS(带 D5) |
| 21 | SPA 兜底 | / 与深链接→index.html；/api 未知→404 JSON | — | PASS |

## 2. 审批链（研究门 R3/R5）实测亮点 — 全部符合"fail-closed"设计

1. 未建协议直接 run → **428 ResearchProtocolRequired** ✔
2. Draft 协议审计 → 428 ✔；Approved 后才可审计 ✔
3. 审计 blocked（列缺失）→ approve 被 **409 ResearchAuditBlocked** 拒绝 ✔
4. 篡改 audit_sha256 → **409 ResearchApprovalStale** ✔
5. 换 args 复用旧 plan → 428 ✔；同 plan 重复 run → 428（一次一批） ✔
6. 协议 expected_version CAS 防并发覆盖 → 409 ✔
7. 快照 zip：`manifest.json/workflow.yaml/provenance/narrative...` 11 项；`stats-code replay <zip>` → **Replay PASS(1 step)**；`--sha256` 锚验证 ✔；篡改 sha → **Replay FAIL, exit 2** ✔

## 3. 本轮确认缺陷清单（按严重度）

| 编号 | 严重度 | 位置 | 现象 | 影响/建议 |
|---|---|---|---|---|
| D1 | 中 | POST datasets (dataset-store) | 非法 base64（`!!!not-base64!!!`）被宽松解码为乱码数据集，返回 201，列名为 `��~m�` | 输入校验缺失：应严格校验 base64 并 422。Node `Buffer.from(s,'base64')` 会静默忽略非法字符，需显式校验 |
| D2 | 中偏高 | 数据集编码检测 | GBK 编码 CSV（中文列名"组别/数值"）被判 `Utf8`，列名变 `���/��ֵ`；契约 Encoding 枚举含 `Gbk` 但从未产出 | 中文用户典型场景（Excel 另存 CSV 常为 GBK/ANSI）会得到乱码数据；建议按 BOM/GBK 启发式检测或对无效 UTF-8 序列回退 GBK |
| D3 | 中 | POST /api/sidecar/:algo | `columns:[]`（schema 默认值即空数组）→ 500 `column index 0 is out of range` | 契约允许的最小合法输入把服务器打成 500；模板渲染应对空列做占位/或 schema 要求非空 |
| D4 | 低中 | 同上 | 未知 algorithm_id → 500 InternalError（应 404/422） | 用户输入映射为服务器内部错误，错误分类不当 |
| D5 | 低 | snapshot/export | destination 为目录/空串 → 500 裸 errno(EISDIR/ENOENT) | 属输入校验缺口（桌面端总传文件路径所以平时不触发）；应 400 |
| D6 | 低 | POST messages | 空 body → 413 `MessageTooLong`，message 却是"缺少 text 字段" | 错误码与语义不符，应 422/400 |
| D8 | 低(文档) | CLI | CONTEXT.md 写 `--replay`，实际语法是 `replay <zip>`；敲 `--replay xxx.zip` 会**静默落入 launcher 模式起服务** | 文档与实现不一致 + 未识别 flag 不报错，建议未知 flag 显式报错 |
| D9 | 建议 | SSE 头 | `content-type: text/event-stream` 未带 `charset=utf-8`，非浏览器客户端可能按 latin-1 解出乱码（浏览器不受影响） | 加 charset 更稳 |

观察项（未复现）：download=true 曾出现一次 60s 读超时，此后 curl 与 requests 连测 4 次均 <20ms 正常，暂视为环境偶发，E2E 阶段继续盯。

## 4. 状态持久化

`~/.config/stats-code/`（Linux 对应 %APPDATA%\stats-code\）生成 `sessions.json` 与 `datasets/` ✔；
删除会话后 GET 404 ✔；无效 LLM key 探测失败后 `llm-config.json` 未写入 ✔。

## 5. 下一轮

记录 04：17 个统计算法正确性核对（仓库 R/SAS/SPSS/教科书基线 HTTP 重放 + scipy 独立复算 + 公开数据集）。
