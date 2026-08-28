import { useState } from "preact/hooks";
import type { Dispatch, StateUpdater } from "preact/hooks";
import type { AppConfig, AppHotwordContext, HotwordState } from "../../domain";
import { configApi, hotwordApi } from "../../ipc/client";
import { mergeHotwords } from "./model";

export function useHotwordController(
  config: AppConfig,
  setConfig: Dispatch<StateUpdater<AppConfig>>,
) {
  const [hotwordOpen, setHotwordOpen] = useState(false);
  const [hotwordState, setHotwordState] = useState<HotwordState | null>(null);
  const [newHotwordText, setNewHotwordText] = useState("");
  const [hotwordEdits, setHotwordEdits] = useState<Record<string, string>>({});
  const [profileContextText, setProfileContextText] = useState("");
  const [appContextName, setAppContextName] = useState("");
  const [appContextText, setAppContextText] = useState("");
  const [appContextEdits, setAppContextEdits] = useState<Record<string, AppHotwordContext>>({});
  const [hotwordNotice, setHotwordNotice] = useState("");
  const [hotwordLoading, setHotwordLoading] = useState(false);

  function applyHotwordState(nextState: HotwordState) {
    setHotwordState(nextState);
    setHotwordEdits(Object.fromEntries(mergeHotwords(nextState).map((word) => [word, word])));
    setProfileContextText(nextState.profile_context);
    setAppContextEdits(
      Object.fromEntries(nextState.app_contexts.map((item) => [item.app_name, { ...item }])),
    );
    setConfig((current) => ({
      ...current,
      hotwords_enabled: nextState.hotwords_enabled,
      hotword_agent_enabled: nextState.hotword_agent_enabled,
      hotword_agent_base_url: nextState.hotword_agent_base_url,
      hotword_agent_model: nextState.hotword_agent_model,
    }));
  }

  async function refreshHotwordState() {
    try {
      applyHotwordState(await hotwordApi.getState());
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function persistHotwordSettings() {
    const nextState = await hotwordApi.saveSettings({
      settings: {
        hotwords_enabled: config.hotwords_enabled,
        hotword_agent_enabled: config.hotword_agent_enabled,
        hotword_agent_base_url: config.hotword_agent_base_url,
        hotword_agent_model: config.hotword_agent_model,
      },
      expectedRevision: config.revision,
    });
    applyHotwordState(nextState);
    setConfig(await configApi.get());
    return nextState;
  }

  async function openHotwordPanel() {
    setHotwordOpen(true);
    await refreshHotwordState();
  }

  async function addHotword() {
    setHotwordNotice("");
    const word = newHotwordText.trim();
    if (!word) {
      setHotwordNotice("请输入热词。");
      return;
    }
    try {
      applyHotwordState(await hotwordApi.add(word));
      setNewHotwordText("");
      setHotwordNotice("热词已添加。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function updateHotword(word: string) {
    setHotwordNotice("");
    const newWord = (hotwordEdits[word] ?? word).trim();
    if (!newWord) {
      setHotwordNotice("热词不能为空。");
      return;
    }
    try {
      applyHotwordState(await hotwordApi.update(word, newWord));
      setHotwordNotice("热词已更新。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function deleteHotword(word: string) {
    setHotwordNotice("");
    try {
      applyHotwordState(await hotwordApi.delete(word));
      setHotwordNotice("热词已删除。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function organizeHotwordsNow() {
    setHotwordNotice("");
    setHotwordLoading(true);
    const pendingBefore = hotwordState?.pending_count ?? 0;
    try {
      await persistHotwordSettings();
      applyHotwordState(await hotwordApi.organize());
      setHotwordNotice(
        pendingBefore > 0
          ? "已完成一轮热词整理。"
          : "没有待整理的历史记录；当前热词库已是最新。",
      );
    } catch (error) {
      setHotwordNotice(String(error));
      await refreshHotwordState();
    } finally {
      setHotwordLoading(false);
    }
  }

  async function saveProfileContext() {
    setHotwordNotice("");
    try {
      applyHotwordState(await hotwordApi.updateProfile(profileContextText));
      setHotwordNotice("个人上下文已保存。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function saveAppContext() {
    setHotwordNotice("");
    try {
      applyHotwordState(await hotwordApi.updateApp(appContextName, appContextText));
      setAppContextName("");
      setAppContextText("");
      setHotwordNotice("应用上下文已保存。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function deleteAppContext(appName: string) {
    setHotwordNotice("");
    try {
      applyHotwordState(await hotwordApi.deleteApp(appName));
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  function updateAppContextDraft(appName: string, context: string) {
    setAppContextEdits((current) => ({
      ...current,
      [appName]: { app_name: appName, context },
    }));
  }

  async function saveExistingAppContext(appName: string) {
    setHotwordNotice("");
    try {
      const context = appContextEdits[appName]?.context ?? "";
      applyHotwordState(await hotwordApi.updateApp(appName, context));
      setHotwordNotice("应用上下文已保存。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  return {
    hotwordOpen,
    setHotwordOpen,
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
    openHotwordPanel,
    addHotword,
    updateHotword,
    deleteHotword,
    organizeHotwordsNow,
    saveProfileContext,
    saveAppContext,
    deleteAppContext,
    updateAppContextDraft,
    saveExistingAppContext,
  };
}
