import { RoseCurveLoader } from "./RoseCurveLoader";
import { getPreInputStateLabel, getPreInputTextSegments } from "./preInputModel";
import "./preinput.css";
import { usePreInputPayload } from "./usePreInputPayload";

export function PreInputOverlay() {
  const { payload, visible } = usePreInputPayload();
  const text = payload.text || "";
  const segments = getPreInputTextSegments(text, payload.confirmedChars);
  const statusLabel = payload.message || getPreInputStateLabel(payload.state);

  return (
    <section
      className={`preinput-shell${visible ? " visible" : ""}`}
      data-state={payload.state}
      aria-label="语音预输入"
    >
      <div className="preinput-status">
        <span className="preinput-status__dot" aria-hidden="true" />
        <span>{statusLabel}</span>
      </div>
      <div className="preinput-text" role="status" aria-live="polite">
        <RoseCurveLoader compact={Boolean(text)} />
        {text ? (
          <span className="preinput-text__copy">
            {segments.hiddenPrefix ? <span className="preinput-text__prefix">...</span> : null}
            <span className="preinput-text__confirmed">{segments.confirmedText}</span>
            <span>{segments.pendingText}</span>
          </span>
        ) : null}
      </div>
    </section>
  );
}
