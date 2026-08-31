import type { ShortcutTriggerMode } from "../../domain";

const OPTIONS: Array<{
  value: ShortcutTriggerMode;
  label: string;
}> = [
  { value: "hold", label: "按住说话" },
  { value: "toggle", label: "点击切换" },
];

export function ShortcutTriggerModeField({
  value,
  saving,
  disabled,
  error,
  onChange,
}: {
  value: ShortcutTriggerMode;
  saving: boolean;
  disabled: boolean;
  error: string;
  onChange: (mode: ShortcutTriggerMode) => void;
}) {
  const locked = disabled || saving;
  return (
    <div className={"shortcut-mode-setting " + (locked ? "is-locked" : "")}>
      <div
        className="shortcut-mode-segmented"
        role="radiogroup"
        aria-label="快捷键触发方式"
      >
        {OPTIONS.map((option) => (
          <label
            key={option.value}
            className={"shortcut-mode-segment " + (value === option.value ? "is-selected" : "")}
          >
            <input
              type="radio"
              name="shortcut-trigger-mode"
              value={option.value}
              checked={value === option.value}
              disabled={locked}
              onChange={() => onChange(option.value)}
            />
            <span>{option.label}</span>
          </label>
        ))}
      </div>
      {disabled ? <small className="shortcut-mode-lock">本次语音结束后可修改。</small> : null}
      {saving ? <small className="shortcut-mode-saving" role="status">正在保存触发方式…</small> : null}
      {error ? <small className="field-error" role="alert">{error}</small> : null}
    </div>
  );
}
