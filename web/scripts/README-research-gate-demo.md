# 研究门禁 Demo 录制

这个脚本用真实浏览器重放并录制完整闭环：

1. 服务端审批演示研究协议；
2. 从 UI 上传 `demo_cohort_with_issues.csv`；
3. 选择 Logistic 回归后，由服务端审计识别重复主键、非法事件编码和零人时，并禁用“批准方案并运行”；
4. 从 UI 换成修正后的 `demo_cohort.csv`；
5. 服务端审计通过，服务端签发方案 ID，确定性引擎完成运行；
6. 校验最终运行绑定的方案 ID 与用户实际审阅的审计 ID/哈希一致，且运行参数没有客户端自报的审批字段。

## 前置条件

- 后端已运行在 `http://127.0.0.1:8080`；
- Web 已运行在 `http://127.0.0.1:5173`，并将 `/api` 代理到同一个后端；
- 已安装 Web 依赖及 Chromium：`npm install`、`npx playwright install chromium`。

## 运行

在 `web/` 目录执行：

```powershell
npm run demo:research-gate
```

如需观察浏览器，使用：

```powershell
$env:HEADED='1'
npm run demo:research-gate
```

可覆盖以下环境变量：

- `STATS_URL`：Web 地址；
- `API_URL`：后端地址；
- `HEADED=1`：显示浏览器；
- `DEMO_SLOW_MO`：每个浏览器动作的减速毫秒数；
- `DEMO_STEP_PAUSE`：关键画面的停留毫秒数。

产物写入 `web/output/playwright/research-gate-demo/`：

- `research-gate-demo.webm`：完整录屏；
- `01` 至 `06` 的关键步骤截图；
- `report.json`：服务端审计、方案和运行绑定证据，以及 PASS/FAIL 结果。

脚本不会启动或停止服务，也不会删除录制过程中创建的会话；`report.json` 会记录会话 ID，便于人工复核。
