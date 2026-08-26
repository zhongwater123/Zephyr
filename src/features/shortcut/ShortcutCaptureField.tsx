import type { JSX } from "preact";
import { useEffect, useRef } from "preact/hooks";
import type { ShortcutLifecycleViewModel } from "./shortcutLifecycle";

const TEXT = {
  title: "语音输入快捷键",
  idleHelp: "按住快捷键开始语音输入，松开后结束。",
  prepareHelp: "正在准备键盘 Hook，请稍候。",
  captureHelp: "按下新的组合键；输入会实时显示，全部松开后自动验证。",
  processingHelp: "正在验证并应用新的快捷键。",
  currentLabel: "当前快捷键",
  resetLabel: "点击重新设置",
  placeholder: "输入快捷键",
} as const;

export function ShortcutCaptureField({
  view,
  requestPending,
  transportError,
  onStart,
  onCancel,
}: {
  view: ShortcutLifecycleViewModel;
  requestPending: boolean;
  transportError: string;
  onStart: () => void;
  onCancel: () => void;
}) {
  const fieldRef = useRef<HTMLButtonElement>(null);
  const onCancelRef = useRef(onCancel);
  const suppressNextClickRef = useRef(false);
  onCancelRef.current = onCancel;
  const active = view.busy || requestPending;
  const preparing = view.phase === "starting" || (requestPending && !view.busy);
  const capturing = view.phase === "capturing";
  const cancellable = requestPending
    || view.phase === "starting"
    || view.phase === "capturing";
  const locked = view.phase === "validating" || view.phase === "applying";
  const visibleShortcut = preparing ? "" : view.displayLabel;
  const keycaps = visibleShortcut
    .split("+")
    .map((key) => key.trim())
    .filter(Boolean);
  const notice = transportError
    || (preparing ? "正在准备换绑…" : view.message);
  const validationWarning = capturing && !transportError && Boolean(view.errorCode);
  const failure = Boolean(transportError || (view.errorCode && !validationWarning));
  const issue = validationWarning || failure;
  const toneClass = validationWarning ? " is-warning" : failure ? " is-error" : "";
  const help = preparing
    ? TEXT.prepareHelp
    : capturing
      ? TEXT.captureHelp
      : locked
        ? TEXT.processingHelp
        : TEXT.idleHelp;

  function handleKeyDown(event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) {
    if (
      cancellable
      && event.key === "Escape"
      && !event.ctrlKey
      && !event.altKey
      && !event.shiftKey
      && !event.metaKey
    ) {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
    }
  }

  function handlePointerDown(event: JSX.TargetedPointerEvent<HTMLButtonElement>) {
    if (!active) return;
    event.preventDefault();
    event.stopPropagation();
    suppressNextClickRef.current = true;
    if (cancellable) onCancel();
  }

  function handleClick(event: JSX.TargetedMouseEvent<HTMLButtonElement>) {
    if (suppressNextClickRef.current) {
      suppressNextClickRef.current = false;
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    if (active) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    onStart();
  }

  useEffect(() => {
    if (!cancellable) return;
    function cancelOnOutsidePointer(event: PointerEvent) {
      const target = event.target;
      if (target instanceof Node && fieldRef.current?.contains(target)) return;
      onCancelRef.current();
    }
    document.addEventListener("pointerdown", cancelOnOutsidePointer, true);
    return () => document.removeEventListener("pointerdown", cancelOnOutsidePointer, true);
  }, [cancellable]);

  return (
    <section className={`shortcut-setting${preparing ? " is-preparing" : ""}${capturing ? " is-capturing" : ""}${toneClass}`}>
      <div className="shortcut-setting-copy">
        <strong>{TEXT.title}</strong>
        <small>{help}</small>
        {notice ? <span className="shortcut-setting-notice" role={issue ? "alert" : "status"}>{notice}</span> : null}
      </div>

      <button
        ref={fieldRef}
        type="button"
        className="shortcut-key-field"
        aria-label={active
          ? preparing
            ? "正在准备换绑，再次点击或按 Escape 取消"
            : locked
            ? "正在验证并应用快捷键"
            : "正在设置快捷键，再次点击或按 Escape 取消"
          : `${TEXT.currentLabel} ${view.activeLabel}，${TEXT.resetLabel}`}
        aria-pressed={active}
        aria-busy={preparing || locked}
        onPointerDown={handlePointerDown}
        aria-invalid={issue}
        onClick={handleClick}
        onKeyDown={handleKeyDown}
      >
        {visibleShortcut ? (
          <span className="shortcut-keycaps" aria-live="polite" aria-atomic="true">
            {keycaps.map((key) => <kbd key={key}>{key}</kbd>)}
          </span>
        ) : (
          <>
            <span className="shortcut-capture-indicator" aria-hidden="true" />
            <span className="shortcut-placeholder">{preparing ? "正在准备" : TEXT.placeholder}</span>
          </>
        )}
      </button>
    </section>
  );
}
