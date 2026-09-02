---
{
  "schemaVersion": 3,
  "featureId": "FEAT-WINDOWS-DISTRIBUTION",
  "authority": "mvp_contract",
  "specStatus": "draft",
  "implementationStatus": "in_progress",
  "implementationReview": {
    "status": "partial",
    "sourceRevision": "b5929f21cee3329d80e732a3fa2ed86ff6035f5c",
    "worktreeState": "dirty",
    "changedPaths": ["package.json", "scripts/package-windows.mjs", "scripts/tauri.mjs", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/src/main.rs", "src-tauri/src/platform.rs", "src-tauri/src/platform/tray.rs", "src-tauri/tauri.conf.json", "src/app/AppShellV2.tsx", "docs/release-windows.md", "docs/features/windows-distribution.md"],
    "reviewedAt": "2026-08-30",
    "summary": "已生成 Zephyr 0.1.1 NSIS 测试安装包，并用 PE Header 门禁保证 release 主程序为 Windows GUI subsystem；0.1.1 首次启动、覆盖安装、签名与自动更新仍待目标环境验证。",
    "knownDeviations": ["Zephyr 0.1.1 尚未在目标测试机复验首次启动无终端窗口"]
  },
  "validationStatus": "unverified",
  "components": ["system.zephyr", "backend.bootstrap", "storage.local", "platform.windows"],
  "decisions": ["ADR-0001", "ADR-0005"],
  "validationSlices": [
    { "id": "AC-WD-01", "components": ["system.zephyr", "backend.bootstrap", "platform.windows"], "requiredEvidence": ["automated", "windows_webview2", "release_artifact"] },
    { "id": "AC-WD-02", "components": ["backend.bootstrap", "storage.local", "platform.windows"], "requiredEvidence": ["release_artifact", "restart_persistence"] },
    { "id": "AC-WD-03", "components": ["system.zephyr", "backend.bootstrap"], "requiredEvidence": ["automated", "release_artifact"] },
    { "id": "AC-WD-04", "components": ["backend.bootstrap", "storage.local", "platform.windows"], "requiredEvidence": ["release_artifact", "restart_persistence", "fault_injection"] }
  ],
  "evidence": [
    {
      "id": "EV-WD-PACKAGE-20260828",
      "acceptanceIds": ["AC-WD-03"],
      "acceptanceCoverage": [{ "acceptanceId": "AC-WD-03", "coverage": "full" }],
      "method": "automated",
      "result": "pass",
      "freshness": "current",
      "capabilities": ["automated", "release_artifact"],
      "scope": "Windows x64 NSIS 打包门禁、前端与 Rust 自动化、release 构建、单一安装包输出、发布清单和独立 SHA-256 复核通过",
      "testRefs": ["npm run package:windows", "src-tauri/target/release/bundle/nsis/release-manifest.json"],
      "limitations": ["产物来自 dirty worktree", "安装包未签名", "未在干净 Windows 目标机执行安装、启动、完整功能、覆盖安装或卸载验收"],
      "sourceRevision": "b5929f21cee3329d80e732a3fa2ed86ff6035f5c",
      "worktreeState": "dirty",
      "changedPaths": ["scripts/package-windows.mjs", "scripts/deployment-env.mjs", "src-tauri/build.rs", "src-tauri/src/repositories.rs", "src-tauri/tauri.conf.json", "docs/release-windows.md", "docs/features/windows-distribution.md"],
      "environment": "Windows development workstation; NSIS currentUser x64; unsigned internal test artifact",
      "validatedAt": "2026-08-28"
    },
    {
      "id": "EV-WD-INSTALL-20260830",
      "acceptanceIds": ["AC-WD-01"],
      "acceptanceCoverage": [{ "acceptanceId": "AC-WD-01", "coverage": "partial" }],
      "method": "historical_observation",
      "result": "fail",
      "freshness": "current",
      "capabilities": ["windows_webview2", "usability_observation", "release_artifact"],
      "scope": "用户报告 0.1.0 安装包已在另一台 Windows 电脑成功安装并启动，但首次启动出现多个不会自动关闭的终端窗口；连接失败的同时存在飞行模式前置条件",
      "limitations": ["仅为用户报告，未收集目标机 Windows 版本、日志和进程快照", "飞行模式使外部 ASR/LLM 完整功能不可验收", "观察对象为已被 0.1.1 替代的 0.1.0 产物"],
      "sourceRevision": "b5929f21cee3329d80e732a3fa2ed86ff6035f5c",
      "worktreeState": "dirty",
      "environment": "另一台 Windows 测试电脑；具体版本和 WebView2 版本未知；用户观察",
      "validatedAt": "2026-08-30"
    },
    {
      "id": "EV-WD-PACKAGE-20260830",
      "acceptanceIds": ["AC-WD-03"],
      "acceptanceCoverage": [{ "acceptanceId": "AC-WD-03", "coverage": "full" }],
      "method": "automated",
      "result": "pass",
      "freshness": "current",
      "capabilities": ["automated", "release_artifact", "static_review"],
      "scope": "Zephyr 0.1.1 x64 NSIS 构建、前端 54 项测试、Rust 191 项测试、单一 Zephyr 命名产物、发布清单、独立 SHA-256 复算和 Windows GUI PE subsystem 检查通过",
      "testRefs": ["npm run package:windows", "dumpbin /headers src-tauri/target/release/gy-typing.exe", "src-tauri/target/release/bundle/nsis/release-manifest.json"],
      "limitations": ["产物来自 dirty worktree", "安装包未签名", "0.1.1 尚未在目标 Windows 电脑安装并复验首次启动无终端、完整功能、覆盖安装或卸载"],
      "sourceRevision": "b5929f21cee3329d80e732a3fa2ed86ff6035f5c",
      "worktreeState": "dirty",
      "changedPaths": ["package.json", "scripts/package-windows.mjs", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/src/main.rs", "src-tauri/src/platform.rs", "src-tauri/src/platform/tray.rs", "src-tauri/tauri.conf.json", "src/app/AppShellV2.tsx", "docs/release-windows.md", "docs/features/windows-distribution.md"],
      "environment": "Windows development workstation; NSIS currentUser x64; unsigned Zephyr 0.1.1 internal test artifact",
      "validatedAt": "2026-08-30"
    }
  ],
  "impactAssessments": []
}
---

# Windows 测试分发与升级

## 用户目标

维护者能够把 Zephyr 构建成普通 Windows 测试用户可安装的应用程序；安装包文件名和应用内用户界面以 `Zephyr` 命名，release 应用不显示或遗留任何控制台窗口。应用驻留系统托盘期间，关闭主窗口只隐藏界面，用户能够从托盘可靠地还原并聚焦同一个主窗口；只有明确选择托盘“退出”才终止应用。当前受控的小范围内部测试版本在构建时注入短期、限额且受监控的共享 ASR/LLM 凭据，测试用户安装后无需自行配置密钥即可使用完整功能。用户后续安装更高版本时，不需要先手工卸载，也不会因为覆盖程序文件而丢失原有本地配置和历史数据。后续自动更新必须验证发布者控制的更新签名，并在退出应用和替换程序前给用户明确反馈。

## 验收场景

- `AC-WD-01`：在干净的 Windows 10/11 x64 测试环境中，普通用户无需开发工具、管理员权限或手工填写 ASR/LLM 密钥即可运行 Zephyr 安装器、启动应用、使用完整功能并完成卸载；安装和首次启动不出现或遗留命令行终端；主窗口关闭按钮与 `Alt+F4` 均隐藏而不销毁主窗口，托盘左键和“打开设置”均能从隐藏或最小化状态还原、显示并聚焦同一个主窗口，连续执行至少 20 次不出现重复窗口、失焦、白屏或无法唤起，托盘“退出”仍完整终止应用；外部服务可用性、额度和麦克风权限作为验收前置条件记录。
- `AC-WD-02`：先安装旧版本并创建非敏感测试数据，再安装同一应用标识的更高版本；新版本可以启动，配置、凭据引用、历史和允许保留的恢复数据仍可读取。
- `AC-WD-03`：一次打包只产生预期的 Windows NSIS 安装包，并附带可追踪到版本、源码 revision、工作树状态和 SHA-256 的发布清单；版本源不一致或发布门禁失败时不产出可分发结果。
- `AC-WD-04`：自动更新启用后，只接受有效签名且版本策略允许的更新；下载、校验、安装或安全关闭失败时不得破坏现有安装和本地数据，并向用户呈现可操作的结果。

## 明确不规定的实现

- 本规格不要求首个测试安装包同时支持 MSI、Microsoft Store 或系统级安装。
- “增量更新”描述从旧版本升级到新版本的用户体验，不要求实现二进制差分补丁；允许下载完整的受签名安装产物后覆盖安装。
- 本规格不规定必须使用 GitHub Releases、对象存储或自建更新服务。
- 本规格不要求安装器或卸载器删除用户生成的数据；数据清理应由单独、明确的用户动作决定。
- 本规格不要求当前内部测试客户端达到公开分发所需的凭据保密强度；客户端内置共享凭据可被提取的边界必须保留在发布说明中。

## 局部假设

- `ASM-WD-01`（Open）：首轮测试对象使用 Windows 10/11 x64，暂不需要 ARM64 安装包。
- `ASM-WD-02`（Open）：首轮测试允许未签名安装包及其 SmartScreen 提示；公开分发前该假设必须关闭。
- `ASM-WD-03`（Open）：后续自动更新可以使用 HTTPS 可访问的静态发布清单；发布源和 channel 尚未确认。
- `ASM-WD-04`（Open）：应用标识 `com.gy.typing` 和当前用户安装范围将保持稳定，作为覆盖安装身份的一部分。
- `ASM-WD-05`（Confirmed）：当前小范围内部测试接受在客户端内置短期、限额且受监控的共享服务凭据；扩大分发范围前必须重新设计凭据签发与撤销边界。

## 架构决策

- 安装后的应用继续遵循 [ADR-0001：Tauri 本地桌面边界](../architecture/adr/0001-tauri-local-desktop-boundary.md)。
- 覆盖安装期间用户数据的兼容性继续遵循 [ADR-0005：带 revision 的原子本地存储](../architecture/adr/0005-revisioned-atomic-local-storage.md)。
- 自动更新发布源、签名密钥生命周期和 channel 成为确定的长期边界时，再评估是否新增 ADR。

## 当前实现入口

- 打包入口：`scripts/package-windows.mjs`
- 私密构建环境加载：`scripts/deployment-env.mjs`、未提交的 `.env.local`
- Rust 构建凭据注入：`src-tauri/build.rs`、`src-tauri/src/repositories.rs`
- npm 命令：`package.json`
- Tauri bundle 配置：`src-tauri/tauri.conf.json`
- 发布与目标环境验证手册：`docs/release-windows.md`
- 本地数据入口：`src-tauri/src/config.rs`、`src-tauri/src/history.rs`、`src-tauri/src/incident/schema.rs`

## 验证状态

当前仍为 `unverified`。0.1.0 已由用户在另一台 Windows 电脑成功安装，但首次启动出现常驻终端窗口，因此 `AC-WD-01` 未通过；当时电脑处于飞行模式，外部 ASR/LLM 完整功能也未形成有效证据。2026-08-30 已生成 `Zephyr_0.1.1_x64-setup.exe`，清单哈希与独立复算一致，release 主程序的 PE subsystem 从 0.1.0 的 Windows CUI 修正为 Windows GUI，打包门禁会阻止该问题回归，两项内部测试凭据仍确认存在。0.1.1 尚未在目标机复验首次启动、完整功能、覆盖安装、卸载或数据持久化，安装包也未签名，因此不能把静态修复和产物生成升级为“安装即用已完成验收”。

## 澄清历史

- 2026-08-27：用户提出为项目增加可供测试用户安装的打包程序，并考虑后续增量更新和覆盖安装。
- 2026-08-27：首阶段按 Windows 当前用户 NSIS 测试安装包推进；自动更新因发布源与签名密钥尚未确定，仅建立兼容边界，不声明已经实现。
- 2026-08-27：用户确认项目仍在并行开发，当前不执行安装包构建；真实 NSIS 产物与目标环境验证延期到开发收敛后。
- 2026-08-28：用户明确要求内部测试安装后无需配置即可使用完整功能，并接受内置短期、限额且受监控的共享密钥；ASR 切换为新版单 `APP Key` 构建注入模式。
- 2026-08-28：首次开发环境实机验证连续返回 ASR HTTP 401；检查确认实际调试二进制未包含当前部署 Key，原因是直接 Cargo/Tauri 入口绕过了 Node 环境加载器。凭据读取已下沉到 Rust `build.rs`，项目原生 Raw WebSocket 握手测试随后通过；快捷键完整链路仍需重启开发程序后复验。
- 2026-08-28：准备首个内部测试安装包时确认 DeepSeek Key 仍只存在于构建机凭据管理器。现已迁移到 Git 忽略的私密构建环境，并将 ASR 与 DeepSeek 凭据同时设为打包硬门禁和编译期注入项。
- 2026-08-28：首次 NSIS 尝试已完成 release 应用编译，但 Tauri 在系统临时目录与 D 盘 target 之间移动 NSIS 工具时因 Windows 跨卷限制失败；打包脚本随后将 `TEMP/TMP` 固定到 target 同卷目录后重试。
- 2026-08-28：仅迁移 `TEMP/TMP` 后仍因 Tauri 默认 NSIS 工具缓存位于系统盘而失败；启用官方 `bundle.useLocalToolsDir` 后，NSIS 下载、校验、解压和 `makensis` 全部通过，生成 0.1.0 x64 内部测试安装包及匹配的 SHA-256 发布清单。分发 Dossier 保持 `unverified`，等待干净目标机验收。
- 2026-08-30：另一台 Windows 电脑已成功安装首个测试包，但首次启动出现不会自行关闭的终端窗口；静态检查确认 0.1.0 release EXE 的 PE subsystem 为 Windows CUI。后续 release 入口改为 Windows GUI subsystem，并把 PE subsystem 检查加入打包门禁；分发产物文件名改为 Zephyr，同时保留既有 NSIS 内部产品名和应用标识以避免破坏首个测试版本的覆盖安装身份。
- 2026-09-02：用户明确主窗口关闭时应用应继续驻留托盘，关闭按钮与 `Alt+F4` 必须统一隐藏主窗口；托盘左键和“打开设置”应从隐藏或最小化状态恢复同一个窗口，窗口缺失或操作失败必须留下不含用户内容的分阶段诊断，托盘“退出”语义保持不变。该行为纳入 `AC-WD-01`，目标 Windows/WebView2 实机证据仍未完成，因此验证状态保持 `unverified`。
