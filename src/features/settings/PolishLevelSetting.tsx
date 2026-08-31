import type { PolishLevel } from "../../domain";

const LEVELS: Array<{
  value: PolishLevel;
  label: string;
  description: string;
}> = [
  {
    value: 1,
    label: "保留原话",
    description: "只做必要清理，尽量保留你的说法。",
  },
  {
    value: 2,
    label: "自然整理",
    description: "让表达更顺，合适时自动整理要点。",
  },
  {
    value: 3,
    label: "更有条理",
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
  const selected = LEVELS.find((level) => level.value === value) ?? LEVELS[1];

  function selectLevel(nextValue: number) {
    if (nextValue === 1 || nextValue === 2 || nextValue === 3) {
      onChange(nextValue);
    }
  }

  return (
    <div className="polish-setting">
      <div className="polish-setting-heading">
        <strong>智能润色</strong>
        <span>说完后，希望文字整理到什么程度？</span>
      </div>
      <p className="polish-setting-intro">
        把口头表达整理成可以直接发送或使用的文字；原话清楚时不会刻意改写。
      </p>
      <input
        className="polish-range"
        type="range"
        min="1"
        max="3"
        step="1"
        value={value}
        disabled={saving}
        aria-label="文字整理程度"
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
