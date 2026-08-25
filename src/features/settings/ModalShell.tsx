import type { ComponentChildren, RefObject } from "preact";
import { useEffect, useRef } from "preact/hooks";

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

export type SettingsPanel = "personalization" | "more_settings";

export function ModalShell({
  panel,
  title,
  eyebrow,
  restoreFocus,
  onClose,
  children,
}: {
  panel: SettingsPanel;
  title: string;
  eyebrow: string;
  restoreFocus: RefObject<HTMLElement>;
  onClose: () => void;
  children: ComponentChildren;
}) {
  const cardRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  const restoreFocusRef = useRef(restoreFocus);
  closeRef.current = onClose;
  restoreFocusRef.current = restoreFocus;

  useEffect(() => {
    const card = cardRef.current;
    if (!card) return;
    const previous = document.activeElement as HTMLElement | null;
    const first = card.querySelector<HTMLElement>(FOCUSABLE);
    window.setTimeout(() => (first ?? card).focus(), 0);

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        closeRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = Array.from(card.querySelectorAll<HTMLElement>(FOCUSABLE));
      if (!focusable.length) {
        event.preventDefault();
        card.focus();
        return;
      }
      const firstItem = focusable[0];
      const lastItem = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === firstItem) {
        event.preventDefault();
        lastItem.focus();
      } else if (!event.shiftKey && document.activeElement === lastItem) {
        event.preventDefault();
        firstItem.focus();
      }
    };

    card.addEventListener("keydown", onKeyDown);
    return () => {
      card.removeEventListener("keydown", onKeyDown);
      window.setTimeout(() => restoreFocusRef.current.current?.focus() ?? previous?.focus(), 0);
    };
  }, [panel]);

  return (
    <section className="settings-modal-backdrop" onMouseDown={onClose}>
      <div
        ref={cardRef}
        className={"settings-modal-card " + panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby={panel + "-title"}
        tabIndex={-1}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="settings-modal-header">
          <div>
            <p className="drawer-kicker">{eyebrow}</p>
            <h2 id={panel + "-title"}>{title}</h2>
          </div>
          <button type="button" className="icon-button" aria-label={"关闭" + title} onClick={onClose}>
            <span aria-hidden="true">×</span>
          </button>
        </header>
        <div className="settings-modal-body">{children}</div>
      </div>
    </section>
  );
}
