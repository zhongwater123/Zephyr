import { listen } from "@tauri-apps/api/event";
import type { JSX } from "preact";
import { useEffect, useRef, useState } from "preact/hooks";
import type { Dispatch, StateUpdater } from "preact/hooks";
import type {
  AppConfig,
  ShortcutEditInterrupted,
  ShortcutEditOutcome,
  ShortcutEditSession,
  ShortcutEditTraceInput,
  ShortcutErrorCode,
} from "../../domain";
import { configApi, shortcutEditApi } from "../../ipc/client";
import {
  buildMainCandidate,
  buildModifierCandidate,
  isModifierCode,
  modifierLabels,
  orderedModifierCodes,
  type CandidateResult,
  type ShortcutCandidate,
} from "./shortcutCapture";

const MODIFIER_HOLD_MS = 200;
const WARNING_MS = 4_000;
const TOAST_MS = 6_000;

export type ShortcutBindingPhase =
  | "idle"
  | "capturing"
  | "committing"
  | "warning"
  | "error";

export type ShortcutBindingViewModel = {
  phase: ShortcutBindingPhase;
  activeLabel: string;
  displayLabel: string;
  message: string;
  errorCode?: ShortcutErrorCode;
  isCapturing: boolean;
  committing: boolean;
};

type TraceDetails = Partial<Omit<
  ShortcutEditTraceInput,
  "traceId" | "editId" | "eventSeq" | "elapsedMs" | "event"
>>;

function newTraceId() {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return "shortcut-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2);
}

export function useShortcutBindingController(
  config: AppConfig,
  setConfig: Dispatch<StateUpdater<AppConfig>>,
  onNotice: (message: string) => void,
  describeMutationError: (error: unknown) => string,
) {
  const [phase, setPhaseState] = useState<ShortcutBindingPhase>("idle");
  const [displayLabel, setDisplayLabel] = useState(config.shortcut);
  const [message, setMessage] = useState("");
  const [errorCode, setErrorCode] = useState<ShortcutErrorCode | undefined>();
  const [toast, setToast] = useState("");

  const phaseRef = useRef<ShortcutBindingPhase>("idle");
  const displayLabelRef = useRef(config.shortcut);
  const configRef = useRef(config);
  const traceIdRef = useRef<string | null>(null);
  const editIdRef = useRef<number | null>(null);
  const editRevisionRef = useRef<number | null>(null);
  const generationRef = useRef(0);
  const eventSeqRef = useRef(0);
  const startedAtRef = useRef(0);
  const heldModifiersRef = useRef(new Set<string>());
  const seenModifiersRef = useRef(new Set<string>());
  const modifierStartedAtRef = useRef<number | null>(null);
  const mainAttemptedRef = useRef(false);
  const pendingCandidateRef = useRef<ShortcutCandidate | null>(null);
  const commitStartedRef = useRef(false);
  const warningTimerRef = useRef<number | null>(null);
  const toastTimerRef = useRef<number | null>(null);

  configRef.current = config;

  function setPhase(next: ShortcutBindingPhase) {
    phaseRef.current = next;
    setPhaseState(next);
  }

  function setDisplayedLabel(next: string) {
    displayLabelRef.current = next;
    setDisplayLabel(next);
  }

  function clearWarningTimer() {
    if (warningTimerRef.current !== null) {
      window.clearTimeout(warningTimerRef.current);
      warningTimerRef.current = null;
    }
  }

  function clearToastTimer() {
    if (toastTimerRef.current !== null) {
      window.clearTimeout(toastTimerRef.current);
      toastTimerRef.current = null;
    }
  }

  function showFailure(value: string) {
    setToast(value);
    onNotice(value);
    clearToastTimer();
    toastTimerRef.current = window.setTimeout(() => {
      setToast("");
      toastTimerRef.current = null;
    }, TOAST_MS);
  }

  function trace(
    event: ShortcutEditTraceInput["event"],
    details: TraceDetails = {},
    editId: number | null = editIdRef.current,
  ) {
    const traceId = traceIdRef.current;
    if (!traceId) return;
    const input: ShortcutEditTraceInput = {
      traceId,
      editId,
      eventSeq: ++eventSeqRef.current,
      elapsedMs: Math.max(0, Math.round(performance.now() - startedAtRef.current)),
      event,
      heldCodes: orderedModifierCodes(heldModifiersRef.current),
      ...details,
    };
    void shortcutEditApi.trace(input).catch(() => undefined);
  }

  function traceKeyboard(
    name: "dom_keydown" | "dom_keyup",
    event: JSX.TargetedKeyboardEvent<HTMLButtonElement>,
    details: TraceDetails = {},
  ) {
    trace(name, {
      code: event.code,
      key: event.key,
      location: event.location,
      repeat: event.repeat,
      ctrl: event.ctrlKey,
      alt: event.altKey,
      shift: event.shiftKey,
      meta: event.metaKey,
      altGraph: event.getModifierState("AltGraph"),
      heldCodes: orderedModifierCodes(heldModifiersRef.current),
      candidateLabel: displayLabelRef.current,
      ...details,
    });
  }

  function resetCaptureState() {
    clearWarningTimer();
    heldModifiersRef.current.clear();
    seenModifiersRef.current.clear();
    modifierStartedAtRef.current = null;
    mainAttemptedRef.current = false;
    pendingCandidateRef.current = null;
    commitStartedRef.current = false;
    editIdRef.current = null;
    editRevisionRef.current = null;
  }

  function applyAuthoritativeShortcut(
    configRevision: number,
    activeLabel: string,
    activeBinding: ShortcutEditOutcome["activeBinding"],
  ) {
    if (configRevision < configRef.current.revision) {
      setDisplayedLabel(configRef.current.shortcut);
      return;
    }
    const next = {
      ...configRef.current,
      revision: configRevision,
      shortcut: activeLabel,
      shortcut_binding: activeBinding,
    };
    configRef.current = next;
    setConfig((current) => (
      configRevision < current.revision
        ? current
        : {
            ...current,
            revision: configRevision,
            shortcut: activeLabel,
            shortcut_binding: activeBinding,
          }
    ));
    setDisplayedLabel(activeLabel);
  }

  function applyOutcome(outcome: ShortcutEditOutcome) {
    applyAuthoritativeShortcut(
      outcome.configRevision,
      outcome.activeLabel,
      outcome.activeBinding,
    );
  }

  function applySession(session: ShortcutEditSession) {
    applyAuthoritativeShortcut(
      session.configRevision,
      session.activeLabel,
      session.activeBinding,
    );
  }

  function applyAuthoritativeConfig(authoritative: AppConfig) {
    if (authoritative.revision < configRef.current.revision) return;
    configRef.current = authoritative;
    setConfig((current) => (
      authoritative.revision < current.revision ? current : authoritative
    ));
    if (phaseRef.current === "idle" || phaseRef.current === "error") {
      setDisplayedLabel(authoritative.shortcut);
    }
  }

  function rollbackLocally(value: string, code?: ShortcutErrorCode) {
    const current = configRef.current;
    setDisplayedLabel(current.shortcut);
    setMessage(value);
    setErrorCode(code);
    setPhase("error");
    showFailure(value);
  }

  async function reconcileTransportFailure(
    error: unknown,
    traceId: string,
    editId: number | null,
  ) {
    void shortcutEditApi.cancel(traceId, editId ?? 0).catch(() => undefined);
    try {
      const authoritative = await configApi.get();
      applyAuthoritativeConfig(authoritative);
    } catch {
      setDisplayedLabel(configRef.current.shortcut);
    }
    rollbackLocally(describeMutationError(error));
  }

  async function commitCandidate(
    candidate: ShortcutCandidate,
    generation: number,
    traceId: string,
    editId: number,
  ) {
    if (commitStartedRef.current) return;
    commitStartedRef.current = true;
    trace("commit_dispatched", { candidateLabel: candidate.label }, editId);
    try {
      const outcome = await shortcutEditApi.commit(
        traceId,
        editId,
        editRevisionRef.current ?? configRef.current.revision,
        candidate.binding,
      );
      if (generationRef.current !== generation || traceIdRef.current !== traceId) return;
      trace("commit_completed", {
        candidateLabel: candidate.label,
        reasonCode: outcome.errorCode ?? null,
      }, editId);
      editIdRef.current = null;
      pendingCandidateRef.current = null;
      commitStartedRef.current = false;
      applyOutcome(outcome);
      if (outcome.success) {
        setMessage(outcome.runtimeState === "disabled" ? outcome.message : "");
        setErrorCode(undefined);
        setPhase("idle");
        traceIdRef.current = null;
        return;
      }
      trace("optimistic_rollback", {
        candidateLabel: candidate.label,
        reasonCode: outcome.errorCode ?? "hook_unavailable",
      }, editId);
      setMessage(outcome.message);
      setErrorCode(outcome.errorCode);
      setPhase("error");
      showFailure(outcome.message);
      traceIdRef.current = null;
    } catch (error) {
      if (generationRef.current !== generation || traceIdRef.current !== traceId) return;
      trace("optimistic_rollback", {
        candidateLabel: candidate.label,
        reasonCode: "transport_error",
      }, editId);
      editIdRef.current = null;
      pendingCandidateRef.current = null;
      commitStartedRef.current = false;
      await reconcileTransportFailure(error, traceId, editId);
      traceIdRef.current = null;
    }
  }

  function finalizeCandidate(candidate: ShortcutCandidate) {
    pendingCandidateRef.current = candidate;
    setDisplayedLabel(candidate.label);
    setMessage("");
    setErrorCode(undefined);
    setPhase("committing");
    trace("candidate_finalized", {
      candidateLabel: candidate.label,
      candidateBinding: candidate.binding,
    });
    const editId = editIdRef.current;
    const traceId = traceIdRef.current;
    if (editId !== null && traceId) {
      void commitCandidate(candidate, generationRef.current, traceId, editId);
    }
  }

  function rejectCandidate(result: Extract<CandidateResult, { error: unknown }>) {
    setDisplayedLabel(result.label);
    setMessage(result.error.message);
    setErrorCode(result.error.code);
    setPhase("warning");
    trace("candidate_rejected", {
      candidateLabel: result.label,
      reasonCode: result.error.code,
    });
    clearWarningTimer();
    warningTimerRef.current = window.setTimeout(() => {
      if (phaseRef.current === "warning") {
        setMessage("");
        setErrorCode(undefined);
        setPhase("capturing");
      }
      warningTimerRef.current = null;
    }, WARNING_MS);
  }

  function useCandidate(result: CandidateResult) {
    if (result.candidate) finalizeCandidate(result.candidate);
    else rejectCandidate(result);
  }

  function beginShortcutEdit() {
    if (phaseRef.current === "capturing" || phaseRef.current === "warning") return;
    if (phaseRef.current === "committing") return;

    const generation = ++generationRef.current;
    const traceId = newTraceId();
    resetCaptureState();
    traceIdRef.current = traceId;
    eventSeqRef.current = 0;
    startedAtRef.current = performance.now();
    setDisplayedLabel("");
    setMessage("");
    setErrorCode(undefined);
    setToast("");
    setPhase("capturing");
    onNotice("");
    trace("ui_capture_started");

    void shortcutEditApi
      .begin(traceId, configRef.current.revision)
      .then((session) => {
        if (generationRef.current !== generation || traceIdRef.current !== traceId) {
          if (session.editId > 0) {
            void shortcutEditApi.cancel(traceId, session.editId).catch(() => undefined);
          }
          return;
        }
        trace("begin_acknowledged", {
          reasonCode: session.errorCode ?? null,
        }, session.editId || null);
        if (session.errorCode || session.editId === 0) {
          applySession(session);
          setMessage(session.message);
          setErrorCode(session.errorCode);
          setPhase("error");
          showFailure(session.message);
          traceIdRef.current = null;
          return;
        }
        editIdRef.current = session.editId;
        editRevisionRef.current = session.configRevision;
        const pending = pendingCandidateRef.current;
        if (pending) {
          void commitCandidate(pending, generation, traceId, session.editId);
        }
      })
      .catch(async (error) => {
        if (generationRef.current !== generation || traceIdRef.current !== traceId) return;
        await reconcileTransportFailure(error, traceId, null);
        traceIdRef.current = null;
      });
  }

  async function cancelShortcutEdit(source = "cancel_requested") {
    if (phaseRef.current === "committing") return false;
    const active =
      phaseRef.current === "capturing"
      || phaseRef.current === "warning";
    if (!active) return true;
    const traceId = traceIdRef.current;
    const editId = editIdRef.current;
    trace(source === "focus_lost" ? "focus_lost" : "cancel_requested");
    ++generationRef.current;
    resetCaptureState();
    setDisplayedLabel(configRef.current.shortcut);
    setMessage("");
    setErrorCode(undefined);
    setPhase("idle");
    traceIdRef.current = null;
    if (!traceId || editId === null) return true;
    try {
      const outcome = await shortcutEditApi.cancel(traceId, editId);
      applyOutcome(outcome);
      if (!outcome.success) {
        setMessage(outcome.message);
        setErrorCode(outcome.errorCode);
        setPhase("error");
        showFailure(outcome.message);
      }
      return outcome.success;
    } catch (error) {
      await reconcileTransportFailure(error, traceId, editId);
      return false;
    }
  }

  function handleKeyDown(event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) {
    if (phaseRef.current !== "capturing" && phaseRef.current !== "warning") return;
    event.preventDefault();
    event.stopPropagation();

    if (
      event.key === "Escape"
      && !event.ctrlKey
      && !event.altKey
      && !event.shiftKey
      && !event.metaKey
    ) {
      traceKeyboard("dom_keydown", event);
      void cancelShortcutEdit();
      return;
    }
    if (event.repeat) {
      traceKeyboard("dom_keydown", event);
      return;
    }
    if (phaseRef.current === "warning") {
      clearWarningTimer();
      setMessage("");
      setErrorCode(undefined);
      setPhase("capturing");
    }
    if (event.getModifierState("AltGraph") || event.code === "AltRight") {
      heldModifiersRef.current.delete("ControlLeft");
      seenModifiersRef.current.delete("ControlLeft");
    }

    if (isModifierCode(event.code)) {
      if (heldModifiersRef.current.size === 0) {
        modifierStartedAtRef.current = performance.now();
        seenModifiersRef.current.clear();
        mainAttemptedRef.current = false;
      }
      heldModifiersRef.current.add(event.code);
      seenModifiersRef.current.add(event.code);
      setDisplayedLabel(modifierLabels(heldModifiersRef.current).join("+"));
      traceKeyboard("dom_keydown", event);
      return;
    }

    mainAttemptedRef.current = true;
    const result = buildMainCandidate(heldModifiersRef.current, event.code);
    const tracedCandidateLabel = result.candidate
      ? result.candidate.label
      : result.label;
    traceKeyboard("dom_keydown", event, {
      candidateLabel: tracedCandidateLabel,
      candidateBinding: result.candidate?.binding ?? null,
      reasonCode: result.error?.code ?? null,
    });
    useCandidate(result);
    if (result.candidate) event.currentTarget.blur();
  }

  function handleKeyUp(event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) {
    if (phaseRef.current !== "capturing" && phaseRef.current !== "warning") return;
    event.preventDefault();
    event.stopPropagation();
    if (!isModifierCode(event.code)) {
      traceKeyboard("dom_keyup", event);
      return;
    }

    heldModifiersRef.current.delete(event.code);
    traceKeyboard("dom_keyup", event);
    if (heldModifiersRef.current.size !== 0) {
      const visibleModifiers = mainAttemptedRef.current
        ? heldModifiersRef.current
        : seenModifiersRef.current;
      setDisplayedLabel(modifierLabels(visibleModifiers).join("+"));
      return;
    }
    if (mainAttemptedRef.current) {
      mainAttemptedRef.current = false;
      seenModifiersRef.current.clear();
      modifierStartedAtRef.current = null;
      return;
    }

    const heldFor = performance.now() - (modifierStartedAtRef.current ?? performance.now());
    const seen = new Set(seenModifiersRef.current);
    seenModifiersRef.current.clear();
    modifierStartedAtRef.current = null;
    if (heldFor < MODIFIER_HOLD_MS) {
      setDisplayedLabel("");
      return;
    }
    const result = buildModifierCandidate(seen);
    useCandidate(result);
    if (result.candidate) event.currentTarget.blur();
  }

  useEffect(() => {
    if (phaseRef.current === "idle" || phaseRef.current === "error") {
      setDisplayedLabel(config.shortcut);
    }
  }, [config.shortcut]);

  useEffect(() => {
    const listener = listen<ShortcutEditInterrupted>(
      "shortcut_edit_interrupted",
      (event) => {
        const outcome = event.payload.outcome;
        if (
          traceIdRef.current !== outcome.traceId
          || (editIdRef.current !== null && editIdRef.current !== outcome.editId)
        ) {
          return;
        }
        trace("edit_interrupted", {
          reasonCode: outcome.errorCode ?? "hook_interrupted",
        }, outcome.editId);
        ++generationRef.current;
        resetCaptureState();
        applyOutcome(outcome);
        setMessage(outcome.message);
        setErrorCode(outcome.errorCode);
        setPhase("error");
        showFailure(outcome.message);
        traceIdRef.current = null;
        void configApi.get().then(applyAuthoritativeConfig).catch(() => undefined);
      },
    );
    return () => {
      ++generationRef.current;
      clearWarningTimer();
      clearToastTimer();
      const traceId = traceIdRef.current;
      const editId = editIdRef.current;
      if (traceId) {
        void shortcutEditApi.cancel(traceId, editId ?? 0).catch(() => undefined);
      }
      void listener.then((dispose) => dispose()).catch(() => undefined);
    };
  }, []);

  return {
    shortcutView: {
      phase,
      activeLabel: config.shortcut,
      displayLabel,
      message,
      errorCode,
      isCapturing: phase === "capturing" || phase === "warning",
      committing: phase === "committing",
    } satisfies ShortcutBindingViewModel,
    shortcutToast: toast,
    clearShortcutToast: () => {
      clearToastTimer();
      setToast("");
    },
    beginShortcutEdit,
    cancelShortcutEdit,
    handleShortcutKeyDown: handleKeyDown,
    handleShortcutKeyUp: handleKeyUp,
  };
}
