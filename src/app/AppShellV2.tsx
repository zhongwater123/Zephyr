import type { JSX } from "preact";
import { lazy, Suspense } from "preact/compat";
import { useEffect, useRef, useState } from "preact/hooks";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  defaultConfig,
  defaultConfigStatus,
  type AppConfig,
  type ConfigStatus,
  type PolishLevel,
  type ShortcutTriggerMode,
  type EndpointPurpose,
  type VoiceStatePayload,
} from "../domain";
import {
  commandErrorMessage,
  conflictConfig,
  endpointIsTrusted,
  reconciliationCommittedRevision,
} from "../security-model";
import { configApi, hotwordApi, providerApi, sessionApi } from "../ipc/client";
import { useRevisionedConfigMutation } from "./useRevisionedConfigMutation";
import { useAsrOptionPool } from "../features/settings/useAsrOptionPool";
import { SettingsSidebar } from "../features/settings/SettingsSidebar";
import { ModalShell, type SettingsPanel } from "../features/settings/ModalShell";
import {
  PersonalizationPanel,
  type PersonalizationTab,
} from "../features/settings/PersonalizationPanel";
import {
  MoreSettingsPanel,
  type MoreSettingsSection,
} from "../features/settings/MoreSettingsPanel";
import { useHistoryController } from "../features/history/useHistoryController";
import { useShortcutBindingController } from "../features/shortcut/useShortcutBindingController";
import { useHotwordController } from "../features/hotwords/useHotwordController";
import { usePendingOutputs } from "../features/pending/usePendingOutputs";

const ZephyrAsciiField = lazy(() =>
  import("../ZephyrAsciiField").then(({ ZephyrAsciiField }) => ({ default: ZephyrAsciiField })),
);

const currentWindow = getCurrentWindow();
function normalizePolishLevel(value: number | undefined): PolishLevel {
  return value === 0 || value === 1 || value === 3 ? value : 2;
}


function normalizeConfig(next: AppConfig): AppConfig {
  return {
    ...next,
    schema_version: next.schema_version ?? 10,
    revision: next.revision ?? 0,
    trusted_endpoints: next.trusted_endpoints ?? [],
    injection_overrides: next.injection_overrides ?? [],
    history_enabled: next.history_enabled ?? true,
    incident_recovery_enabled: next.incident_recovery_enabled ?? false,
    incident_consent_version: next.incident_consent_version ?? 0,
    incident_save_failed_audio: next.incident_save_failed_audio ?? true,
    incident_save_failed_text: next.incident_save_failed_text ?? true,
    incident_retention_days: next.incident_retention_days ?? 7,
    incident_storage_limit_mb: next.incident_storage_limit_mb ?? 512,
    incident_success_rollup_days: next.incident_success_rollup_days ?? 30,
    hotwords_enabled: next.hotwords_enabled ?? true,
    hotword_agent_enabled: next.hotword_agent_enabled ?? false,
    hotword_agent_base_url: next.hotword_agent_base_url || "https://api.deepseek.com",
    hotword_agent_model: next.hotword_agent_model || "deepseek-v4-flash",
    polish_level: normalizePolishLevel(next.polish_level),
    shortcut_trigger_mode: next.shortcut_trigger_mode === "toggle" ? "toggle" : "hold",
    asr: next.asr ?? defaultConfig.asr,
  };
}

export function AppShell() {
  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [configStatus, setConfigStatus] = useState<ConfigStatus>(defaultConfigStatus);
  const [voiceStatus, setVoiceStatus] = useState<VoiceStatePayload>({ state: "Idle", message: "就绪" });
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [activePanel, setActivePanel] = useState<SettingsPanel | null>(null);
  const [personalizationTab, setPersonalizationTab] = useState<PersonalizationTab>("words");
  const [moreSection, setMoreSection] = useState<MoreSettingsSection>("speech");
  const [notice, setNotice] = useState("");
  const [enabledSaving, setEnabledSaving] = useState(false);
  const [enabledError, setEnabledError] = useState("");
  const [historySaving, setHistorySaving] = useState(false);
  const [historyError, setHistoryError] = useState("");
  const [incidentRecoverySaving, setIncidentRecoverySaving] = useState(false);
  const [incidentRecoveryError, setIncidentRecoveryError] = useState("");
  const [organizerSaving, setOrganizerSaving] = useState("");
  const [organizerError, setOrganizerError] = useState("");
  const [organizerBaseUrl, setOrganizerBaseUrl] = useState(defaultConfig.hotword_agent_base_url);
  const [organizerModel, setOrganizerModel] = useState(defaultConfig.hotword_agent_model);
  const [clipboardCompatibilityExe, setClipboardCompatibilityExe] = useState("");
  const [compatibilitySaving, setCompatibilitySaving] = useState(false);
  const [providerTestState, setProviderTestState] = useState("");
  const [polishSaving, setPolishSaving] = useState(false);
  const [polishError, setPolishError] = useState("");
  const [triggerModeSaving, setTriggerModeSaving] = useState(false);
  const [triggerModeError, setTriggerModeError] = useState("");
  const [organizerTestState, setOrganizerTestState] = useState("");

  const menuRef = useRef<HTMLButtonElement>(null);
  const personalizationRef = useRef<HTMLButtonElement>(null);
  const moreSettingsRef = useRef<HTMLButtonElement>(null);

  const {
    pool: asrOptionPool,
    saving: asrOptionSaving,
    load: loadAsrOptionPool,
    savingOptions: asrSavingOptions,
    errors: asrOptionErrors,
    setOption: setAsrOption,
  } = useAsrOptionPool((message) => {
    if (message && !message.includes("已保存")) setNotice(message);
  });

  const configMutation = useRevisionedConfigMutation(setConfig, refreshConfigStatus);
  const {
    pendingOutputs,
    refreshPendingOutputs,
    deliverPendingOutput,
    copyPendingOutput,
    discardPendingOutput,
  } = usePendingOutputs(setNotice);

  const {
    historyQuery,
    setHistoryQuery,
    historyItems,
    selectedHistoryId,
    editingHistoryText,
    setEditingHistoryText,
    historyNotice,
    historyLoading,
    loadHistory,
    selectHistoryItem,
    saveHistoryItem,
    copyHistoryItem,
    deleteHistoryItem,
    clearAllHistory,
  } = useHistoryController();

  const {
    hotwordState,
    newHotwordText,
    setNewHotwordText,
    hotwordEdits,
    setHotwordEdits,
    profileContextText,
    setProfileContextText,
    appContextName,
    setAppContextName,
    appContextText,
    setAppContextText,
    appContextEdits,
    hotwordNotice,
    hotwordLoading,
    refreshHotwordState,
    addHotword,
    updateHotword,
    deleteHotword,
    organizeHotwordsNow,
    saveProfileContext,
    saveAppContext,
    deleteAppContext,
    updateAppContextDraft,
    saveExistingAppContext,
  } = useHotwordController(config, setConfig);

  const {
    shortcutView,
    shortcutToast,
    clearShortcutToast,
    beginShortcutEdit,
    cancelShortcutEdit,
    handleShortcutKeyDown,
    handleShortcutKeyUp,
  } = useShortcutBindingController(config, setConfig, setNotice, configMutation.describeError);

  useEffect(() => {
    void configApi.get()
      .then((next) => {
        const loaded = normalizeConfig(next);
        setConfig(loaded);
        setOrganizerBaseUrl(loaded.hotword_agent_base_url);
        setOrganizerModel(loaded.hotword_agent_model);
      })
      .catch((error) => {
        setConfig((current) => ({ ...current, enabled: false }));
        setNotice("配置读取失败，已保持禁用：" + commandErrorMessage(error));
      });
    void refreshConfigStatus();
    void loadAsrOptionPool();
    void refreshHotwordState();
    void refreshPendingOutputs();
    void sessionApi.getVoiceState().then(setVoiceStatus).catch((error) => {
      setNotice("语音状态读取失败：" + commandErrorMessage(error));
    });

    const unlisten = listen<VoiceStatePayload>("voice_state_changed", (event) => {
      setVoiceStatus(event.payload);
    });
    const unlistenPending = listen("pending_outputs_changed", () => {
      void refreshPendingOutputs();
    });
    return () => {
      void unlisten.then((dispose) => dispose());
      void unlistenPending.then((dispose) => dispose());
    };
  }, []);

  useEffect(() => {
    if (!drawerOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeDrawer();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [drawerOpen]);

  async function refreshConfigStatus() {
    try {
      setConfigStatus(await configApi.getStatus());
    } catch (error) {
      setConfigStatus({
        provider_ready: false,
        provider_message: commandErrorMessage(error),
        recovery_warning: null,
        global_shortcut_supported: true,
      });
    }
  }

  function closeDrawer() {
    void cancelShortcutEdit("focus_lost");
    setDrawerOpen(false);
    window.setTimeout(() => menuRef.current?.focus(), 0);
  }

  function openPanel(panel: SettingsPanel) {
    void cancelShortcutEdit("focus_lost");
    setDrawerOpen(false);
    setActivePanel(panel);
    if (panel === "personalization") {
      setPersonalizationTab("words");
      void refreshHotwordState();
    } else {
      setMoreSection("speech");
      void refreshConfigStatus();
    }
  }

  function closePanel() {
    setActivePanel(null);
    setDrawerOpen(true);
  }

  async function setEnabled(enabled: boolean) {
    const previous = config;
    setEnabledError("");
    setEnabledSaving(true);
    setConfig((current) => ({ ...current, enabled }));
    try {
      const revision = await configApi.setEnabled({ enabled, expectedRevision: previous.revision });
      setConfig((current) => ({ ...current, enabled, revision }));
    } catch (error) {
      const conflict = conflictConfig(error);
      const committedRevision = reconciliationCommittedRevision(error);
      setConfig(
        conflict
          ? normalizeConfig(conflict)
          : committedRevision !== null
            ? { ...previous, enabled, revision: committedRevision }
            : previous,
      );
      setEnabledError(configMutation.describeError(error));
    } finally {
      setEnabledSaving(false);
    }
  }

  async function setHistoryEnabled(enabled: boolean) {
    const previous = config;
    setHistoryError("");
    setHistorySaving(true);
    setConfig((current) => ({ ...current, history_enabled: enabled }));
    try {
      setConfig(normalizeConfig(await configApi.setHistoryEnabled({
        enabled,
        expectedRevision: previous.revision,
      })));
    } catch (error) {
      const conflict = conflictConfig(error);
      setConfig(conflict ? normalizeConfig(conflict) : previous);
      setHistoryError(configMutation.describeError(error));
    } finally {
      setHistorySaving(false);
    }
  }

  async function setIncidentRecoveryEnabled(enabled: boolean) {
    const previous = config;
    setIncidentRecoveryError("");
    setIncidentRecoverySaving(true);
    setConfig((current) => ({ ...current, incident_recovery_enabled: enabled }));
    try {
      setConfig(normalizeConfig(await configApi.setIncidentRecoveryEnabled({
        enabled,
        expectedRevision: previous.revision,
      })));
    } catch (error) {
      const conflict = conflictConfig(error);
      setConfig(conflict ? normalizeConfig(conflict) : previous);
      setIncidentRecoveryError(configMutation.describeError(error));
    } finally {
      setIncidentRecoverySaving(false);
    }
  }

  async function saveOrganizerSettings(
    field: "enabled" | "base_url" | "model",
    overrides: Partial<Pick<AppConfig, "hotwords_enabled" | "hotword_agent_enabled" | "hotword_agent_base_url" | "hotword_agent_model">> = {},
  ) {
    const previous = config;
    const next = {
      ...config,
      hotword_agent_base_url: organizerBaseUrl,
      hotword_agent_model: organizerModel,
      ...overrides,
    };
    setOrganizerError("");
    setOrganizerSaving(field);
    setConfig(next);
    try {
      let expectedRevision = previous.revision;
      if (
        field === "base_url" &&
        !endpointIsTrusted(previous, next.hotword_agent_base_url, "hotword_agent")
      ) {
        const authorized = await configApi.authorizeEndpoint({
          endpoint: next.hotword_agent_base_url,
          purpose: "hotword_agent",
          expectedRevision,
        });
        expectedRevision = authorized.revision;
        setConfig(authorized);
      }
      await hotwordApi.saveSettings({
        settings: {
          hotwords_enabled: next.hotwords_enabled,
          hotword_agent_enabled: next.hotword_agent_enabled,
          hotword_agent_base_url: next.hotword_agent_base_url,
          hotword_agent_model: next.hotword_agent_model,
        },
        expectedRevision,
      });
      const saved = normalizeConfig(await configApi.get());
      setConfig(saved);
      setOrganizerBaseUrl(saved.hotword_agent_base_url);
      setOrganizerModel(saved.hotword_agent_model);
      await refreshHotwordState();
    } catch (error) {
      const conflict = conflictConfig(error);
      const restored = conflict ? normalizeConfig(conflict) : previous;
      setConfig(restored);
      setOrganizerBaseUrl(restored.hotword_agent_base_url);
      setOrganizerModel(restored.hotword_agent_model);
      setOrganizerError(configMutation.describeError(error));
    } finally {
      setOrganizerSaving("");
    }
  }

  async function setHotwordsEnabled(enabled: boolean) {
    await saveOrganizerSettings("enabled", { hotwords_enabled: enabled });
  }

  async function setOrganizerEnabled(enabled: boolean) {
    await saveOrganizerSettings("enabled", { hotword_agent_enabled: enabled });
  }

  async function setClipboardCompatibility(executableName: string, enabled: boolean) {
    const candidate = executableName.trim();
    if (!candidate) return;
    setCompatibilitySaving(true);
    setNotice("");
    try {
      const saved = await configApi.setClipboardCompatibility({
        executableName: candidate,
        enabled,
        expectedRevision: config.revision,
      });
      setConfig(normalizeConfig(saved));
      if (enabled) setClipboardCompatibilityExe("");
    } catch (error) {
      setNotice(configMutation.describeError(error));
    } finally {
      setCompatibilitySaving(false);
    }
  }

  async function savePolishLevel(level: PolishLevel) {
    if (polishSaving || level === config.polish_level) return;
    const previous = config;
    const next = { ...config, polish_level: level };
    setPolishError("");
    setPolishSaving(true);
    setConfig(next);
    try {
      const saved = normalizeConfig(await configApi.save({
        config: next,
        expectedRevision: previous.revision,
      }));
      setConfig(saved);
    } catch (error) {
      const conflict = conflictConfig(error);
      setConfig(conflict ? normalizeConfig(conflict) : previous);
      setPolishError(configMutation.describeError(error));
    } finally {
      setPolishSaving(false);
    }
  }

  async function saveShortcutTriggerMode(mode: ShortcutTriggerMode) {
    if (triggerModeSaving || mode === config.shortcut_trigger_mode) return;
    const previous = config;
    setTriggerModeError("");
    setTriggerModeSaving(true);
    setConfig((current) => ({ ...current, shortcut_trigger_mode: mode }));
    try {
      const saved = normalizeConfig(await configApi.setShortcutTriggerMode({
        mode,
        expectedRevision: previous.revision,
      }));
      setConfig(saved);
    } catch (error) {
      const conflict = conflictConfig(error);
      if (conflict) {
        setConfig(normalizeConfig(conflict));
      } else {
        try {
          setConfig(normalizeConfig(await configApi.get()));
        } catch {
          setConfig(previous);
        }
      }
      setTriggerModeError(configMutation.describeError(error));
    } finally {
      setTriggerModeSaving(false);
    }
  }

  async function revokeTrustedEndpoint(origin: string, purpose: EndpointPurpose) {
    try {
      setConfig(normalizeConfig(await configApi.revokeEndpoint({
        endpoint: origin,
        purpose,
        expectedRevision: config.revision,
      })));
    } catch (error) {
      setNotice(configMutation.describeError(error));
    }
  }

  async function runProviderTest() {
    setProviderTestState("testing");
    try {
      setProviderTestState(await providerApi.test());
      await refreshConfigStatus();
    } catch (error) {
      setProviderTestState(commandErrorMessage(error));
    }
  }

  async function runOrganizerTest() {
    setOrganizerTestState("testing");
    try {
      setOrganizerTestState(await hotwordApi.testAgent());
      await refreshHotwordState();
    } catch (error) {
      setOrganizerTestState(commandErrorMessage(error));
    }
  }

  async function promoteAgentWord(word: string) {
    try {
      await hotwordApi.promoteAgent(word);
      await refreshHotwordState();
    } catch (error) {
      setNotice(commandErrorMessage(error));
    }
  }

  async function deleteAgentWord(word: string) {
    try {
      await hotwordApi.deleteAgent(word);
      await refreshHotwordState();
    } catch (error) {
      setNotice(commandErrorMessage(error));
    }
  }

  function selectPersonalizationTab(tab: PersonalizationTab) {
    setPersonalizationTab(tab);
    if (tab === "history") void loadHistory("");
  }

  function openOrganizerSettings() {
    setMoreSection("organizer");
    setActivePanel("more_settings");
  }

  async function copyDiagnostics() {
    const details = [
      "Zephyr v0.1.2",
      "语音服务：" + configStatus.provider_message,
      "运行状态：" + voiceStatus.state + " / " + voiceStatus.message,
      "智能整理：" + (hotwordState?.last_error || "无最近错误"),
      "配置 revision：" + config.revision,
    ].join("\n");
    try {
      await navigator.clipboard.writeText(details);
      setNotice("诊断信息已复制。");
    } catch (error) {
      setNotice("复制失败：" + commandErrorMessage(error));
    }
  }

  function startWindowDrag(event: JSX.TargetedPointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    void currentWindow.startDragging();
  }

  function toggleWindowMaximize(event: JSX.TargetedMouseEvent<HTMLDivElement>) {
    if (event.detail === 2) void currentWindow.toggleMaximize();
  }

  const overlayOpen = drawerOpen || activePanel !== null;

  return (
    <main className={"zephyr-app zephyr-v2 " + (overlayOpen ? "surface-open" : "")}>
      <div className="window-drag-strip" onPointerDown={startWindowDrag} onClick={toggleWindowMaximize} />
      <div className="window-controls" aria-label="窗口控制">
        <button type="button" aria-label="最小化" onClick={() => void currentWindow.minimize()}><span className="window-icon minimize" /></button>
        <button type="button" aria-label="最大化或还原" onClick={() => void currentWindow.toggleMaximize()}><span className="window-icon maximize" /></button>
        <button type="button" aria-label="关闭" onClick={() => void currentWindow.close()}><span className="window-icon close" /></button>
      </div>

      <section className="zephyr-stage" onClick={() => drawerOpen && closeDrawer()}>
        <Suspense fallback={null}>
          <ZephyrAsciiField
            state={voiceStatus.state}
            muted={overlayOpen}
            shortcut={config.shortcut}
            triggerMode={config.shortcut_trigger_mode}
          />
        </Suspense>
        <button
          ref={menuRef}
          type="button"
          className="config-toggle"
          aria-label="打开语音输入设置"
          aria-expanded={drawerOpen}
          aria-controls="config-drawer"
          onClick={(event) => {
            event.stopPropagation();
            if (drawerOpen) closeDrawer();
            else setDrawerOpen(true);
          }}
        >
          <svg className="menu-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7h14M5 12h14M5 17h14" /></svg>
        </button>
      </section>

      <SettingsSidebar
        open={drawerOpen}
        config={config}
        configStatus={configStatus}
        voiceStatus={voiceStatus}
        shortcutView={shortcutView}
        optionPool={asrOptionPool}
        optionSaving={asrOptionSaving}
        optionSavingMap={asrSavingOptions}
        optionErrors={asrOptionErrors}
        polishSaving={polishSaving}
        polishError={polishError}
        triggerModeSaving={triggerModeSaving}
        triggerModeError={triggerModeError}
        enabledSaving={enabledSaving}
        enabledError={enabledError}
        menuRef={menuRef}
        personalizationRef={personalizationRef}
        moreSettingsRef={moreSettingsRef}
        onClose={closeDrawer}
        onEnabled={(enabled) => void setEnabled(enabled)}
        onShortcutCapture={beginShortcutEdit}
        onShortcutCancel={(source) => void cancelShortcutEdit(source)}
        onShortcutKeyDown={handleShortcutKeyDown}
        onShortcutKeyUp={handleShortcutKeyUp}
        onOption={(optionId, value) => {
          void setAsrOption(optionId, value).then(() => configApi.get().then((next) => setConfig(normalizeConfig(next))));
        }}
        onPolishLevel={(level) => void savePolishLevel(level)}
        onTriggerMode={(mode) => void saveShortcutTriggerMode(mode)}
        onLaunch={openPanel}
      />

      {activePanel === "personalization" ? (
        <ModalShell
          panel="personalization"
          eyebrow="Zephyr / 个性化"
          title="让输入更懂你"
          restoreFocus={personalizationRef}
          onClose={closePanel}
        >
          <PersonalizationPanel
            tab={personalizationTab}
            state={hotwordState}
            loading={hotwordLoading || organizerSaving !== ""}
            notice={hotwordNotice || notice}
            newWord={newHotwordText}
            edits={hotwordEdits}
            profileText={profileContextText}
            appName={appContextName}
            appText={appContextText}
            appEdits={appContextEdits}
            historyEnabled={config.history_enabled}
            historySaving={historySaving}
            historyError={historyError}
            historyQuery={historyQuery}
            historyItems={historyItems}
            selectedHistoryId={selectedHistoryId}
            editingHistoryText={editingHistoryText}
            historyNotice={historyNotice}
            historyLoading={historyLoading}
            onTab={selectPersonalizationTab}
            onHistoryEnabled={(enabled) => void setHistoryEnabled(enabled)}
            onHotwordsEnabled={(enabled) => void setHotwordsEnabled(enabled)}
            onOrganizerEnabled={(enabled) => void setOrganizerEnabled(enabled)}
            onOpenServiceSettings={openOrganizerSettings}
            onRefreshWords={() => void refreshHotwordState()}
            onOrganize={() => void organizeHotwordsNow()}
            onNewWord={setNewHotwordText}
            onAddWord={() => void addHotword()}
            onEditWord={(word, value) => setHotwordEdits((current) => ({ ...current, [word]: value }))}
            onUpdateWord={(word) => void updateHotword(word)}
            onDeleteWord={(word) => void deleteHotword(word)}
            onPromoteAgentWord={(word) => void promoteAgentWord(word)}
            onDeleteAgentWord={(word) => void deleteAgentWord(word)}
            onProfileText={setProfileContextText}
            onSaveProfile={() => void saveProfileContext()}
            onAppName={setAppContextName}
            onAppText={setAppContextText}
            onSaveApp={() => void saveAppContext()}
            onAppDraft={updateAppContextDraft}
            onSaveExistingApp={(appName) => void saveExistingAppContext(appName)}
            onDeleteApp={(appName) => void deleteAppContext(appName)}
            onHistoryQuery={setHistoryQuery}
            onLoadHistory={() => void loadHistory()}
            onSelectHistory={selectHistoryItem}
            onEditingHistoryText={setEditingHistoryText}
            onSaveHistory={() => void saveHistoryItem()}
            onCopyHistory={() => void copyHistoryItem()}
            onDeleteHistory={() => void deleteHistoryItem()}
            onClearHistory={() => void clearAllHistory()}
          />
        </ModalShell>
      ) : null}

      {activePanel === "more_settings" ? (
        <ModalShell
          panel="more_settings"
          eyebrow="Zephyr / 更多设置"
          title="设置"
          restoreFocus={moreSettingsRef}
          onClose={closePanel}
        >
          <MoreSettingsPanel
            section={moreSection}
            config={{ ...config, hotword_agent_base_url: organizerBaseUrl, hotword_agent_model: organizerModel }}
            configStatus={configStatus}
            providerName={asrOptionPool?.providerDisplayName || "语音识别服务"}
            hotwordState={hotwordState}
            organizerSaving={organizerSaving}
            organizerError={organizerError}
            compatibilityExe={clipboardCompatibilityExe}
            compatibilitySaving={compatibilitySaving}
            historySaving={historySaving}
            historyError={historyError}
            incidentRecoverySaving={incidentRecoverySaving}
            incidentRecoveryError={incidentRecoveryError}
            diagnosticMessage={notice}
            providerTestState={providerTestState}
            organizerTestState={organizerTestState}
            pendingOutputs={pendingOutputs}
            onSection={setMoreSection}
            onProviderTest={() => void runProviderTest()}
            onOrganizerTest={() => void runOrganizerTest()}
            onOrganizerEnabled={(enabled) => void setOrganizerEnabled(enabled)}
            onOrganizerBaseUrl={setOrganizerBaseUrl}
            onOrganizerModel={setOrganizerModel}
            onOrganizerBaseUrlCommit={() => void saveOrganizerSettings("base_url")}
            onOrganizerModelCommit={() => void saveOrganizerSettings("model")}
            onCompatibilityExe={setClipboardCompatibilityExe}
            onAddCompatibility={() => void setClipboardCompatibility(clipboardCompatibilityExe, true)}
            onRemoveCompatibility={(name) => void setClipboardCompatibility(name, false)}
            onHistoryEnabled={(enabled) => void setHistoryEnabled(enabled)}
            onIncidentRecoveryEnabled={(enabled) => void setIncidentRecoveryEnabled(enabled)}
            onRevokeEndpoint={(origin) => void revokeTrustedEndpoint(origin, "hotword_agent")}
            onCopyDiagnostics={() => void copyDiagnostics()}
            onDeliverPending={(id, confirmUncertain) => void deliverPendingOutput(id, confirmUncertain)}
            onCopyPending={(id) => void copyPendingOutput(id)}
            onDiscardPending={(id) => void discardPendingOutput(id)}
          />
        </ModalShell>
      ) : null}

      {shortcutToast ? (
        <div className="shortcut-error-toast" role="alert">
          <span>{shortcutToast}</span>
          <button type="button" aria-label="关闭快捷键错误提示" onClick={clearShortcutToast}>×</button>
        </div>
      ) : null}
      <div className="sr-only" aria-live="polite">{notice}</div>
    </main>
  );
}
