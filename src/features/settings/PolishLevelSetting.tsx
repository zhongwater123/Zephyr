import type { PolishLevel } from "../../domain";

const LEVELS: Array<{
  value: PolishLevel;
  label: string;
  description: string;
}> = [
  {
    value: 0,
    label: "Fast",
    description: "快速响应，仅识别原话。",
  },
  {
    value: 1,
    label: "轻微整理",
    description: "去掉口头重复和明显语病，尽量保留你的说法。",
  },
  {
    value: 2,
    label: "自然表达",
    description: "让表达更顺，合适时自动整理要点。",
  },
  {
    value: 3,
    label: "理清重点",
    description: "更深入地重组长内容，让重点更清楚。",
  },
];

export function PolishLevelSetting({
  value,
  saving,
  error,
  onChange,
}: {
  value: PolishLevel;
  saving: boolean;
  error: string;
  onChange: (level: PolishLevel) => void;
}) {
  const selected = LEVELS.find((level) => level.value === value) ?? LEVELS[2];

  function selectLevel(nextValue: number) {
    if (nextValue === 0 || nextValue === 1 || nextValue === 2 || nextValue === 3) {
      onChange(nextValue);
    }
  }

  return (
    <div className="polish-setting">
      <div className="polish-setting-heading">
        <strong>智能润色</strong>
        <span>说完后，希望得到怎样的文字？</span>
      </div>
      <p className="polish-setting-intro">
        选择更快直出，或让文字更顺、更有条理。
      </p>
      <input
        className="polish-range"
        type="range"
        min="0"
        max="3"
        step="1"
        value={value}
        disabled={saving}
        aria-label="智能润色输出方式"
        aria-valuetext={selected.label}
        onChange={(event) => selectLevel(Number(event.currentTarget.value))}
      />
      <div className="polish-range-labels" aria-hidden="true">
        {LEVELS.map((level) => (
          <span key={level.value} className={value === level.value ? "is-active" : ""}>
            {level.label}
          </span>
        ))}
      </div>
      <p className="polish-setting-result" aria-live="polite">
        <strong>{selected.label}</strong>
        <span>{selected.description}</span>
      </p>
      {error ? (
        <p className="field-error" role="alert" title={error}>
          暂时没保存成功，请再试一次。
        </p>
      ) : null}
    </div>
  );
}
