export function BehaviorSwitch({
  label,
  description,
  checked,
  onChange,
  disabled = false,
  compact = false,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  compact?: boolean;
}) {
  return (
    <label className={`behavior-switch ${disabled ? "is-disabled" : ""} ${compact ? "is-compact" : ""}`}>
      <span className={`behavior-switch-copy ${compact ? "sr-only" : ""}`}>
        <strong>{label}</strong>
        <small>{description}</small>
      </span>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.currentTarget.checked)}
      />
    </label>
  );
}
