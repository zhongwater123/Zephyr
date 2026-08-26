import type { JSX } from "preact";
import { useEffect, useRef } from "preact/hooks";
import type { ShortcutBindingViewModel } from "./useShortcutBindingController";

const TEXT = {
  title: "语音输入快捷键",
  idleHelp: "按住快捷键开始语音输入，松开后结束。",
  captureHelp: "直接按下新的组合键，主键按下后自动保存。",

  currentLabel: "当前快捷键",
  resetLabel: "点击更改",
  placeholder: "正在录入",
} as const;

export function ShortcutCaptureField({
  view,
  onStart,
  onCancel,
  onKeyDown,
  onKeyUp,
}: {
  view: ShortcutBindingViewModel;
  onStart: () => void;
  onCancel: (source?: string) => void;
  onKeyDown: (event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) => void;
  onKeyUp: (event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) => void;
}) {
  const fieldRef = useRef<HTMLButtonElement>(null);
  const onCancelRef = useRef(onCancel);
  onCancelRef.current = onCancel;
  const suppressClickRef = useRef(false);
  const issue = view.phase === "warning" || view.phase === "error";
  const toneClass =
    view.phase === "warning" ? " is-warning" : view.phase === "error" ? " is-error" : "";
  const keycaps = view.displayLabel
    .split("+")
    .map((key) => key.trim())
    .filter(Boolean);
  const help = view.isCapturing ? TEXT.captureHelp : TEXT.idleHelp;

  function startAndFocus() {
    onStart();
    window.requestAnimationFrame(() => fieldRef.current?.focus());
  }

  function handlePointerDown(event: JSX.TargetedPointerEvent<HTMLButtonElement>) {
    if (view.committing) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    suppressClickRef.current = true;
    if (view.isCapturing) {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
      return;
    }
    startAndFocus();
  }

  function handleClick(event: JSX.TargetedMouseEvent<HTMLButtonElement>) {
    if (suppressClickRef.current) {
      suppressClickRef.current = false;
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    if (view.committing) return;
    if (view.isCapturing) onCancel();
    else startAndFocus();
  }

  function handleFocus() {
    if (view.phase === "idle" || view.phase === "error") onStart();
  }

  function handleBlur() {
    if (view.isCapturing) onCancel("focus_lost");
  }

  useEffect(() => {
    if (!view.isCapturing) return;
    function cancelOnOutsidePointer(event: PointerEvent) {
      const target = event.target;
      if (target instanceof Node && fieldRef.current?.contains(target)) return;
      void onCancelRef.current("focus_lost");
    }
    document.addEventListener("pointerdown", cancelOnOutsidePointer, true);
    return () => document.removeEventListener("pointerdown", cancelOnOutsidePointer, true);
  }, [view.isCapturing]);

  return (
    <section
      className={
        "shortcut-setting"
        + (view.isCapturing ? " is-capturing" : "")
        + (view.committing ? " is-committing" : "")
        + toneClass
      }
    >
      <div className="shortcut-setting-copy">
        <strong>{TEXT.title}</strong>
        <small>{help}</small>
        {view.message ? (
          <span className="shortcut-setting-notice" role={issue ? "alert" : "status"}>
            {view.message}
          </span>
        ) : null}
      </div>

      <button
        ref={fieldRef}
        type="button"
        className="shortcut-key-field"
        aria-label={
          view.isCapturing
            ? "正在录入快捷键，再次点击或按 Escape 取消"
            : view.committing
              ? "正在应用快捷键"
              : TEXT.currentLabel + " " + view.activeLabel + "，" + TEXT.resetLabel
        }
        aria-pressed={view.isCapturing}
        aria-busy={view.committing}
        aria-invalid={issue}
        disabled={view.committing}
        onPointerDown={handlePointerDown}
        onClick={handleClick}
        onFocus={handleFocus}
        onBlur={handleBlur}
        onKeyDown={onKeyDown}
        onKeyUp={onKeyUp}
      >
        {keycaps.length > 0 ? (
          <span className="shortcut-keycaps" aria-live="polite" aria-atomic="true">
            {keycaps.map((key, index) => <kbd key={key + index}>{key}</kbd>)}
          </span>
        ) : (
          <>
            <span className="shortcut-capture-indicator" aria-hidden="true" />
            <span className="shortcut-placeholder">{TEXT.placeholder}</span>
          </>
        )}
      </button>
    </section>
  );
}
