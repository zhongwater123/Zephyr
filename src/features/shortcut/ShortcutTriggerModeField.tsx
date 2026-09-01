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
  return (
    <div className={"shortcut-mode-setting " + (disabled ? "is-locked" : "")}>
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
              disabled={disabled || saving}
              onChange={() => onChange(option.value)}
            />
            <span>{option.label}</span>
          </label>
        ))}
      </div>
      {disabled ? <small className="shortcut-mode-lock">本次语音结束后可修改。</small> : null}
      {error ? <small className="field-error" role="alert">{error}</small> : null}
    </div>
  );
}
