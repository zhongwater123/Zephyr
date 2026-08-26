import type { JSX } from "preact";
import { lazy, Suspense } from "preact/compat";
import { useEffect, useMemo, useState } from "preact/hooks";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  defaultConfig,
  defaultConfigStatus,
  type AppConfig,
  type ConfigStatus,
  type EndpointPurpose,
  type VoiceStatePayload,
} from "../domain";
import {
  commandErrorMessage,
  endpointIsTrusted,
  isOfficialEndpoint,
} from "../security-model";
import {
  configApi,
  hotwordApi,
  providerApi,
  sessionApi,
} from "../ipc/client";
import { useRevisionedConfigMutation } from "./useRevisionedConfigMutation";
import { OptionPoolRenderer } from "../features/settings/OptionPoolRenderer";
import { useAsrOptionPool } from "../features/settings/useAsrOptionPool";
import { HistoryDialog } from "../features/history/HistoryDialog";
import { useHistoryController } from "../features/history/useHistoryController";
import { ShortcutDialog } from "../features/shortcut/ShortcutDialog";
import { useShortcutLifecycleController } from "../features/shortcut/useShortcutLifecycleController";
import { PendingOutputsPanel } from "../features/pending/PendingOutputsPanel";
import { usePendingOutputs } from "../features/pending/usePendingOutputs";
import { HotwordDialog } from "../features/hotwords/HotwordDialog";
import { useHotwordController } from "../features/hotwords/useHotwordController";

const ZephyrAsciiField = lazy(() =>
  import("../ZephyrAsciiField").then(({ ZephyrAsciiField }) => ({ default: ZephyrAsciiField })),
);

const currentWindow = getCurrentWindow();

export function AppShell() {
  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [configStatus, setConfigStatus] = useState<ConfigStatus>(defaultConfigStatus);
  const [status, setStatus] = useState<VoiceStatePayload>({
    state: "Idle",
    message: "就绪",
  });
  const [notice, setNotice] = useState("");
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [hotwordExpanded, setHotwordExpanded] = useState(false);
  const [hotwordApiExpanded, setHotwordApiExpanded] = useState(false);
  const [smokeStatus, setSmokeStatus] = useState<"idle" | "running" | "passed" | "failed">("idle");
  const [clipboardCompatibilityExe, setClipboardCompatibilityExe] = useState("");
  const {
    pool: asrOptionPool,
    saving: asrOptionSaving,
    load: loadAsrOptionPool,
    setOption: setAsrOption,
  } = useAsrOptionPool(setNotice);  const configMutation = useRevisionedConfigMutation(setConfig, refreshConfigStatus);
  const {
    historyOpen, setHistoryOpen, historyQuery, setHistoryQuery, historyItems,
    selectedHistoryId, editingHistoryText, setEditingHistoryText, historyNotice,
    historyLoading, openHistoryPanel, loadHistory, selectHistoryItem, saveHistoryItem,
    copyHistoryItem, deleteHistoryItem, clearAllHistory,
  } = useHistoryController();
  const {
    pendingOutputs, refreshPendingOutputs, deliverPendingOutput,
    copyPendingOutput, discardPendingOutput,
  } = usePendingOutputs(setNotice);
  const {
    hotwordOpen, setHotwordOpen, hotwordState, hotwordApiKey, setHotwordApiKey,
    newHotwordText, setNewHotwordText, hotwordEdits, setHotwordEdits,
    profileContextText, setProfileContextText, appContextName, setAppContextName,
    appContextText, setAppContextText, appContextEdits, hotwordNotice, hotwordLoading,
    refreshHotwordState, openHotwordPanel, addHotword, updateHotword, deleteHotword,
    organizeHotwordsNow, saveProfileContext, saveAppContext, deleteAppContext,
    updateAppContextDraft, saveExistingAppContext,
  } = useHotwordController(config, setConfig);
  const {
    shortcutOpen, shortcutView, shortcutRequestPending, shortcutTransportError,
    canRestoreDefault, openShortcutPanel, captureShortcut, closeShortcutSession,
    retryShortcutCapture, restoreDefaultShortcut,
  } = useShortcutLifecycleController(config, setConfig, setNotice, applyMutationError);

  useEffect(() => {
    configApi.get()
      .then((nextConfig) => {
        setConfig({
          ...nextConfig,
          schema_version: nextConfig.schema_version ?? 6,
          revision: nextConfig.revision ?? 0,
          trusted_endpoints: nextConfig.trusted_endpoints ?? [],
          injection_overrides: nextConfig.injection_overrides ?? [],
          history_enabled: nextConfig.history_enabled ?? true,
          hotwords_enabled: nextConfig.hotwords_enabled ?? true,
          hotword_agent_enabled: nextConfig.hotword_agent_enabled ?? false,
          hotword_agent_base_url: nextConfig.hotword_agent_base_url || "https://api.deepseek.com",
          hotword_agent_model: nextConfig.hotword_agent_model || "deepseek-v4-flash",
          asr: nextConfig.asr ?? defaultConfig.asr,
        });
      })
      .catch((error) => {
        setConfig((current) => ({ ...current, enabled: false }));
        setNotice(`配置读取失败，已保持禁用：${commandErrorMessage(error)}`);
      });
    refreshConfigStatus();
    void loadAsrOptionPool();
    refreshHotwordState();
    refreshPendingOutputs();
    sessionApi.getVoiceState().then(setStatus).catch((error) => {
      setNotice(`语音状态读取失败：${commandErrorMessage(error)}`);
    });

    const unlisten = listen<VoiceStatePayload>("voice_state_changed", (event) => {
      setStatus(event.payload);
    });
    const unlistenPending = listen("pending_outputs_changed", () => {
      void refreshPendingOutputs();
    });

    return () => {
      unlisten.then((dispose) => dispose());
      unlistenPending.then((dispose) => dispose());
    };
  }, []);

  useEffect(() => {
    if (!drawerOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setDrawerOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [drawerOpen]);

  const statusTone = useMemo(() => {
    if (status.state === "Error") return "danger";
    if (status.state === "Recording" || status.state === "Transcribing") return "active";
    return "idle";
  }, [status.state]);
  const hotwordAgentOnline = Boolean(
    hotwordState?.has_hotword_agent_api_key && !hotwordState?.last_error
  );
  const isHotwordReady = !config.hotword_agent_enabled || hotwordAgentOnline;
  const isRuntimeReady = config.enabled && configStatus.provider_ready && isHotwordReady;
  const topStatusMessage =
    status.state === "Idle"
      ? isRuntimeReady
        ? "已就绪"
        : config.enabled
          ? "服务不可用"
          : "已暂停"
      : status.message;

  async function refreshConfigStatus() {
    try {
      const nextStatus = await configApi.getStatus();
      setConfigStatus(nextStatus);
    } catch (error) {
      setConfigStatus({
        provider_ready: false,
        provider_message: commandErrorMessage(error),
        recovery_warning: null,
      });
    }
  }

  function applyMutationError(error: unknown): string {
    return configMutation.describeError(error);
  }

  async function saveConfig() {
    setNotice("");
    const sequence = configMutation.begin();
    try {
      const saved = await configApi.save({
        config,
        expectedRevision: config.revision,
        hotwordAgentApiKey: hotwordApiKey || null,
      });
      if (!configMutation.isLatest(sequence)) return;
      setConfig(saved);
      setHotwordApiKey("");
      await refreshConfigStatus();
      await refreshHotwordState();
      setNotice("配置已保存。");
    } catch (error) {
      if (configMutation.isLatest(sequence)) setNotice(applyMutationError(error));
    }
  }

  async function authorizeHotwordEndpoint() {
    setNotice("");
    try {
      const saved = await configApi.authorizeEndpoint({
        endpoint: config.hotword_agent_base_url,
        purpose: "hotword_agent",
        expectedRevision: config.revision,
      });
      setConfig(saved);
      await refreshConfigStatus();
      setNotice("主机授权已保存。后续连接仍会在读取凭据前重新检查。 ");
    } catch (error) {
      setNotice(applyMutationError(error));
    }
  }

  async function revokeTrustedEndpoint(origin: string, purpose: EndpointPurpose) {
    setNotice("");
    try {
      const saved = await configApi.revokeEndpoint({
        endpoint: origin,
        purpose,
        expectedRevision: config.revision,
      });
      setConfig(saved);
      await refreshConfigStatus();
      setNotice("主机授权已撤销；Windows 凭据管理器中的密钥未删除。 ");
    } catch (error) {
      setNotice(applyMutationError(error));
    }
  }

  async function setClipboardCompatibility(executableName: string, enabled: boolean) {
    setNotice("");
    try {
      const saved = await configApi.setClipboardCompatibility({
        executableName,
        enabled,
        expectedRevision: config.revision,
      });
      setConfig(saved);
      if (enabled) setClipboardCompatibilityExe("");
      setNotice(enabled ? "已为该应用启用剪贴板兼容模式。" : "已恢复 Unicode SendInput。 ");
    } catch (error) {
      setNotice(applyMutationError(error));
    }
  }

  async function setEnabled(enabled: boolean) {
    setNotice("");
    try {
      const revision = await configApi.setEnabled({
        enabled,
        expectedRevision: config.revision,
      });
      setConfig((current) => ({ ...current, enabled, revision }));
    } catch (error) {
      setNotice(applyMutationError(error));
    }
  }

  async function runSmokeTest() {
    setNotice("");
    setSmokeStatus("running");
    try {
      const saved = await configApi.save({
        config,
        expectedRevision: config.revision,
        hotwordAgentApiKey: hotwordApiKey || null,
      });
      setConfig(saved);
      setHotwordApiKey("");
      const asrText = await providerApi.test();
      const hotwordText = await hotwordApi.testAgent();
      await refreshConfigStatus();
      await refreshHotwordState();
      setSmokeStatus("passed");
      setNotice(`冒烟测试通过：${asrText}；${hotwordText}`);
    } catch (error) {
      setSmokeStatus("failed");
      setNotice(String(error));
    }
  }


  function startWindowDrag(event: JSX.TargetedPointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    void currentWindow.startDragging();
  }

  function toggleWindowMaximize(event: JSX.TargetedMouseEvent<HTMLDivElement>) {
    if (event.detail === 2) void currentWindow.toggleMaximize();
  }

  return (
    <main className={`zephyr-app ${drawerOpen ? "drawer-open" : ""}`}>
      <div className="window-drag-strip" onPointerDown={startWindowDrag} onClick={toggleWindowMaximize} />
      <div className="window-controls" aria-label="窗口控制">
        <button type="button" aria-label="最小化" onClick={() => void currentWindow.minimize()}>
          <span className="window-icon minimize" aria-hidden="true" />
        </button>
        <button type="button" aria-label="最大化或还原" onClick={() => void currentWindow.toggleMaximize()}>
          <span className="window-icon maximize" aria-hidden="true" />
        </button>
        <button type="button" aria-label="关闭" onClick={() => void currentWindow.close()}>
          <span className="window-icon close" aria-hidden="true" />
        </button>
      </div>
      <section className="zephyr-stage" onClick={() => drawerOpen && setDrawerOpen(false)}>
        <Suspense fallback={null}>
          <ZephyrAsciiField state={status.state} muted={drawerOpen} shortcut={config.shortcut} />
        </Suspense>
        <button
          type="button"
          className="config-toggle"
          aria-label="打开设置"
          aria-expanded={drawerOpen}
          aria-controls="config-drawer"
          onClick={(event) => {
            event.stopPropagation();
            setDrawerOpen((open) => !open);
          }}
        >
          <svg
            className="menu-icon"
            viewBox="0 0 24 24"
            aria-hidden="true"
            focusable="false"
          >
            <path d="M5 7h14M5 12h14M5 17h14" />
          </svg>
        </button>
      </section>

      <aside
        id="config-drawer"
        className="config-drawer"
        aria-hidden={!drawerOpen}
        onClick={(event) => event.stopPropagation()}
      >
        <header className="drawer-header">
          <div>
            <p className="drawer-kicker">Zephyr / 设置</p>
            <h1>云端语音输入</h1>
          </div>
          <button type="button" className="drawer-close" onClick={() => setDrawerOpen(false)}>
            关闭
          </button>
        </header>

        <section className="console-block" aria-live="polite">
          <div className={`status-row ${statusTone}`}>
            <span>状态</span>
            <strong>{topStatusMessage}</strong>
          </div>
          <div className="status-row">
            <span>快捷键</span>
            <button type="button" className="shortcut-inline-button" onClick={openShortcutPanel}>
              <span>{config.shortcut}</span>
              <small>点击编辑</small>
            </button>
          </div>
          {configStatus.recovery_warning ? (
            <p className="config-message">安全恢复：{configStatus.recovery_warning}</p>
          ) : null}
        </section>

        <section className="console-block">
          <div className="console-title with-status">
            <span
              className={"status-dot " + (configStatus.provider_ready ? "online" : "")}
              aria-hidden="true"
            />
            <span>识别服务</span>
          </div>
          <p className="config-message">{configStatus.provider_message}</p>
        </section>

        <OptionPoolRenderer
          pool={asrOptionPool}
          saving={asrOptionSaving}
          onChange={(optionId, value) =>
            void setAsrOption(optionId, value).then(() => configApi.get().then(setConfig))
          }
        />
        <section className="console-block">
          <div className="console-title">历史记录</div>
          <div className="history-settings-row">
            <label className="switch">
              <input
                type="checkbox"
                checked={config.history_enabled}
                onChange={(event) =>
                  setConfig((current) => ({
                    ...current,
                    history_enabled: event.currentTarget.checked,
                  }))
                }
              />
              <span>{config.history_enabled ? "记录历史" : "停止记录"}</span>
            </label>
            <button type="button" className="secondary" onClick={openHistoryPanel}>
              打开历史
            </button>
          </div>
        </section>

        <section className="console-block">
          <button
            type="button"
            className="collapsible-title"
            aria-expanded={hotwordExpanded}
            onClick={() => setHotwordExpanded((expanded) => !expanded)}
          >
            <span className="title-with-status">
              <span
                className={`status-dot ${hotwordAgentOnline ? "online" : ""}`}
                title={hotwordAgentOnline ? "LLM 已配置且最近无错误" : "LLM 未连通或存在最近错误"}
                aria-hidden="true"
              />
              <span>热词与上下文</span>
            </span>
            <small>{hotwordAgentOnline ? "已连通" : "待配置"}</small>
          </button>
          {hotwordExpanded ? (
            <div className="collapsible-content">
              <label className="switch">
                <input
                  type="checkbox"
                  checked={config.hotwords_enabled}
                  onChange={(event) =>
                    setConfig((current) => ({
                      ...current,
                      hotwords_enabled: event.currentTarget.checked,
                    }))
                  }
                />
                <span>{config.hotwords_enabled ? "语音识别时注入热词" : "不注入热词"}</span>
              </label>
              <label className="switch">
                <input
                  type="checkbox"
                  checked={config.hotword_agent_enabled}
                  onChange={(event) =>
                    setConfig((current) => ({
                      ...current,
                      hotword_agent_enabled: event.currentTarget.checked,
                    }))
                  }
                />
                <span>{config.hotword_agent_enabled ? "每 20 条历史自动整理" : "仅手动整理"}</span>
              </label>
              <button
                type="button"
                className="collapsible-subtitle"
                aria-expanded={hotwordApiExpanded}
                onClick={() => setHotwordApiExpanded((expanded) => !expanded)}
              >
                <span>DeepSeek 配置</span>
                <small>{hotwordState?.has_hotword_agent_api_key ? "已保存密钥" : "待填写"}</small>
              </button>
              {hotwordApiExpanded ? (
                <div className="collapsible-content nested">
                  <label>
                    DeepSeek 地址
                    <input
                      value={config.hotword_agent_base_url}
                      onInput={(event) =>
                        setConfig((current) => ({
                          ...current,
                          hotword_agent_base_url: event.currentTarget.value,
                        }))
                      }
                    />
                  </label>
                  <div className="history-settings-row">
                    <span>
                      凭据主机：
                      {endpointIsTrusted(config, config.hotword_agent_base_url, "hotword_agent")
                        ? "已授权"
                        : "待 Windows 原生确认"}
                    </span>
                    {!endpointIsTrusted(
                      config,
                      config.hotword_agent_base_url,
                      "hotword_agent",
                    ) ? (
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => void authorizeHotwordEndpoint()}
                      >
                        授权此主机
                      </button>
                    ) : null}
                  </div>
                  <label>
                    DeepSeek 模型
                    <input
                      value={config.hotword_agent_model}
                      onInput={(event) =>
                        setConfig((current) => ({
                          ...current,
                          hotword_agent_model: event.currentTarget.value,
                        }))
                      }
                    />
                  </label>
                  <label>
                    DeepSeek 密钥
                    <input
                      type="password"
                      value={hotwordApiKey}
                      placeholder={hotwordState?.has_hotword_agent_api_key ? "已保存；留空不变" : "粘贴 DeepSeek API Key"}
                      onInput={(event) => setHotwordApiKey(event.currentTarget.value)}
                    />
                  </label>
                </div>
              ) : null}
              <div className="behavior-grid" aria-label="热词状态">
                <span>待整理</span>
                <strong>{hotwordState?.pending_count ?? 0} 条</strong>
                <span>最近整理</span>
                <strong>{hotwordState?.updated_at || "暂无"}</strong>
              </div>
              <div className="drawer-actions compact">
                <button type="button" className="secondary" onClick={organizeHotwordsNow} disabled={hotwordLoading}>
                  整理热词
                </button>
                <button type="button" className="secondary" onClick={openHotwordPanel}>
                  管理词库
                </button>
              </div>
              {hotwordState?.last_error ? <p className="config-message">最近错误：{hotwordState.last_error}</p> : null}
            </div>
          ) : null}
        </section>

        <section className="console-block">
          <div className="console-title">文本注入</div>
          <p className="config-message">
            默认使用 Unicode SendInput，不读写剪贴板。仅为明确不兼容的旧应用启用剪贴板模式。
          </p>
          <div className="history-settings-row">
            <input
              value={clipboardCompatibilityExe}
              placeholder="legacy.exe"
              onInput={(event) => setClipboardCompatibilityExe(event.currentTarget.value)}
            />
            <button
              type="button"
              className="secondary"
              disabled={!clipboardCompatibilityExe.trim()}
              onClick={() =>
                void setClipboardCompatibility(clipboardCompatibilityExe.trim(), true)
              }
            >
              启用兼容模式
            </button>
          </div>
          {config.injection_overrides.map((entry) => (
            <div className="history-settings-row" key={entry.executable_name}>
              <span>{entry.executable_name} · 剪贴板兼容</span>
              <button
                type="button"
                className="secondary danger"
                onClick={() => void setClipboardCompatibility(entry.executable_name, false)}
              >
                恢复 Unicode
              </button>
            </div>
          ))}
        </section>

        <section className="console-block">
          <div className="console-title">已授权主机</div>
          {config.trusted_endpoints.filter((entry) => !isOfficialEndpoint(entry.origin)).length ? (
            config.trusted_endpoints
              .filter((entry) => !isOfficialEndpoint(entry.origin))
              .map((entry) => (
                <div className="history-settings-row" key={`${entry.purpose}:${entry.origin}`}>
                  <span>
                    热词 · {entry.origin}
                  </span>
                  <button
                    type="button"
                    className="secondary danger"
                    onClick={() => void revokeTrustedEndpoint(entry.origin, entry.purpose)}
                  >
                    撤销
                  </button>
                </div>
              ))
          ) : (
            <p className="config-message">当前只有内置官方主机。</p>
          )}
        </section>

        <PendingOutputsPanel
          outputs={pendingOutputs}
          onDeliver={(id) => void deliverPendingOutput(id)}
          onCopy={(id) => void copyPendingOutput(id)}
          onDiscard={(id) => void discardPendingOutput(id)}
        />

        <footer className="drawer-actions">
          <button type="button" onClick={saveConfig}>
            保存
          </button>
          <button
            type="button"
            className={`secondary ${smokeStatus === "passed" ? "tested" : ""}`}
            onClick={runSmokeTest}
            disabled={smokeStatus === "running"}
          >
            {smokeStatus === "passed"
              ? "✓ 已测试"
              : smokeStatus === "running"
                ? "测试中"
                : smokeStatus === "failed"
                  ? "重新测试"
                  : "冒烟测试"}
          </button>
        </footer>

        {notice ? <p className="notice">{notice}</p> : null}
      </aside>

      <HistoryDialog
        open={historyOpen}
        query={historyQuery}
        items={historyItems}
        selectedId={selectedHistoryId}
        editingText={editingHistoryText}
        notice={historyNotice}
        loading={historyLoading}
        onClose={() => setHistoryOpen(false)}
        onRefresh={() => void loadHistory()}
        onClear={() => void clearAllHistory()}
        onQuery={(query) => {
          setHistoryQuery(query);
          void loadHistory(query);
        }}
        onSelect={selectHistoryItem}
        onEditingText={setEditingHistoryText}
        onSave={() => void saveHistoryItem()}
        onCopy={() => void copyHistoryItem()}
        onDelete={() => void deleteHistoryItem()}
      />

      <ShortcutDialog
        open={shortcutOpen}
        view={shortcutView}
        requestPending={shortcutRequestPending}
        transportError={shortcutTransportError}
        canRestoreDefault={canRestoreDefault}
        onClose={() => void closeShortcutSession()}
        onCapture={captureShortcut}
        onRetry={() => void retryShortcutCapture()}
        onRestoreDefault={() => void restoreDefaultShortcut()}
      />

      <HotwordDialog
        open={hotwordOpen}
        state={hotwordState}
        loading={hotwordLoading}
        notice={hotwordNotice}
        newWord={newHotwordText}
        edits={hotwordEdits}
        profileText={profileContextText}
        appName={appContextName}
        appText={appContextText}
        appEdits={appContextEdits}
        onClose={() => setHotwordOpen(false)}
        onRefresh={() => void refreshHotwordState()}
        onOrganize={() => void organizeHotwordsNow()}
        onNewWord={setNewHotwordText}
        onAdd={() => void addHotword()}
        onEdit={(word, value) => setHotwordEdits((current) => ({ ...current, [word]: value }))}
        onUpdate={(word) => void updateHotword(word)}
        onDelete={(word) => void deleteHotword(word)}
        onProfileText={setProfileContextText}
        onSaveProfile={() => void saveProfileContext()}
        onAppName={setAppContextName}
        onAppText={setAppContextText}
        onSaveApp={() => void saveAppContext()}
        onAppDraft={updateAppContextDraft}
        onSaveExistingApp={(appName) => void saveExistingAppContext(appName)}
        onDeleteApp={(appName) => void deleteAppContext(appName)}
      />
    </main>
  );
}
