import { listen } from "@tauri-apps/api/event";
import type { JSX } from "preact";
import { useEffect, useRef, useState } from "preact/hooks";
import type { Dispatch, StateUpdater } from "preact/hooks";
import type {
  AppConfig,
  ShortcutMode,
  ShortcutPreview,
  ShortcutRuntimeStatus,
} from "../../domain";
import { shortcutApi } from "../../ipc/client";
import { shortcutFromKeyboardEvent } from "./keyboard";

export function useShortcutController(
  config: AppConfig,
  setConfig: Dispatch<StateUpdater<AppConfig>>,
  onNotice: (message: string) => void,
  describeMutationError: (error: unknown) => string,
) {
  const [shortcutOpen, setShortcutOpen] = useState(false);
  const [shortcutDraft, setShortcutDraft] = useState("");
  const [shortcutPreview, setShortcutPreview] = useState<ShortcutPreview | null>(null);
  const [shortcutStatus, setShortcutStatus] = useState<ShortcutRuntimeStatus | null>(null);
  const [shortcutNotice, setShortcutNotice] = useState("");
  const [shortcutChecking, setShortcutChecking] = useState(false);
  const sequenceRef = useRef(0);
  const previewIdRef = useRef<number | null>(null);
  const completingRef = useRef<number | null>(null);
  const pollTimerRef = useRef<number | null>(null);
  const revisionRef = useRef(config.revision);

  revisionRef.current = config.revision;

  function stopPolling() {
    if (pollTimerRef.current !== null) {
      window.clearTimeout(pollTimerRef.current);
      pollTimerRef.current = null;
    }
  }

  function schedulePreviewRefresh(previewId: number, sequence: number) {
    stopPolling();
    pollTimerRef.current = window.setTimeout(async () => {
      if (sequence !== sequenceRef.current || previewIdRef.current !== previewId) return;
      try {
        const preview = await shortcutApi.preview(previewId);
        if (sequence !== sequenceRef.current || previewIdRef.current !== previewId) return;
        await handlePreview(preview, sequence);
      } catch (error) {
        if (sequence === sequenceRef.current && previewIdRef.current === previewId) {
          setShortcutChecking(false);
          setShortcutNotice(describeMutationError(error));
        }
      }
    }, 180);
  }

  async function commitReadyPreview(preview: ShortcutPreview, sequence: number) {
    if (completingRef.current === preview.previewId) return;
    completingRef.current = preview.previewId;
    stopPolling();
    setShortcutChecking(true);
    setShortcutNotice("检查完成，正在启用…");
    try {
      const saved = await shortcutApi.commit(preview.previewId, revisionRef.current);
      if (sequence !== sequenceRef.current) return;
      previewIdRef.current = null;
      setConfig(saved);
      setShortcutStatus(await shortcutApi.status().catch(() => null));
      setShortcutOpen(false);
      setShortcutChecking(false);
      onNotice(saved.shortcut_mode === "exclusive_hook"
        ? `快捷键 ${saved.shortcut} 已以独占模式启用。`
        : `快捷键 ${saved.shortcut} 已以标准模式启用。`);
    } catch (error) {
      if (sequence === sequenceRef.current) {
        setShortcutChecking(false);
        setShortcutNotice(describeMutationError(error));
      }
    } finally {
      if (completingRef.current === preview.previewId) completingRef.current = null;
    }
  }

  async function handlePreview(preview: ShortcutPreview, sequence: number) {
    if (sequence !== sequenceRef.current || previewIdRef.current !== preview.previewId) return;
    setShortcutPreview(preview);
    setShortcutNotice(preview.reason);
    if (preview.state === "reserved_standard" || preview.state === "hook_verified") {
      await commitReadyPreview(preview, sequence);
      return;
    }
    if (preview.state === "awaiting_hook_test") {
      setShortcutChecking(false);
      schedulePreviewRefresh(preview.previewId, sequence);
      return;
    }
    setShortcutChecking(false);
  }

  useEffect(() => {
    void shortcutApi.status().then(setShortcutStatus).catch(() => undefined);
    const previewListener = listen<ShortcutPreview>("shortcut_preview_changed", (event) => {
      if (event.payload.previewId !== previewIdRef.current) return;
      void handlePreview(event.payload, sequenceRef.current);
    });
    const statusListener = listen<ShortcutRuntimeStatus>("shortcut_status_changed", (event) => {
      setShortcutStatus(event.payload);
    });
    return () => {
      sequenceRef.current += 1;
      stopPolling();
      void previewListener.then((dispose) => dispose());
      void statusListener.then((dispose) => dispose());
      void shortcutApi.cancel(previewIdRef.current);
    };
  }, []);

  function cancelCurrentPreview() {
    stopPolling();
    const previewId = previewIdRef.current;
    previewIdRef.current = null;
    completingRef.current = null;
    if (previewId !== null) void shortcutApi.cancel(previewId);
  }

  function openShortcutPanel() {
    sequenceRef.current += 1;
    cancelCurrentPreview();
    setShortcutOpen(true);
    setShortcutDraft("");
    setShortcutPreview(null);
    setShortcutChecking(false);
    setShortcutNotice("选择模式，然后按下要使用的快捷键。");
    void shortcutApi.status().then(setShortcutStatus).catch(() => undefined);
  }

  async function prepareShortcutCandidate(candidate: string, mode: ShortcutMode = "standard") {
    const sequence = ++sequenceRef.current;
    cancelCurrentPreview();
    setShortcutDraft(candidate);
    setShortcutPreview(null);
    setShortcutNotice("正在检查快捷键…");
    setShortcutChecking(true);
    try {
      const preview = await shortcutApi.prepare(candidate, mode);
      if (sequence !== sequenceRef.current) {
        await shortcutApi.cancel(preview.previewId);
        return;
      }
      previewIdRef.current = preview.previewId;
      setShortcutPreview(preview);

      // Re-read once after assigning the id. This reconciles a hook verification
      // event that may have arrived before the prepare IPC promise resolved.
      const current = mode === "exclusive_hook"
        ? await shortcutApi.preview(preview.previewId).catch(() => preview)
        : preview;
      await handlePreview(current, sequence);
    } catch (error) {
      if (sequence === sequenceRef.current) {
        setShortcutChecking(false);
        setShortcutNotice(describeMutationError(error));
      }
    }
  }

  function takeExclusiveControl() {
    if (shortcutDraft) {
      void prepareShortcutCandidate(shortcutDraft, "exclusive_hook");
    }
  }

  function clearShortcutDraft() {
    sequenceRef.current += 1;
    cancelCurrentPreview();
    setShortcutDraft("");
    setShortcutPreview(null);
    setShortcutChecking(false);
    setShortcutNotice("请按下要使用的快捷键。");
  }

  function captureShortcut(event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat) return;
    const hasModifier = event.ctrlKey || event.altKey || event.shiftKey || event.metaKey;
    if (event.key === "Escape" && !hasModifier) {
      closeShortcutPanel();
      return;
    }
    if (event.key === "Backspace" && !hasModifier) {
      clearShortcutDraft();
      return;
    }
    const candidate = shortcutFromKeyboardEvent(event);
    if (candidate) void prepareShortcutCandidate(candidate, "standard");
  }

  function closeShortcutPanel() {
    sequenceRef.current += 1;
    cancelCurrentPreview();
    setShortcutChecking(false);
    setShortcutOpen(false);
  }

  return {
    shortcutOpen,
    shortcutDraft,
    shortcutPreview,
    shortcutStatus,
    shortcutNotice,
    shortcutChecking,
    openShortcutPanel,
    prepareShortcutCandidate,
    captureShortcut,
    closeShortcutPanel,
    takeExclusiveControl,
    clearShortcutDraft,
  };
}
