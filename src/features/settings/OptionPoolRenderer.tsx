import type { AsrOptionPool, ConfigValue, OptionSpec } from "../../domain";
import { BehaviorSwitch } from "./BehaviorSwitch";

function booleanValue(value: ConfigValue | undefined, fallback: ConfigValue) {
  if (value?.type === "boolean") return value.value;
  return fallback.type === "boolean" ? fallback.value : false;
}

function ToggleOption({
  option,
  pool,
  disabled,
  onChange,
  error,
}: {
  option: OptionSpec;
  pool: AsrOptionPool;
  disabled: boolean;
  onChange: (optionId: string, value: ConfigValue) => void;
  error?: string;
}) {
  const checked = booleanValue(pool.values[option.id], option.defaultValue);
  return (
    <>
      <BehaviorSwitch
        label={option.label}
        description={option.disabledReason || option.description}
        checked={checked}
        disabled={disabled || !option.enabled}
        onChange={(value) => onChange(option.id, { type: "boolean", value })}
      />
      {error ? <p className="field-error" role="alert">{error}</p> : null}
    </>
  );
}

export function OptionPoolRenderer({
  pool,
  saving,
  savingOptions,
  errors,
  onChange,
}: {
  pool: AsrOptionPool | null;
  saving: boolean;
  onChange: (optionId: string, value: ConfigValue) => void;
  savingOptions?: Record<string, boolean>;
  errors?: Record<string, string>;
}) {
  if (!pool) {
    return <p className="config-message">正在加载识别选项…</p>;
  }

  return (
    <section className="console-block">
      <div className="console-title">输入效果</div>
      <div
        className="behavior-switch-list"
        aria-label="输入效果"
      >
        {[...pool.options]
          .sort((left, right) => left.order - right.order)
          .map((option) =>
            option.controlKind === "toggle" ? (
              <ToggleOption
                key={option.id}
                option={option}
                pool={pool}
                disabled={savingOptions ? Boolean(savingOptions[option.id]) : saving}
                error={errors?.[option.id]}
                onChange={onChange}
              />
            ) : (
              <p className="config-message" role="status" key={option.id}>
                “{option.label}”使用了当前版本不支持的控件，已安全禁用。
              </p>
            ),
          )}
      </div>
    </section>
  );
}
