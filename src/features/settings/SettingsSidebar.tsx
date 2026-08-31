import type { JSX, RefObject } from "preact";
import type { AppConfig, AsrOptionPool, ConfigStatus, ConfigValue, PolishLevel, ShortcutTriggerMode, VoiceStatePayload } from "../../domain";
import { ShortcutCaptureField } from "../shortcut/ShortcutCaptureField";
import { ShortcutTriggerModeField } from "../shortcut/ShortcutTriggerModeField";
import type { ShortcutBindingViewModel } from "../shortcut/useShortcutBindingController";
import { BehaviorSwitch } from "./BehaviorSwitch";
import { OptionPoolRenderer } from "./OptionPoolRenderer";
import { PolishLevelSetting } from "./PolishLevelSetting";

const RULER_TICK_COUNT = 12;
const RULER_TICK_WINDOW = 100 / (RULER_TICK_COUNT - 1);
const RULER_TICKS = Array.from({ length: RULER_TICK_COUNT }, (_, i) => {
  const isFirst = i === 0;
  const isLast = i === RULER_TICK_COUNT - 1;
  const center = (i / (RULER_TICK_COUNT - 1)) * 100;
  const start = isFirst ? 0 : Math.max(0, center - RULER_TICK_WINDOW);
  const end = isLast ? 100 : Math.min(100, center + RULER_TICK_WINDOW);
  return {
    i,
    range: `${start}% ${end}%`,
    edgeClass: isFirst ? " is-first" : isLast ? " is-last" : "",
  };
});

function runtimePresentation(
  config: AppConfig,
  service: ConfigStatus,
  status: VoiceStatePayload,
) {
  if (!service.provider_ready || status.state === "Error") {
    return { label: "服务不可用", detail: status.state === "Error" ? status.message : "语音服务暂时无法连接", tone: "danger" };
  }
  if (!config.enabled || status.state === "Disabled") {
    return { label: "已暂停", detail: "开启后即可通过快捷键输入", tone: "paused" };
  }
  if (status.state === "Starting") return { label: "正在启动", detail: "正在取得麦克风输入", tone: "active" };
  if (status.state === "Recording") {
    return {
      label: "正在聆听",
      detail: config.shortcut_trigger_mode === "toggle" ? "再次按下快捷键后开始识别" : "松开快捷键后开始识别",
      tone: "active",
    };
  }
  if (status.state === "Transcribing") return { label: "识别中", detail: "正在把语音转换为文字", tone: "active" };
  if (status.state === "Pasting") return { label: "正在输入", detail: "文字即将出现在当前应用", tone: "active" };
  return {
    label: "已就绪",
    detail: config.shortcut_trigger_mode === "toggle" ? "按一下快捷键开始说话" : "按住快捷键开始说话",
    tone: "ready",
  };
}

export function SettingsSidebar({
  open,
  config,
  configStatus,
  voiceStatus,
  shortcutView,

  optionPool,
  optionSaving,
  optionSavingMap,
  optionErrors,
  polishSaving,
  polishError,
  triggerModeSaving,
  triggerModeError,
  enabledSaving,
  enabledError,
  menuRef,
  personalizationRef,
  moreSettingsRef,
  onClose,
  onEnabled,
  onShortcutCapture,
  onShortcutCancel,
  onShortcutKeyDown,
  onShortcutKeyUp,
  onOption,
  onPolishLevel,
  onTriggerMode,
  onLaunch,
}: {
  open: boolean;
  config: AppConfig;
  configStatus: ConfigStatus;
  voiceStatus: VoiceStatePayload;
  shortcutView: ShortcutBindingViewModel;

  optionPool: AsrOptionPool | null;
  optionSaving: boolean;
  optionSavingMap: Record<string, boolean>;
  optionErrors: Record<string, string>;
  polishSaving: boolean;
  polishError: string;
  triggerModeSaving: boolean;
  triggerModeError: string;
  enabledSaving: boolean;
  enabledError: string;
  menuRef: RefObject<HTMLButtonElement>;
  personalizationRef: RefObject<HTMLButtonElement>;
  moreSettingsRef: RefObject<HTMLButtonElement>;
  onClose: () => void;
  onEnabled: (enabled: boolean) => void;
  onShortcutCapture: () => void;
  onShortcutCancel: (source?: string) => void;
  onShortcutKeyDown: (event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) => void;
  onShortcutKeyUp: (event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) => void;
  onOption: (optionId: string, value: ConfigValue) => void;
  onPolishLevel: (level: PolishLevel) => void;
  onTriggerMode: (mode: ShortcutTriggerMode) => void;
  onLaunch: (panel: "personalization" | "more_settings") => void;
}) {
  const runtime = runtimePresentation(config, configStatus, voiceStatus);
  const voiceControlsLocked = ["Starting", "Recording", "Transcribing", "Pasting"].includes(
    voiceStatus.state,
  );
  return (
    <aside id="config-drawer" className={"settings-sidebar " + (open ? "is-open" : "")} aria-hidden={!open}>
      <header className="settings-sidebar-header">
        <div>
          <p className="drawer-kicker">Zephyr / 语音输入</p>
          <h1>语音输入</h1>
        </div>
        <button type="button" className="icon-button" aria-label="关闭设置" onClick={onClose}>
          <span aria-hidden="true">×</span>
        </button>
      </header>

      <div className="settings-sidebar-scroll-wrap">
        <div className="settings-sidebar-scroll">
          <section className={"voice-overview-card " + runtime.tone} aria-live="polite">
            <div className="voice-overview-row">
              <div className="voice-overview-heading">
                <span className="runtime-status-indicator" aria-hidden="true" />
                <strong>语音输入</strong>
                <span className="voice-overview-state">{runtime.label}</span>
              </div>
              <BehaviorSwitch
                compact
                label="语音输入"
                description={config.enabled ? "在任意应用中使用快捷键输入" : "当前不会监听快捷键"}
                checked={config.enabled}
                disabled={enabledSaving}
                onChange={onEnabled}
              />
            </div>
            <p className="voice-overview-detail">{runtime.detail}</p>
            {enabledError ? <p className="field-error" role="alert">{enabledError}</p> : null}
          </section>

          <section className="shortcut-config-card">
            <ShortcutCaptureField
              view={shortcutView}
              onStart={onShortcutCapture}
              onCancel={onShortcutCancel}
              onKeyDown={onShortcutKeyDown}
              onKeyUp={onShortcutKeyUp}
              mode={config.shortcut_trigger_mode}
              disabled={voiceControlsLocked}
              disabledReason={voiceControlsLocked ? "本次语音结束后可修改快捷键。" : ""}
            />

            <ShortcutTriggerModeField
              value={config.shortcut_trigger_mode}
              saving={triggerModeSaving}
              disabled={voiceControlsLocked}
              error={triggerModeError}
              onChange={onTriggerMode}
            />
          </section>

          <OptionPoolRenderer
            pool={optionPool}
            saving={optionSaving}
            savingOptions={optionSavingMap}
            errors={optionErrors}
            onChange={onOption}
          >
            <PolishLevelSetting
              value={config.polish_level}
              saving={polishSaving}
              error={polishError}
              onChange={onPolishLevel}
            />
          </OptionPoolRenderer>

          <section className="launch-section" aria-labelledby="launch-title">
            <div className="section-heading">
              <h2 id="launch-title">按你的方式使用</h2>
              <p>词库、习惯和服务设置集中管理</p>
            </div>
            <div className="launch-card-grid">
              <button
                ref={personalizationRef}
                type="button"
                className="launch-card"
                onClick={() => onLaunch("personalization")}
              >
                <span className="launch-icon" aria-hidden="true">Aa</span>
                <strong>个性化</strong>
                <small>词库、表达习惯与历史</small>
                <span className="launch-arrow" aria-hidden="true">↗</span>
              </button>
              <button
                ref={moreSettingsRef}
                type="button"
                className="launch-card"
                onClick={() => onLaunch("more_settings")}
              >
                <span className="launch-icon" aria-hidden="true">···</span>
                <strong>更多设置</strong>
                <small>服务、兼容性与隐私</small>
                <span className="launch-arrow" aria-hidden="true">↗</span>
              </button>
            </div>
          </section>
        </div>
        <div className="settings-sidebar-ruler" aria-hidden="true">
          {RULER_TICKS.map((tick) => (
            <span
              key={tick.i}
              className={"settings-sidebar-ruler-tick" + tick.edgeClass}
              style={{ "--tick-range": tick.range } as JSX.CSSProperties}
            />
          ))}
        </div>
      </div>
    </aside>
  );
}
