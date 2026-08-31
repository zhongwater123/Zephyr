import type { ShortcutTriggerMode } from "../../domain";

const OPTIONS: Array<{
  value: ShortcutTriggerMode;
  label: string;
  description: string;
}> = [
  {
    value: "hold",
    label: "按住说话",
    description: "按下开始，松开结束",
  },
  {
    value: "toggle",
    label: "按一下开始，再按一下结束",
    description: "松开后继续聆听",
  },
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
    <fieldset className="shortcut-mode-setting" disabled={locked}>
      <legend>快捷键触发方式</legend>
      <p>选择每次语音输入如何开始和结束。</p>
      <div className="shortcut-mode-options">
        {OPTIONS.map((option) => (
          <label
            key={option.value}
            className={"shortcut-mode-option " + (value === option.value ? "is-selected" : "")}
          >
            <input
              type="radio"
              name="shortcut-trigger-mode"
              value={option.value}
              checked={value === option.value}
              disabled={locked}
              onChange={() => onChange(option.value)}
            />
            <span>
              <strong>{option.label}</strong>
              <small>{option.description}</small>
            </span>
          </label>
        ))}
      </div>
      {disabled ? <small className="shortcut-mode-lock">本次语音结束后可修改。</small> : null}
      {saving ? <small className="shortcut-mode-saving" role="status">正在保存触发方式…</small> : null}
      {error ? <small className="field-error" role="alert">{error}</small> : null}
    </fieldset>
  );
}
