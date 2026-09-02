---
{
  "schemaVersion": 3,
  "featureId": "FEAT-MACOS-RUNNABLE-SLICE",
  "authority": "mvp_contract",
  "confirmation": {
    "confirmedBy": "user",
    "confirmedAt": "2026-09-01",
    "sourceRef": "Codex task: user clarified that the current macOS goal is a real runnable end-to-end slice, not exact Windows feature parity"
  },
  "specStatus": "confirmed",
  "implementationStatus": "not_started",
  "implementationReview": {
    "status": "unreviewed",
    "sourceRevision": "c9f9a6a0e19f3505b78398058263a54dd57ced8e",
    "worktreeState": "clean",
    "reviewedAt": "2026-09-01",
    "summary": "当前 clean main 仍是 Windows-only；尚未开始 macOS Runnable Slice 的实现复核。",
    "knownDeviations": []
  },
  "validationStatus": "unverified",
  "components": [
    "system.zephyr",
    "frontend.shell",
    "frontend.ipc",
    "backend.bootstrap",
    "backend.commands",
    "backend.services",
    "backend.voice-controller",
    "backend.streaming",
    "backend.repositories",
    "external.asr"
  ],
  "decisions": ["ADR-0001", "ADR-0002", "ADR-0004", "ADR-0005", "ADR-0012"],
  "validationSlices": [
    {
      "id": "AC-MACRUN-01",
      "components": ["system.zephyr", "frontend.shell", "backend.bootstrap"],
      "requiredEvidence": ["automated", "runtime_hook"]
    },
    {
      "id": "AC-MACRUN-02",
      "components": ["backend.bootstrap", "backend.voice-controller", "backend.streaming"],
      "requiredEvidence": ["automated", "runtime_hook"]
    },
    {
      "id": "AC-MACRUN-03",
      "components": ["backend.services", "backend.voice-controller", "backend.streaming", "external.asr"],
      "requiredEvidence": ["automated", "runtime_hook"]
    },
    {
      "id": "AC-MACRUN-04",
      "components": ["frontend.shell", "frontend.ipc", "backend.commands", "backend.services"],
      "requiredEvidence": ["automated", "runtime_hook"]
    },
    {
      "id": "AC-MACRUN-05",
      "components": ["system.zephyr", "backend.bootstrap", "backend.services", "backend.voice-controller", "backend.streaming"],
      "requiredEvidence": ["automated"]
    }
  ],
  "evidence": [],
  "impactAssessments": []
}
---

# macOS Runnable Slice

## 用户目标

当前阶段的目标是在同一个 Zephyr 仓库中，让核心语音链路在至少一台真实 Mac 上首次端到端跑通，而不是立即精确复现 Windows 的全部桌面原生体验。

用户能够在 Mac 上启动应用，通过应用内按钮或一个简单、可靠的快捷方式开始和结束录音，获得真实语音识别结果，并在 Zephyr 内查看、复制结果后手动粘贴到目标应用。第一阶段成功的判断是“核心价值链可用且失败可解释”，不是“与 Windows 功能对等”。

## 验收场景

### AC-MACRUN-01：真实 Mac 构建与启动

- 在一台已记录芯片架构和 macOS 版本的真实 Mac 上，从 clean revision 完成 Rust/Tauri 与前端构建并启动应用。
- 主窗口能够正常渲染；设置和核心页面不会因为 Windows-only 初始化失败而整体不可用。
- 尚未实现的 macOS 能力必须明确显示为 Unsupported 或 Unavailable，不得以 no-op 返回成功。

### AC-MACRUN-02：真实麦克风输入

- 用户可在应用内开始和结束一次录音，会话读取到真实、非零的麦克风音频并正常释放设备。
- Microphone 的未决定、已授权、拒绝或设备不可用状态可区分；拒绝权限不会造成应用崩溃或持续假录音。
- 应用内按钮可以承担本切片的触发入口；普通快捷键只有在实现成本可控时才作为便利项加入。

### AC-MACRUN-03：共享 ASR 与处理链路

- Mac 采集的真实音频进入现有共享 Voice/ASR 链路，至少完成 Fast 路径的一次 final transcript。
- 当前受控内部凭据可用时，可以继续验证现有 LLM 成稿路径；LLM 凭据重构和完整成稿模式不得阻塞 Fast 路径的首次跑通。
- 不复制第二套 VoiceSessionActor、ASR 协议或文本处理主链路来换取 Mac 可运行。

### AC-MACRUN-04：结果可见且可带走

- 最终文本在 Zephyr 界面中可见，并提供明确的复制动作。
- 用户可以把复制结果手动粘贴到其他应用；本验收不要求自动捕获目标窗口或自动发送 Cmd+V。
- ASR、权限或设备失败时，界面给出可理解的失败状态，不把失败显示成空白成功。

### AC-MACRUN-05：Windows 基线不回归

- 为 macOS 增加的条件编译、依赖和启动装配不破坏当前 Windows 构建与既有自动化。
- 共享链路的变化继续遵守现有单所有者会话、取消和有界队列边界。

## 明确不规定的实现

以下事项不属于当前 Runnable Slice 的完成条件；它们可以被调研或做独立技术探针，但不得阻塞、扩大或偷换本切片：

- 精确复现 Windows 的按住说话、松开结束、左右修饰键、Fn/Globe、鼠标侧键或全局吞键语义；
- 捕获和复验原目标应用、自动 Cmd+V、剪贴板事务、三态交付恢复或跨应用自动写回；
- 透明非激活浮层、跨 Space/全屏/多显示器浮层体验；
- Accessibility 或 Input Monitoring 权限，除非选择的最小触发方案实际需要；
- Mac App Store、MAS sandbox 或 App Store 审核兼容；
- Developer ID、Hardened Runtime、公证、staple、DMG、自动更新和干净机器安装；
- Intel 或 Universal Binary，以及完整最低系统支持矩阵；
- 所有第三方应用互操作、IDE/终端输入表面分类或 Windows 功能对等；
- 为了“跨平台整洁”而先建设覆盖所有系统能力的大型统一平台层。

这些后续能力仍受既有数据完整性和副作用安全边界约束。“暂不要求自动写回”不等于允许用不确定、可重复提交的方式快速实现自动写回。

## 局部假设

### ASM-MACRUN-01：首台目标 Mac 环境

- 状态：Open
- 当前判断：先以开发团队实际可获得的一台真实 Mac 作为 bring-up 环境，并完整记录芯片与系统版本。
- 影响：该机器上的成功是开发证据，不自动形成对所有 macOS 版本或 CPU 架构的支持承诺。

### ASM-MACRUN-02：手动复制是可接受的首阶段交付

- 状态：Confirmed
- 结论：最终文字在应用内可见并可复制，已经足以完成当前“Mac 真正跑通”的目标。

### ASM-MACRUN-03：首阶段触发入口可以降级

- 状态：Confirmed
- 结论：应用内按钮或一个简单、可靠的普通快捷键均可用于首次跑通；无需先复现 Windows 的完整全局物理键语义。

### ASM-MACRUN-04：内部服务凭据可用性

- 状态：Open
- 当前判断：ASR 首次联调可以沿用现有受控内部凭据模型，但必须在真实 Mac 上验证读取与网络链路。
- 影响：如果 LLM 凭据或 Keychain 适配阻塞，只降级到 Fast transcript，不扩大本切片去解决公开分发凭据模型。

## 概念迭代记录

### CI-MACRUN-01：完整自动写回平台化不是 Runnable Slice 前置条件

- 状态：Confirmed
- 当前结论：Windows 自动交付已经通过现有 `DeliveryExecutor` 隔离执行机制，但共享工作流仍保留 `Unicode`、`ClipboardPaste` 等 Windows 风格策略。该语义耦合必须在 macOS 自动写回真正进入范围时处理；当前 Runnable Slice 通过应用内结果路线绕开目标捕获和自动交付，不先建设 `AutomaticTextDeliveryPort`。
- 依据：`AC-MACRUN-04` 只要求结果在 Zephyr 内可见并可复制，且“明确不规定的实现”排除了自动 `Cmd+V`、剪贴板事务和跨应用写回。提前抽象尚无第二个平台实现反馈的自动交付策略会扩大当前 MVP，并增加错误抽象风险。

### CI-MACRUN-02：下一实现阶段只建立最小 macOS 适配与 Unsupported 边界

- 状态：Confirmed
- 当前结论：下一阶段先处理 target-specific 依赖、macOS 可用的凭据 backend、Tauri macOS 配置、Windows-only 启动装配隔离、显式 Unsupported/Unavailable 适配器，以及 Apple Silicon CI 的编译、测试和 `.app` 打包。`OutputRoute::InApp`、UI 录音按钮、结果卡和麦克风权限生命周期在随后的垂直切片中实现。
- 依据：先让同一工程暴露真实 macOS 编译与启动阻塞，能够保持故障可归因；该阶段只证明构建脊柱成立，不替代 `AC-MACRUN-01` 至 `AC-MACRUN-04` 所需的真实 Mac 运行证据。

## 架构决策

- [ADR-0001：Tauri 本地桌面边界](../architecture/adr/0001-tauri-local-desktop-boundary.md) 提供当前壳层与 IPC 的历史边界；它的 Windows-only 部分需要在实现 macOS 时由后续决策扩展，而不是复制一套 App。
- [ADR-0002：单所有者有界语音会话](../architecture/adr/0002-single-owner-bounded-voice-session.md) 和 [ADR-0012：统一语音输入控制面](../architecture/adr/0012-unified-voice-input-control-plane.md) 继续约束共享会话链路。
- [macOS 单仓能力切片开发与安全交付边界](../architecture/proposals/macos-parallel-development.md) 是后续原生输入和分发设计的 Proposed 材料，不覆盖本 Dossier 的当前 MVP 范围。

## 当前实现入口

- 应用装配与平台入口：`src-tauri/src/lib.rs`、`src-tauri/src/main.rs`、`src-tauri/src/platform.rs`
- 语音会话与触发：`src-tauri/src/voice_controller/`、`src-tauri/src/voice_trigger.rs`
- 目标能力边界：`src-tauri/src/target_port.rs`、`src-tauri/src/windows_target.rs`
- 麦克风与 ASR：`src-tauri/src/audio.rs`、`src-tauri/src/streaming_pipeline.rs`、`src-tauri/src/provider/`
- 共享服务与 IPC：`src-tauri/src/services.rs`、`src-tauri/src/commands/`、`src/ipc/client.ts`
- 主界面：`src/app/AppShellV2.tsx`、`src/features/`

## 验证状态

- `implementationStatus=not_started`：截至记录 revision，产品仍按 Windows-only 装配，尚无可复核的 macOS Runnable Slice。
- `validationStatus=unverified`：没有真实 Mac 上绑定 clean revision 的构建、麦克风、ASR 和结果复制证据。
- Windows 上的单元测试、CI 或既有功能证据不能替代本 Dossier 的 macOS 实机证据。
- `3d71d91` 已在 clean `main` 将 Windows 目标捕获、存在性检查、前台复验和激活迁移到 `TargetPort`/`WindowsTargetAdapter`，共享 Voice、Delivery 和 Pending 只持有带平台中立 context 的 opaque `CapturedTarget`；GitHub Actions run `33538788062` 的 PR fast 与 Full engineering checks 均通过。该记录只证明跨平台准备边界和 Windows 自动化基线，不覆盖任何 `AC-MACRUN-*` 的真实 Mac 证据，因此不改变上述状态。

## 澄清历史

- 2026-09-01：用户明确把当前目标从“首个 Mac Alpha 精确覆盖 Windows 原生体验和分发链路”收窄为“先在真实 Mac 上端到端跑通核心价值链”。原生快捷键、目标捕获、透明浮层、自动写回和公证 DMG 保留为后续里程碑，不再作为当前实现前置条件。
- 2026-09-02：Windows 目标能力端口化以 `3d71d91` 进入 clean `main` 并通过对应 GitHub CI；用户确认下一步进入“macOS 所需的最小适配器和明确的 Unsupported 能力”，不把完整自动写回平台化提前为 Runnable Slice 前置条件。
