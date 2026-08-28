---
{
  "schemaVersion": 3,
  "featureId": "FEAT-WINDOWS-DISTRIBUTION",
  "authority": "mvp_contract",
  "specStatus": "draft",
  "implementationStatus": "in_progress",
  "implementationReview": {
    "status": "partial",
    "sourceRevision": "dc4be390846b0a54e00cadf868db4b9c6db9686b",
    "worktreeState": "dirty",
    "changedPaths": ["package.json", "scripts/package-windows.mjs", "scripts/tauri.mjs", "src-tauri/tauri.conf.json", "docs/release-windows.md", "docs/features/windows-distribution.md"],
    "reviewedAt": "2026-08-27",
    "summary": "已建立 Windows NSIS 测试打包契约；真实安装、覆盖安装、签名与自动更新尚待目标环境验证和后续实现。",
    "knownDeviations": []
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
  "evidence": [],
  "impactAssessments": []
}
---

# Windows 测试分发与升级

## 用户目标

维护者能够把 Zephyr 构建成普通 Windows 测试用户可安装的应用程序；用户后续安装更高版本时，不需要先手工卸载，也不会因为覆盖程序文件而丢失原有本地配置和历史数据。后续自动更新必须验证发布者控制的更新签名，并在退出应用和替换程序前给用户明确反馈。

## 验收场景

- `AC-WD-01`：在干净的 Windows 10/11 x64 测试环境中，普通用户无需开发工具和管理员权限即可运行安装器、启动应用并完成卸载。
- `AC-WD-02`：先安装旧版本并创建非敏感测试数据，再安装同一应用标识的更高版本；新版本可以启动，配置、凭据引用、历史和允许保留的恢复数据仍可读取。
- `AC-WD-03`：一次打包只产生预期的 Windows NSIS 安装包，并附带可追踪到版本、源码 revision、工作树状态和 SHA-256 的发布清单；版本源不一致或发布门禁失败时不产出可分发结果。
- `AC-WD-04`：自动更新启用后，只接受有效签名且版本策略允许的更新；下载、校验、安装或安全关闭失败时不得破坏现有安装和本地数据，并向用户呈现可操作的结果。

## 明确不规定的实现

- 本规格不要求首个测试安装包同时支持 MSI、Microsoft Store 或系统级安装。
- “增量更新”描述从旧版本升级到新版本的用户体验，不要求实现二进制差分补丁；允许下载完整的受签名安装产物后覆盖安装。
- 本规格不规定必须使用 GitHub Releases、对象存储或自建更新服务。
- 本规格不要求安装器或卸载器删除用户生成的数据；数据清理应由单独、明确的用户动作决定。

## 局部假设

- `ASM-WD-01`（Open）：首轮测试对象使用 Windows 10/11 x64，暂不需要 ARM64 安装包。
- `ASM-WD-02`（Open）：首轮测试允许未签名安装包及其 SmartScreen 提示；公开分发前该假设必须关闭。
- `ASM-WD-03`（Open）：后续自动更新可以使用 HTTPS 可访问的静态发布清单；发布源和 channel 尚未确认。
- `ASM-WD-04`（Open）：应用标识 `com.gy.typing` 和当前用户安装范围将保持稳定，作为覆盖安装身份的一部分。

## 架构决策

- 安装后的应用继续遵循 [ADR-0001：Tauri 本地桌面边界](../architecture/adr/0001-tauri-local-desktop-boundary.md)。
- 覆盖安装期间用户数据的兼容性继续遵循 [ADR-0005：带 revision 的原子本地存储](../architecture/adr/0005-revisioned-atomic-local-storage.md)。
- 自动更新发布源、签名密钥生命周期和 channel 成为确定的长期边界时，再评估是否新增 ADR。

## 当前实现入口

- 打包入口：`scripts/package-windows.mjs`
- npm 命令：`package.json`
- Tauri bundle 配置：`src-tauri/tauri.conf.json`
- 发布与目标环境验证手册：`docs/release-windows.md`
- 本地数据入口：`src-tauri/src/config.rs`、`src-tauri/src/history.rs`、`src-tauri/src/incident/schema.rs`

## 验证状态

当前为 `unverified`。NSIS 打包配置和发布门禁已建立；用户明确要求并行开发阶段暂不构建，因此尚未保留成功构建的 artifact SHA-256，也未在非开发机验证全新安装、覆盖安装、卸载、数据持久化或自动更新。

## 澄清历史

- 2026-08-27：用户提出为项目增加可供测试用户安装的打包程序，并考虑后续增量更新和覆盖安装。
- 2026-08-27：首阶段按 Windows 当前用户 NSIS 测试安装包推进；自动更新因发布源与签名密钥尚未确定，仅建立兼容边界，不声明已经实现。
- 2026-08-27：用户确认项目仍在并行开发，当前不执行安装包构建；真实 NSIS 产物与目标环境验证延期到开发收敛后。
