# Stats Code 修复任务清单（交给 Claude Code 执行）

> 这份清单由云端全量测试（含补测）产出。每条缺陷都已**独立复现**，并给出**精确文件:行号、根因、改法、验证方式**。
> 被测代码：本仓库 `ts-backend/`（TS 后端）+ `web/`（React 前端）。测试时未改任何业务代码。
>
> **给 Claude Code 的开场提示（可直接粘贴）：**
> ```
> 请阅读 测试记录/修复任务清单-给ClaudeCode.md，按 P0→P4 顺序逐条修复。
> 每条：先读我标注的文件:行号确认根因，改完后跑该条的“验证”命令。
> 约束：不要破坏现有契约测试（tests/integration/contract-*）；改完后在 ts-backend 跑 `npm run typecheck` 和 `npm test`，web 跑 `npm test`，全绿再进行下一条。
> 行为类改动（D11/D12/G2）先只读分析并向我确认方案再动手，不要擅自改判定逻辑。
> ```

---

## P0 · 安全（最高优先级）

### S1 — CORS `Access-Control-Allow-Origin: *`，任意网页可跨源操纵本地 API
- **文件**：`ts-backend/packages/server/src/router.ts:210-237`
- **现状**：`reply.header('access-control-allow-origin', '*')`（第 228、235 行），方法 `GET,POST,PATCH,DELETE,OPTIONS`，headers `*`。注释写"mirrors tower_http CorsLayer::permissive()"。
- **根因**：本产品是**无认证的 localhost 单机应用**（绑 `127.0.0.1:8080`）。ACAO=* 让用户浏览器里打开的任意恶意网页都能跨源读写会话/数据/LLM 配置/导出快照（DNS-rebinding 面）。
- **改法**：把 CORS 收紧为同源。桌面壳/本地 SPA 都是同源访问，本不需要 `*`。建议：
  - 仅当请求 `Origin` 为 `http://127.0.0.1:<port>` / `http://localhost:<port>` / 桌面壳 origin 时回显该 origin，否则不发 ACAO（或发 `null`）。
  - 对状态变更路由（POST/PATCH/DELETE）加同源校验（校验 `Origin`/`Sec-Fetch-Site`），非同源直接 403。
  - 复现命令：`curl -H "Origin: https://evil.example.com" -I http://127.0.0.1:8080/api/health` —— 修复后不应回显 `access-control-allow-origin: *`。
- **验证**：契约/集成测试仍绿；上面 curl 不再对陌生 origin 放行；SPA 自身功能不受影响（同源）。

### S2 — spawn 哨兵 `.exe` 剥离仅在 Windows，非 Windows 上 `python.exe` 逃逸
- **文件**：`ts-backend/packages/engine/src/spawn_policy.ts:76-88`（`normalizeCommand`，`if (IS_WINDOWS && ...) strip .exe`）；对应失败单测 `tests/unit/spawn-policy.test.ts:49`
- **根因**：`.exe` 后缀仅当 `IS_WINDOWS` 时剥离。于是在 Linux/Mac 上 `C:\Python311\python.exe` → 归一化为 `python.exe`（不在黑名单）→ **`checkSpawn` 放行**。`matchForbiddenCommand('/usr/bin/python3')` 正确拦截，但 `.exe` 变体逃逸。单测 `spawn-policy.test.ts:49-52` 与 property `forbidden-spawn` 是平台无关断言，在非 Windows CI 上**真失败**。
- **改法**：把已知可执行后缀（`.exe`、可选 `.bat`/`.cmd`/`.com`）的剥离改为**平台无关**——始终尝试剥离再比对。保证哨兵跨平台一致。
- **验证**：`node -e "const e=require('./build/…engine');console.log(e.matchForbiddenCommand('C:\\\\Python311\\\\python.exe'))"` 应非 null；`npm run test:unit` 中 spawn-policy 与 property forbidden-spawn 转绿。

---

## P1 · 旗舰功能 / 用户会撞死的墙

### D16 — 快照导出主按钮「下载审计快照」抛未处理文件系统错误（审计快照是信任三件套之一）
- **文件**：`ts-backend/packages/engine/src/snapshot/exporter.ts`（第 32 行 import 只有 `readFileSync/renameSync/rmSync`，**没有 `mkdirSync`**；第 290 行 `renameSync(destTmp, destination)`）；前端 `web/src/hooks/useSnapshotExport.ts` 传的 destination
- **复现**：专业模式跑完分析 → 第 5 步"诊断与导出" → 点「下载审计快照」→ 浏览器红字 `导出失败 EEXIST: file already exists, mkdir '…'`。API 侧：destination=已存在目录→`500 EISDIR`；父目录不存在→`500 ENOENT`；正常文件路径→200（重复导出也 200）。
- **根因**：exporter 写 `<dest>.tmp` 再 `renameSync` 到 `<dest>`，但**从不创建父目录**（缺 mkdir），也不处理 destination 是目录的情况；download 流程某处 `mkdir` 未容忍已存在（EEXIST）。
- **改法**：
  1. 导出前 `mkdirSync(dirname(destination), { recursive: true })`（recursive 天然容忍已存在，消除 EEXIST/ENOENT）。
  2. 若 destination 落在已存在目录上，归一化为该目录下的文件名（或直接报 400 友好提示，而非 EISDIR）。
  3. download 模式优先走内存流（不落地临时文件），失败时给前端结构化错误而非裸 errno。
  4. 前端 `useSnapshotExport`：download 模式不要传会触发目录 mkdir 的 destination。
- **验证**：跑两次 `POST /api/snapshot/export {download:true}` 到同一路径都 200；浏览器点「下载审计快照」得到 zip 而非红字；`stats-code replay <zip>` 仍 PASS。

### D12 —（行为类，先确认）列名 `weight`/`wt` 一律被当抽样权重，整库阻断且不可覆盖
- **文件**：`ts-backend/packages/server/src/conversation/dataset-audit.ts:100-103`（weight 候选含 `'weight','weights','wt'` + 正则 `/^…(weight|weights|wt)$/`）、`:389-391`（`SURVEY_DESIGN_UNSUPPORTED` blocker）
- **复现**：上传含 `weight`/`wt` 列的 CSV（如 R PlantGrowth 结局列就叫 weight）→ 任意审计 `status:blocked / SURVEY_DESIGN_UNSUPPORTED`，且 blocker 无法覆盖 → 该数据集永远无法分析。
- **根因**：把"列名恰好含 weight/wt"等同于"这是抽样权重"。但出生体重/体重/植物重量等作为结局/协变量极常见。
- **改法（需你先确认方向）**：不能只凭列名判定抽样权重。建议：仅当用户在 `roles.weight` 显式声明时才触发 SURVEY_DESIGN_UNSUPPORTED；或在审计卡给"这不是抽样权重"的一键否认，否认后放行。**先只读分析 + 给方案，勿擅改。**
- **验证**：PlantGrowth（weight 结局列）能走完审计→审批→ANOVA，得 F=4.846/p=0.0159。

### D11 —（行为类，先确认）主键只认 7 种命名且用户指定被拒 → 无白名单列名的数据集在 UI 内死锁
- **文件**：`ts-backend/packages/server/src/conversation/dataset-audit.ts:73-77`（主键候选仅 participant_id/subject_id/patient_id/person_id/record_id/study_id/case_id + 正则）、`:303-324`（`PRIMARY_KEY_UNBOUND` / `AUDIT_ROLE_OVERRIDE_REJECTED`）
- **复现**：上传列名 `id`/`rownames` 或无 ID 列的数据集 → `blocked / PRIMARY_KEY_UNBOUND`；用 `roles.primary_key:['rownames']` 覆盖 → 追加 `AUDIT_ROLE_OVERRIDE_REJECTED`，仍 blocked。UI 无引导，用户只能改列名重传。
- **改法（需你先确认方向）**：放开"经用户确认的主键绑定"路径（把用户指定的 PK 写进审计哈希链即可保证可复现），不要一律拒绝覆盖；UI 在 blocked 时明确提示"请指定主键列"。**先只读分析 + 给方案。**
- **验证**：mtcars 原始列名（无 participant_id）能经用户指定主键后走完审批链。

---

## P2 · 健壮性 / 错误分类

### D15 — 退化/非法输入一律 `500 SkillExecutionFailed`（应 422）
- **文件**：`ts-backend/packages/server/src/conversation/skill-runner.ts`（引擎正确抛 Error）→ `packages/server/src/router.ts:588` 错误映射（`err.code==='InternalError'?500:...`，未识别"输入错误"类）
- **复现**（全链路 run 均 500）：ttest 单组 / ttest 单例组 / linear 完全共线 / correlation 零方差列 / correlation 非法 method / anova 非数值文本。引擎消息都清晰正确，只是 HTTP 码错。
- **改法**：在 skill-runner 对"输入导致的拒绝"抛一个可识别错误（如 `SkillInvalidInputError` 或复用 `SkillInvalidArgs`），router 据此映射 **422** 而非 500。保留 500 仅给真正的内部异常。
- **验证**：上述 6 种输入返回 422 且带原有清晰消息；契约测试不回归。

### D1 — 非法 base64 只挡空值，非空乱码被接受（201）
- **文件**：`ts-backend/packages/server/src/router.ts:623-629`
- **根因**：`Buffer.from(data,'base64')` 对非法字符**静默产乱码不抛错**；现有 422 只在结果为空时触发。`"!!!not-base64!!!"` 解码非空 → 放行 → 乱码数据集。
- **改法**：解码前用严格校验，例如 `/^[A-Za-z0-9+/]*={0,2}$/` 且长度 %4==0；或 `Buffer.from(s,'base64').toString('base64')` 往返比对（去除换行后不等即非法）→ 422。
- **验证**：`data:"!!!not-base64!!!"` → 422；正常 CSV 上传仍 201。

### D2 — 编码从不检测，GBK/UTF-16 中文一律乱码
- **文件**：`ts-backend/packages/server/src/conversation/delimited-table.ts:40`（写死 `new TextDecoder('utf-8')`）；`packages/server/src/conversation/dataset-store.ts:255`（写死 `encoding:'Utf8'`）
- **根因**：无编码探测，永远按 UTF-8 解码并标 Utf8。契约 `Encoding` 枚举里的 `Gbk`/`Utf16` **从未产出**。
- **改法**：加编码探测——先看 BOM（`FF FE`/`FE FF`→UTF-16LE/BE，`EF BB BF`→UTF-8-BOM）；无 BOM 时尝试 UTF-8 严格解码（`fatal:true`），失败则回退 GBK（Node 需 `TextDecoder('gbk')`，或引入轻量探测）。`dataset-store` 按探测结果如实标 `encoding`。
- **验证**：GBK 编码"组别,数值"上传 → `encoding:Gbk`、列名正确中文；UTF-16LE 同理；纯 UTF-8 不回归。

### D3 — sidecar 空 columns（schema 默认值）→ 500
- **文件**：`ts-backend/packages/engine/src/sidecar/render.ts:48`（`column index ${index} is out of range`）；契约 `packages/server/src/contract/sidecar.ts` `columns` 默认 `[]`
- **复现**：`POST /api/sidecar/ttest {software:"R",dataset_sha256:"…"}`（不传 columns）→ 500。
- **改法**：模板渲染遇缺失列索引时给占位（如 `<col>`）而非抛错；或契约要求 columns 非空并在路由层 422。二选一，与前端实际传参对齐（前端正常会传 columns）。
- **验证**：不带 columns 的 sidecar 请求返回 200 占位文本或 422，不再 500。

### D4 — sidecar 未知算法 → 500 InternalError（应 404/422）
- **文件**：`ts-backend/packages/engine/src/sidecar/index.ts:220`（`throw new GenerateError('unknown_algorithm', …)`）→ router 映射成 500
- **改法**：router 层把 `unknown_algorithm` 映射为 404（或 422），不要归 500。
- **验证**：`POST /api/sidecar/not_an_algo` → 404/422。

### D6 — 空消息体 → `413 MessageTooLong`，文案却是"缺少 text 字段"
- **文件**：`ts-backend/packages/server/src/router.ts:453`（`reply.code(413).send({error_code:'MessageTooLong', message:'请求体缺少 text 字段'})`）
- **改法**：缺 text 字段用 **422**（如 `SkillInvalidArgs` 或新增 `InvalidRequest`）；413/MessageTooLong 仅保留给真正超长（第 456 行那支）。
- **验证**：`POST …/messages {}` → 422；超长消息仍 413。

---

## P3 · 清理（低风险）

### D5 — 快照 destination 传目录/空串 → 500 裸 errno
- 归入 **D16** 一并处理（mkdir recursive + destination 归一化 + 400 友好提示）。

### D8 — `--replay` 文档与实现不一致，且未知 flag 静默启动服务器
- **文件**：`CONTEXT.md`（写 `--replay`）vs `ts-backend/packages/engine/src/cli.ts:100-102`（实际是 `replay <zip>` 子命令）；`classifyInvocation` 对未知 `--flag` 落入 launcher。
- **改法**：CONTEXT.md 改为 `replay <zip> [--sha256 <sha>]`；`cli.ts` 对未识别的 `--flag` 显式报错退出（非 `--no-browser/--version/--help/-V` 的 flag → stderr + exit 2），不要静默起服务。
- **验证**：`stats-code --replay x.zip` 报错而非起服务；`stats-code replay x.zip` 正常。

### D9 — SSE `Content-Type` 缺 `charset=utf-8`
- **文件**：SSE 响应头设置处（`packages/server/src/sse.ts` 或 router 里起 SSE 的地方；grep `text/event-stream`）
- **改法**：`text/event-stream` → `text/event-stream; charset=utf-8`。
- **验证**：非浏览器客户端（curl/requests）读取 SSE 帧中文不乱码。

### D14 — `/assets/*` 未命中兜底 index.html（应 404），引发 OTS 字体报错噪声
- **文件**：`ts-backend/packages/server/src/spa.ts:47-72`（`setNotFoundHandler`，`ASSET_PREFIXES` 在 :30）
- **改法**：notFoundHandler 里，若 `routePath` 以 `ASSET_PREFIXES` 任一前缀开头但未命中 asset → 返回 404，不要回退 index.html；仅非 asset 路由回退 SPA。
- **验证**：请求不存在的 `/assets/x.woff2` → 404；深链接 `/some/route` 仍回 index.html。

---

## P4 · 测试资产修复

### D10 — Kleinbaum 线性回归基线内嵌数据损坏（weight 列 = sbp 列）且无人消费
- **文件**：`ts-backend/tests/parity/known_values/linear_textbook_kleinbaum.json`
- **现状**：`dataset.rows` 20 行的 weight 列全部等于 sbp 值；`expected` 是 β_age=0.8614/β_weight=0.3342/R²=0.9981，与损坏数据不符；无测试引用。
- **改法**：用真实 Kleinbaum SBP 数据（sbp/age/weight 三列，N=20）重录，并在 parity 或 unit 里接入一条消费它的断言；或删除该文件。
- **验证**：若保留，新增测试用它做 OLS 得到 expected 值。

### D13 — Hosmer Logistic 基线数值与其声称的模型-数据不可复现
- **文件**：`ts-backend/tests/parity/known_values/logistic_textbook_hosmer.json`
- **现状**：声称 low~age+lwt 得 β_age=-0.0271/β_lwt=-0.0152/ll=-117.336；但对权威 MASS birthwt(N=189) 做同模型 MLE 实为 β=(-0.03979,-0.01278)/ll=-113.562（引擎/scipy/独立 Newton 三方一致）。无内嵌数据、无测试消费。
- **改法**：明确数据来源重录（附内嵌数据），或删除。
- **验证**：若保留，新增测试用内嵌数据复现 expected。

---

## P5 · 需产品决策（非机械修复，先讨论）

### G2 — 引擎 17 算法只有 8 个有 UI/HTTP 通路
- **文件**：`ts-backend/packages/server/src/conversation/skill-runner.ts`（分发 switch 仅 tableone/ttest/anova/correlation/linear/logistic/cox/kaplan_meier + power/inspect）；`web` 配置器仅 7 模块。对照 `packages/engine/src/stats/index.ts` `ALGORITHM_IDS`(17) 与 `/api/coverage-matrix`(宣示 17)。
- **缺口**：nonparametric、or_rr、attributable_risk、standardization、life_table、diagnostic_roc 无 `/run` 通路（422"未知统计方法"）；power_phase2/3/single_arm 合并为一个 `power` skill。覆盖矩阵与等价代码侧栏对外宣示 17 个可用。
- **两个方向（请选）**：
  - (A) 补齐 skill-runner 分发 + 配置器模块，把 9 个补上（工作量大但对齐宣示）；
  - (B) 覆盖矩阵/文档显式区分"引擎级能力 / 界面可运行能力"，不让用户误以为都能点（快，但功能面不变）。

### UX 观察项（可选）
- **O2**：t 检验组序按数据首现序，符号可能与 R/SPSS（按水平排序）相反；`skill-runner` runTtest 里改为按组标签排序，或 UI 标注差值方向。
- **O3**：简易模式追问卡要用户填"数据集 ID"（UUID），过技术化；改为给数据集选择器。

---

## 验证总纲（每批改完跑）

```bash
# 后端
cd ts-backend
npm run typecheck          # 类型
npm run lint               # 导入边界 + 规则
npm test                   # unit+integration+property+parity 全套
# 前端
cd ../web
npm test
# 端到端冒烟（可选）
cd ../ts-backend && npm run build && node dev-server.mjs &   # :8080
# 然后手点 或 跑你自己的 e2e
```

**回归红线**：`tests/integration/contract-golden.test.ts`、`contract-diff-harness.test.ts` 必须保持绿——它们锁定 API 契约，任何路由/错误码改动都要同步更新契约 schema（`packages/server/src/contract/*`）后再让这两个测试通过。

---

## 附：本清单来源
云端全量测试 + 补测，全部缺陷已独立复现。详见同目录 `00-问题总清单-汇总.md`（每条完整证据）、`03/04/05/06/08` 分层记录、`证据与截图.zip`（58 张截图 + 逐用例 JSONL）。
