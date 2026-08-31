import { useLayoutEffect, useRef } from "preact/hooks";
import { RoseCurveLoader } from "./RoseCurveLoader";
import { getPreInputTextSegments } from "./preInputModel";
import "./preinput.css";
import { usePreInputPayload } from "./usePreInputPayload";

const LINEAR_SCROLL_SPEED_PX_PER_MS = 0.18;
const MAX_FRAME_DELTA_MS = 48;
const TEXT_LINE_HEIGHT_PX = 16;

function getChangedSuffixLength(previousText: string, currentText: string) {
  const previousChars = Array.from(previousText);
  const currentChars = Array.from(currentText);
  let commonPrefixLength = 0;

  while (
    commonPrefixLength < previousChars.length &&
    commonPrefixLength < currentChars.length &&
    previousChars[commonPrefixLength] === currentChars[commonPrefixLength]
  ) {
    commonPrefixLength += 1;
  }

  return currentChars.length - commonPrefixLength;
}

function splitTextTail(text: string, tailLength: number) {
  const chars = Array.from(text);
  const splitAt = Math.max(0, chars.length - tailLength);
  return {
    stableText: chars.slice(0, splitAt).join(""),
    revealText: chars.slice(splitAt).join(""),
  };
}

export function PreInputOverlay() {
  const { payload, visible } = usePreInputPayload();
  const textViewportRef = useRef<HTMLSpanElement>(null);
  const scrollFrameRef = useRef<number | null>(null);
  const scrollTargetRef = useRef(0);
  const lastFrameTimeRef = useRef<number | null>(null);
  const activeSessionRef = useRef<number | null>(null);
  const wasVisibleRef = useRef(false);
  const previousRevealSessionRef = useRef<number | null>(null);
  const previousRevealTextRef = useRef("");
  const text = payload.text || "";
  const statusLabel =
    payload.message
    ?? (payload.state === "starting"
      ? "正在启动麦克风"
      : payload.state === "recording"
        ? "正在聆听"
        : payload.state === "error"
          ? "语音输入失败"
          : "正在识别");
  const segments = getPreInputTextSegments(text, payload.confirmedChars);
  const previousText =
    previousRevealSessionRef.current === payload.sessionId ? previousRevealTextRef.current : "";
  const revealCharCount = getChangedSuffixLength(previousText, text);
  const pendingCharCount = Array.from(segments.pendingText).length;
  const pendingRevealCount = Math.min(revealCharCount, pendingCharCount);
  const confirmedRevealCount = Math.max(0, revealCharCount - pendingRevealCount);
  const confirmedParts = splitTextTail(segments.confirmedText, confirmedRevealCount);
  const pendingParts = splitTextTail(segments.pendingText, pendingRevealCount);

  useLayoutEffect(() => {
    const viewport = textViewportRef.current;
    const isNewSession = activeSessionRef.current !== payload.sessionId;
    const becameVisible = visible && !wasVisibleRef.current;
    activeSessionRef.current = payload.sessionId;
    wasVisibleRef.current = visible;

    const stopScrolling = () => {
      if (scrollFrameRef.current !== null) {
        window.cancelAnimationFrame(scrollFrameRef.current);
        scrollFrameRef.current = null;
      }
      lastFrameTimeRef.current = null;
    };

    if (viewport) {
      const flow = viewport.querySelector<HTMLElement>(".preinput-text__flow");
      viewport.toggleAttribute(
        "data-has-previous-line",
        (flow?.offsetHeight ?? 0) > TEXT_LINE_HEIGHT_PX + 0.5,
      );
    }

    if (!viewport || !visible) {
      stopScrolling();
      return;
    }

    const target = Math.max(0, viewport.scrollHeight - viewport.clientHeight);
    scrollTargetRef.current = target;
    const prefersReducedMotion =
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

    if (isNewSession || becameVisible || prefersReducedMotion) {
      stopScrolling();
      viewport.scrollTop = target;
      return;
    }

    if (Math.abs(target - viewport.scrollTop) < 0.5 || scrollFrameRef.current !== null) return;

    const scrollLinearly = (time: number) => {
      const currentViewport = textViewportRef.current;
      if (!currentViewport) {
        scrollFrameRef.current = null;
        lastFrameTimeRef.current = null;
        return;
      }

      const previousTime = lastFrameTimeRef.current ?? time - 16;
      const elapsed = Math.min(Math.max(time - previousTime, 0), MAX_FRAME_DELTA_MS);
      const distance = scrollTargetRef.current - currentViewport.scrollTop;
      const step = LINEAR_SCROLL_SPEED_PX_PER_MS * elapsed;
      lastFrameTimeRef.current = time;

      if (Math.abs(distance) <= Math.max(step, 0.5)) {
        currentViewport.scrollTop = scrollTargetRef.current;
        scrollFrameRef.current = null;
        lastFrameTimeRef.current = null;
        return;
      }

      currentViewport.scrollTop += Math.sign(distance) * step;
      scrollFrameRef.current = window.requestAnimationFrame(scrollLinearly);
    };

    lastFrameTimeRef.current = null;
    scrollFrameRef.current = window.requestAnimationFrame(scrollLinearly);
  }, [payload.sessionId, payload.seq, text, visible]);

  useLayoutEffect(
    () => () => {
      if (scrollFrameRef.current !== null) window.cancelAnimationFrame(scrollFrameRef.current);
    },
    [],
  );

  useLayoutEffect(() => {
    previousRevealSessionRef.current = payload.sessionId;
    previousRevealTextRef.current = text;
  }, [payload.sessionId, text]);

  return (
    <section
      className={`preinput-shell${visible ? " visible" : ""}`}
      data-state={payload.state}
      aria-label="语音预输入"
    >
      <div className="preinput-text" role="status" aria-live="polite">
        <RoseCurveLoader compact={Boolean(text)} label={statusLabel} />
        {text ? (
          <span ref={textViewportRef} className="preinput-text__copy">
            <span className="preinput-text__content">
              <span className="preinput-text__flow">
                {segments.hiddenPrefix ? <span className="preinput-text__prefix">...</span> : null}
                {confirmedParts.stableText ? (
                  <span className="preinput-text__confirmed">{confirmedParts.stableText}</span>
                ) : null}
                {confirmedParts.revealText ? (
                  <span
                    key={`confirmed-${payload.sessionId}-${payload.seq}`}
                    className="preinput-text__confirmed preinput-text__reveal"
                  >
                    {confirmedParts.revealText}
                  </span>
                ) : null}
                {pendingParts.stableText ? <span>{pendingParts.stableText}</span> : null}
                {pendingParts.revealText ? (
                  <span
                    key={`pending-${payload.sessionId}-${payload.seq}`}
                    className="preinput-text__reveal"
                  >
                    {pendingParts.revealText}
                  </span>
                ) : null}
              </span>
            </span>
          </span>
        ) : null}
      </div>
    </section>
  );
}
