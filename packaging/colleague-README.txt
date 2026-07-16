Stats Code — 同事演示 / 试用包
================================

这是可直接发给同事的本地演示包。
- 统计数值由本机确定性引擎计算，不依赖大模型“编造数字”
- 未配置 API Key 也可完整跑通演示（专业模式）
- 本包不包含任何 API 密钥；请勿把自己的 Key 塞进压缩包再转发

一、最快上手（推荐）
--------------------
1. 解压整个文件夹（不要只抽单个 exe）
2. 双击「start.bat」或「stats-code.exe」
3. 浏览器自动打开后，点「暂不配置，进入专业模式」
4. 协议 →「加载演示协议」→ 审批
5. 上传 data\demo_cohort.csv（或界面「一键加载临床队列示例数据」）
6. 选「基线特征表」：group=disease，连续=age/bmi，分类=sex/smoke
7. 审阅数据质量卡 →「批准方案并运行」

二、可选：安装到本机
--------------------
双击「install.bat」（或 PowerShell 运行 install.ps1）
→ 安装到 %LOCALAPPDATA%\Programs\stats-code\
→ 创建桌面快捷方式「Stats Code」
→ 若包内有 data\demo_cohort.csv，会一并复制到安装目录

三、关于 API Key
----------------
- 核心统计（Table One / 回归 / 生存等）不需要 Key
- 若要用 AI 解读，请同事在自己电脑里单独配置自己的 Key
- 配置保存在本机 %APPDATA%\stats-code\，不会写回本 zip

四、常见问题
------------
- 浏览器没开：访问程序提示的 http://127.0.0.1:端口 （默认从 8080 起）
- 端口占用：程序会在 8080–8200 自动换空闲端口
- 杀软拦截：对 stats-code.exe 放行（本地 Node SEA 单文件）
- 校验：PowerShell 运行  powershell -ExecutionPolicy Bypass -File .\verify-demo-pack.ps1

五、请勿转发的内容
------------------
- 自己的 sk-... / llm-config.json / .env
- 真实患者数据（本包仅含脱敏 demo_cohort）
