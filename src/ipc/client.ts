import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  AsrOptionPool,
  ConfigValue,
  ConfigStatus,
  EndpointPurpose,
  HistoryItem,
  IncidentHealth,
  IncidentItem,
  HotwordState,
  PendingOutput,
  PreInputPayload,
  ShortcutBinding,
  ShortcutEditOutcome,
  ShortcutEditSession,
  ShortcutEditTraceInput,
  VoiceStatePayload,
} from "../domain";

export const configApi = {
  get: () => invoke<AppConfig>("get_config"),
  getStatus: () => invoke<ConfigStatus>("get_config_status"),
  save: (args: {
    config: AppConfig;
    expectedRevision: number;
  }) => invoke<AppConfig>("save_config", args),
  authorizeEndpoint: (args: {
    endpoint: string;
    purpose: EndpointPurpose;
    expectedRevision: number;
  }) => invoke<AppConfig>("authorize_endpoint", args),
  revokeEndpoint: (args: {
    endpoint: string;
    purpose: EndpointPurpose;
    expectedRevision: number;
  }) => invoke<AppConfig>("revoke_endpoint", args),
  setClipboardCompatibility: (args: {
    executableName: string;
    enabled: boolean;
    expectedRevision: number;
  }) => invoke<AppConfig>("set_clipboard_compatibility", args),
  setEnabled: (args: { enabled: boolean; expectedRevision: number }) =>
    invoke<number>("set_enabled", args),
  setHistoryEnabled: (args: { enabled: boolean; expectedRevision: number }) =>
    invoke<AppConfig>("set_history_enabled", args),
  setIncidentRecoveryEnabled: (args: { enabled: boolean; expectedRevision: number }) =>
    invoke<AppConfig>("set_incident_recovery_enabled", args),
};

export const pendingApi = {
  list: () => invoke<PendingOutput[]>("list_pending_outputs"),
  deliver: (id: string) => invoke<void>("deliver_pending_output", { id }),
  copy: (id: string) => invoke<void>("copy_pending_output", { id }),
  discard: (id: string) => invoke<void>("discard_pending_output", { id }),
};

export const historyApi = {
  list: (args: { query?: string | null; limit: number; offset: number }) =>
    invoke<HistoryItem[]>("list_history", args),
  update: (id: string, text: string) => invoke<void>("update_history", { id, text }),
  copy: (id: string) => invoke<void>("copy_history_text", { id }),
  delete: (id: string) => invoke<void>("delete_history", { id }),
  clear: () => invoke<void>("clear_history"),
};

export const incidentApi = {
  list: (limit = 50, offset = 0) =>
    invoke<IncidentItem[]>("list_incidents", { limit, offset }),
  health: () => invoke<IncidentHealth>("get_incident_health"),
  copyText: (id: string) => invoke<void>("copy_incident_text", { id }),
  audio: async (id: string) =>
    new Uint8Array(await invoke<ArrayBuffer>("get_incident_audio", { id })),
  saveAudio: (id: string, path: string) =>
    invoke<void>("save_incident_audio", { id, path }),
  remove: (id: string) => invoke<void>("delete_incident", { id }),
  setPinned: (id: string, pinned: boolean) =>
    invoke<void>("set_incident_pinned", { id, pinned }),
  report: async (
    id: string,
    options: { includeText: boolean; includeAudio: boolean; includeLogExcerpt: boolean },
  ) => new Uint8Array(await invoke<ArrayBuffer>("export_incident_report", { id, options })),
  saveReport: (
    id: string,
    path: string,
    options: { includeText: boolean; includeAudio: boolean; includeLogExcerpt: boolean },
  ) => invoke<void>("save_incident_report", { id, path, options }),
  recordFrontend: (input: { source: string; code: string; message: string; stack?: string | null }) =>
    invoke<void>("record_frontend_incident", { input }),
};

export const shortcutEditApi = {
  begin: (traceId: string, expectedRevision: number) =>
    invoke<ShortcutEditSession>("begin_shortcut_edit", { traceId, expectedRevision }),
  commit: (
    traceId: string,
    editId: number,
    expectedRevision: number,
    binding: ShortcutBinding,
  ) =>
    invoke<ShortcutEditOutcome>("commit_shortcut_edit", {
      traceId,
      editId,
      expectedRevision,
      binding,
    }),
  cancel: (traceId: string, editId: number) =>
    invoke<ShortcutEditOutcome>("cancel_shortcut_edit", { traceId, editId }),
  trace: (input: ShortcutEditTraceInput) =>
    invoke<void>("record_shortcut_edit_trace", { input }),
};
export const hotwordApi = {
  getState: () => invoke<HotwordState>("get_hotword_state"),
  saveSettings: (args: {
    settings: {
      hotwords_enabled: boolean;
      hotword_agent_enabled: boolean;
      hotword_agent_base_url: string;
      hotword_agent_model: string;
    };
    expectedRevision: number;
  }) => invoke<HotwordState>("save_hotword_settings", args),
  saveManual: (words: string[]) =>
    invoke<HotwordState>("save_manual_hotwords", { words }),
  add: (word: string) => invoke<HotwordState>("add_hotword", { word }),
  update: (oldWord: string, newWord: string) =>
    invoke<HotwordState>("update_hotword", { oldWord, newWord }),
  delete: (word: string) => invoke<HotwordState>("delete_hotword", { word }),
  organize: () => invoke<HotwordState>("organize_hotwords_now"),
  testAgent: () => invoke<string>("test_hotword_agent"),
  deleteAgent: (word: string) =>
    invoke<HotwordState>("delete_agent_hotword", { word }),
  promoteAgent: (word: string) =>
    invoke<HotwordState>("promote_agent_hotword", { word }),
  updateProfile: (text: string) =>
    invoke<HotwordState>("update_profile_context", { text }),
  updateApp: (appName: string, context: string) =>
    invoke<HotwordState>("update_app_context", { appName, context }),
  deleteApp: (appName: string) =>
    invoke<HotwordState>("delete_app_context", { appName }),
};

export const asrApi = {
  getOptionPool: () => invoke<AsrOptionPool>("get_asr_option_pool"),
  setOption: (args: {
    optionId: string;
    value: ConfigValue;
    expectedRevision: number;
  }) => invoke<AsrOptionPool>("set_asr_option", args),
};
export const providerApi = {
  test: () => invoke<string>("test_provider"),
};

export const sessionApi = {
  getVoiceState: () => invoke<VoiceStatePayload>("get_voice_state"),
};

export const preinputApi = {
  getPayload: () => invoke<PreInputPayload | null>("get_preinput_payload"),
};
