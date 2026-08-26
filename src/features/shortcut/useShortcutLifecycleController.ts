import { listen } from "@tauri-apps/api/event";
import type { JSX } from "preact";
import { useEffect, useReducer, useRef, useState } from "preact/hooks";
import type { Dispatch, StateUpdater } from "preact/hooks";
import { defaultConfig } from "../../domain";
import type { AppConfig, ShortcutLifecycleSnapshot } from "../../domain";
import { shortcutLifecycleApi } from "../../ipc/client";
import {
  initialShortcutLifecycleState,
  selectShortcutLifecycle,
  shortcutLifecycleReducer,
} from "./shortcutLifecycle";

const RECONCILE_INTERVAL_MS = 250;

function sameBinding(
  left: AppConfig["shortcut_binding"],
  right: AppConfig["shortcut_binding"],
) {
  return JSON.stringify(left ?? null) === JSON.stringify(right ?? null);
}

function operationIsActive(snapshot: ShortcutLifecycleSnapshot | null) {
  const phase = snapshot?.operation?.phase;
  return phase === "starting"
    || phase === "capturing"
    || phase === "validating"
    || phase === "applying";
}

export function useShortcutLifecycleController(
  config: AppConfig,
  setConfig: Dispatch<StateUpdater<AppConfig>>,
  onNotice: (message: string) => void,
  describeMutationError: (error: unknown) => string,
) {
  const [shortcutOpen, setShortcutOpen] = useState(false);
  const [clientState, dispatch] = useReducer(
    shortcutLifecycleReducer,
    initialShortcutLifecycleState,
  );
  const [requestPending, setRequestPending] = useState(false);
  const [transportError, setTransportError] = useState("");
  const snapshotRef = useRef<ShortcutLifecycleSnapshot | null>(null);
  const configRevisionRef = useRef(config.revision);
  const requestGenerationRef = useRef(0);
  const awaitingNewOperationRef = useRef(false);
  const pollTimerRef = useRef<number | null>(null);
  configRevisionRef.current = config.revision;

  const view = selectShortcutLifecycle(clientState.snapshot, config.shortcut);

  function stopPolling() {
    if (pollTimerRef.current !== null) {
      window.clearTimeout(pollTimerRef.current);
      pollTimerRef.current = null;
    }
  }

  function syncConfig(snapshot: ShortcutLifecycleSnapshot) {
    setConfig((current) => {
      if (
        snapshot.configRevision < current.revision
        || (
          snapshot.configRevision === current.revision
          && snapshot.runtime.activeLabel === current.shortcut
          && sameBinding(snapshot.runtime.activeBinding, current.shortcut_binding)
        )
      ) {
        return current;
      }
      return {
        ...current,
        revision: snapshot.configRevision,
        shortcut: snapshot.runtime.activeLabel,
        shortcut_binding: snapshot.runtime.activeBinding,
      };
    });
  }

  function acceptSnapshot(
    snapshot: ShortcutLifecycleSnapshot,
    allowOperationChange = false,
  ) {
    const visibleSnapshot =
      !allowOperationChange
      && (snapshotRef.current?.operation ?? null) === null
      && snapshot.operation
      && !operationIsActive(snapshot)
        ? { ...snapshot, operation: null }
        : snapshot;
    const currentState = { snapshot: snapshotRef.current };
    const nextState = shortcutLifecycleReducer(currentState, {
      type: "snapshot_received",
      snapshot: visibleSnapshot,
      allowOperationChange,
    });
    if (nextState === currentState) return false;
    snapshotRef.current = nextState.snapshot;
    dispatch({
      type: "snapshot_received",
      snapshot: visibleSnapshot,
      allowOperationChange,
    });
    syncConfig(visibleSnapshot);
    if (operationIsActive(visibleSnapshot)) {
      const operationId = visibleSnapshot.operation?.operationId;
      if (operationId !== undefined) startPolling(operationId);
    } else {
      stopPolling();
      const operation = visibleSnapshot.operation;
      if (operation?.phase === "failed") {
        onNotice(operation.message);
      } else if (operation?.phase === "succeeded" && operation.kind !== "capture") {
        onNotice(operation.message);
      }
    }
    return true;
  }

  function startPolling(operationId: number) {
    if (pollTimerRef.current !== null) return;
    const reconcile = async () => {
      pollTimerRef.current = null;
      const trackedOperationId = snapshotRef.current?.operation?.operationId;
      if (trackedOperationId !== operationId || !operationIsActive(snapshotRef.current)) {
        return;
      }
      try {
        const snapshot = await shortcutLifecycleApi.get(operationId);
        acceptSnapshot(snapshot);
      } catch {
        // Tauri Event remains the primary channel; the next query retries transport loss.
      }
      if (
        snapshotRef.current?.operation?.operationId === operationId
        && operationIsActive(snapshotRef.current)
      ) {
        pollTimerRef.current = window.setTimeout(
          () => void reconcile(),
          RECONCILE_INTERVAL_MS,
        );
      }
    };
    pollTimerRef.current = window.setTimeout(
      () => void reconcile(),
      RECONCILE_INTERVAL_MS,
    );
  }

  useEffect(() => {
    void shortcutLifecycleApi
      .get(null)
      .then((snapshot) => acceptSnapshot(snapshot, true))
      .catch((error) => setTransportError(describeMutationError(error)));
    const listener = listen<ShortcutLifecycleSnapshot>(
      "shortcut_lifecycle_changed",
      (event) => {
        const allowOperationChange = awaitingNewOperationRef.current;
        if (acceptSnapshot(event.payload, allowOperationChange)) {
          awaitingNewOperationRef.current = false;
        }
      },
    );
    return () => {
      requestGenerationRef.current += 1;
      awaitingNewOperationRef.current = false;
      stopPolling();
      const operation = snapshotRef.current?.operation;
      if (operation && operationIsActive(snapshotRef.current)) {
        void shortcutLifecycleApi.cancelOperation(operation.operationId);
      }
      void listener.then((dispose) => dispose());
    };
  }, []);

  async function runOperation(
    request: () => Promise<ShortcutLifecycleSnapshot>,
  ) {
    const generation = ++requestGenerationRef.current;
    awaitingNewOperationRef.current = true;
    setRequestPending(true);
    setTransportError("");
    try {
      const snapshot = await request();
      if (requestGenerationRef.current !== generation) {
        if (operationIsActive(snapshot) && snapshot.operation) {
          await shortcutLifecycleApi
            .cancelOperation(snapshot.operation.operationId)
            .catch(() => undefined);
        }
        return;
      }
      awaitingNewOperationRef.current = false;
      acceptSnapshot(snapshot, true);
    } catch (error) {
      if (requestGenerationRef.current !== generation) return;
      awaitingNewOperationRef.current = false;
      setTransportError(describeMutationError(error));
    } finally {
      if (requestGenerationRef.current === generation) {
        setRequestPending(false);
      }
    }
  }

  function beginShortcutCapture() {
    onNotice("");
    return runOperation(() =>
      shortcutLifecycleApi.startCapture(configRevisionRef.current),
    );
  }

  async function cancelShortcutOperation() {
    requestGenerationRef.current += 1;
    awaitingNewOperationRef.current = false;
    stopPolling();
    setRequestPending(false);
    const operation = snapshotRef.current?.operation;
    if (!operation || !operationIsActive(snapshotRef.current)) return true;
    setRequestPending(true);
    setTransportError("");
    try {
      const snapshot = await shortcutLifecycleApi.cancelOperation(
        operation.operationId,
      );
      acceptSnapshot(snapshot);
      return !operationIsActive(snapshot);
    } catch (error) {
      setTransportError(describeMutationError(error));
      return false;
    } finally {
      setRequestPending(false);
    }
  }

  async function closeShortcutSession() {
    const cancelled = await cancelShortcutOperation();
    stopPolling();
    if (cancelled) {
      const closedState = shortcutLifecycleReducer(
        { snapshot: snapshotRef.current },
        { type: "session_closed" },
      );
      snapshotRef.current = closedState.snapshot;
      dispatch({ type: "session_closed" });
      setTransportError("");
    }
    setShortcutOpen(false);
  }

  function openShortcutPanel() {
    setShortcutOpen(true);
    void shortcutLifecycleApi
      .get(null)
      .then((snapshot) => acceptSnapshot(snapshot, true))
      .then(() => beginShortcutCapture())
      .catch((error) => setTransportError(describeMutationError(error)));
  }

  function captureShortcut(event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) {
    if (
      event.key === "Escape"
      && !event.ctrlKey
      && !event.altKey
      && !event.shiftKey
      && !event.metaKey
    ) {
      event.preventDefault();
      event.stopPropagation();
      void closeShortcutSession();
    }
  }

  function restoreDefaultShortcut() {
    return runOperation(() =>
      shortcutLifecycleApi.restoreDefault(configRevisionRef.current),
    );
  }

  function undoShortcut() {
    const operationId = snapshotRef.current?.operation?.operationId;
    if (!operationId) return Promise.resolve();
    return runOperation(() =>
      shortcutLifecycleApi.undo(operationId, configRevisionRef.current),
    );
  }

  return {
    shortcutOpen,
    shortcutLifecycle: clientState.snapshot,
    shortcutView: view,
    shortcutRequestPending: requestPending,
    shortcutTransportError: transportError,
    canRestoreDefault: view.activeLabel !== defaultConfig.shortcut,
    openShortcutPanel,
    beginShortcutCapture,
    cancelShortcutOperation,
    closeShortcutSession,
    captureShortcut,
    retryShortcutCapture: beginShortcutCapture,
    restoreDefaultShortcut,
    undoShortcut,
  };
}
