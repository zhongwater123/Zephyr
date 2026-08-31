import type { JSX } from "preact";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import type { PolishLevel } from "../../domain";
import { createPolishField, type FieldHandle } from "./polishFieldRenderer";
import {
  POLISH_STOPS,
  magnetize,
  positionFromClientX,
  tierFor,
} from "./polishTrackMath";

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

/** Directional swap of the level name/description. Latest-wins. */
const SWAP_MS = 140;

type Gesture = { pointerId: number; startLevel: PolishLevel; tier: PolishLevel };

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

  const trackRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const gestureRef = useRef<Gesture | null>(null);

  // Live position and level for the renderer. Refs, not state: the animation
  // loop reads these every frame and must never drive a re-render.
  const posRef = useRef<number>(POLISH_STOPS[value]);
  const levelRef = useRef<number>(value);

  const [dragging, setDragging] = useState(false);
  // Which level the readout shows. Follows the pointer during a drag so the
  // preview is live, while the COMMITTED value stays what gets saved and
  // announced.
  const [previewTier, setPreviewTier] = useState<PolishLevel>(value);
  const [outgoing, setOutgoing] = useState<{ level: PolishLevel; dir: 1 | -1 } | null>(null);
  const prevTierRef = useRef<PolishLevel>(value);
  const swapTimerRef = useRef<number>(0);

  const reduced = usePrefersReducedMotion();
  const shown = LEVELS[previewTier];

  // Keep the visual position in sync when the level changes from outside a
  // gesture: keyboard, a rollback after a failed save, or a fresh mount.
  useLayoutEffect(() => {
    if (gestureRef.current) return;
    posRef.current = POLISH_STOPS[value];
    levelRef.current = value;
    writeTrackPosition(trackRef.current, POLISH_STOPS[value]);
    setPreviewTier(value);
  }, [value]);

  // Directional readout swap. A new target mid-swap replaces the outgoing slot
  // instead of queueing, so a fast flick across several levels lands on the
  // last one rather than replaying every step.
  useEffect(() => {
    const prev = prevTierRef.current;
    if (prev === previewTier) return;
    prevTierRef.current = previewTier;
    if (reduced) {
      setOutgoing(null);
      return;
    }
    setOutgoing({ level: prev, dir: previewTier > prev ? 1 : -1 });
    if (swapTimerRef.current) window.clearTimeout(swapTimerRef.current);
    swapTimerRef.current = window.setTimeout(() => {
      swapTimerRef.current = 0;
      setOutgoing(null);
    }, SWAP_MS);
  }, [previewTier, reduced]);

  useEffect(
    () => () => {
      if (swapTimerRef.current) window.clearTimeout(swapTimerRef.current);
    },
    [],
  );

  // Field lifecycle: created once, resized with the element, and paused
  // whenever the document is hidden so a background window burns no frames.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || typeof canvas.getContext !== "function") return;

    let field: FieldHandle;
    try {
      field = createPolishField(canvas, {
        getPosition: () => posRef.current,
        getLevel: () => levelRef.current,
        getStatic: () => reduced,
      });
    } catch {
      // No 2D context (some headless environments): the track still works,
      // it just has no energy field.
      return;
    }
    field.resize();
    if (document.visibilityState !== "hidden") field.start();

    function onVisibility() {
      if (document.visibilityState === "hidden") field.stop();
      else field.start();
    }
    document.addEventListener("visibilitychange", onVisibility);

    let observer: ResizeObserver | null = null;
    if (typeof ResizeObserver === "function") {
      observer = new ResizeObserver(() => field.resize());
      observer.observe(canvas);
    }

    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      observer?.disconnect();
      field.destroy();
    };
  }, [reduced]);

  function setLivePosition(u: number) {
    posRef.current = u;
    writeTrackPosition(trackRef.current, u);
  }

  function handlePointerDown(event: JSX.TargetedPointerEvent<HTMLDivElement>) {
    if (saving) return;
    const track = trackRef.current;
    if (!track) return;
    const u = positionFromClientX(event.clientX, track.getBoundingClientRect());
    // No usable layout box (happy-dom, or a collapsed sidebar): leave the
    // environment's own behaviour untouched rather than preventDefault-ing.
    if (u === null) return;

    // This is what stops the native range from firing its own input events
    // mid-drag, which would commit the config repeatedly during one gesture.
    event.preventDefault();
    inputRef.current?.focus({ preventScroll: true });
    track.setPointerCapture?.(event.pointerId);
    const tier = tierFor(u, value);
    gestureRef.current = { pointerId: event.pointerId, startLevel: value, tier };
    setDragging(true);
    setLivePosition(magnetize(u));
    if (tier !== previewTier) setPreviewTier(tier);
  }

  function handlePointerMove(event: JSX.TargetedPointerEvent<HTMLDivElement>) {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    const track = trackRef.current;
    if (!track) return;
    const u = positionFromClientX(event.clientX, track.getBoundingClientRect());
    if (u === null) return;
    setLivePosition(magnetize(u));
    const next = tierFor(u, gesture.tier);
    if (next !== gesture.tier) {
      gesture.tier = next;
      setPreviewTier(next);
    }
  }

  function endGesture(commit: boolean) {
    const gesture = gestureRef.current;
    gestureRef.current = null;
    setDragging(false);
    if (!gesture) return;
    const target = commit ? gesture.tier : value;
    setLivePosition(POLISH_STOPS[target]);
    levelRef.current = target;
    // THE single commit for this gesture. The only other caller of onChange is
    // the native input's own change handler, which is one commit per key press.
    if (commit && gesture.tier !== value) onChange(gesture.tier);
    if (!commit) setPreviewTier(value);
  }

  function handlePointerUp(event: JSX.TargetedPointerEvent<HTMLDivElement>) {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    endGesture(true);
  }

  function handlePointerCancel() {
    if (!gestureRef.current) return;
    endGesture(false);
  }

  function selectLevel(nextValue: number) {
    if (nextValue === value) return;
    if (nextValue === 0 || nextValue === 1 || nextValue === 2 || nextValue === 3) {
      onChange(nextValue);
    }
  }

  const enterDir = outgoing ? (outgoing.dir > 0 ? " dir-up" : " dir-down") : "";

  return (
    <div className="polish-setting">
      <div className="polish-setting-heading">
        <strong>智能润色</strong>
        <span className="polish-level-window">
          {outgoing ? (
            <span
              key={"out-" + outgoing.level}
              className={"polish-level-slot is-leaving" + enterDir}
              aria-hidden="true"
            >
              {LEVELS[outgoing.level].label}
            </span>
          ) : null}
          <span
            key={"in-" + previewTier}
            className={"polish-level-slot" + (outgoing ? " is-entering" + enterDir : "")}
          >
            {shown.label}
          </span>
        </span>
        {saving ? <span className="polish-saving-halo" aria-hidden="true" /> : null}
      </div>
      <p className="polish-setting-intro">说完后，希望得到怎样的文字？</p>
      <p className="polish-setting-sub">选择更快直出，或让文字更顺、更有条理。</p>

      <div className="polish-track-row">
        <span className="polish-endpoint" aria-hidden="true">Faster</span>
        <div
          ref={trackRef}
          className={
            "polish-track" +
            (dragging ? " is-dragging" : "") +
            (reduced ? " is-static" : "") +
            (saving ? " is-saving" : "")
          }
          data-tier={previewTier}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onPointerCancel={handlePointerCancel}
          onLostPointerCapture={handlePointerCancel}
        >
          <div className="polish-track-bed" aria-hidden="true">
            <canvas ref={canvasRef} className="polish-field" />
            <span className="polish-notch" style={{ left: "33.3333%" }} />
            <span className="polish-notch" style={{ left: "66.6667%" }} />
          </div>
          <span className="polish-thumb" aria-hidden="true" />
          <input
            ref={inputRef}
            className="polish-range"
            type="range"
            min="0"
            max="3"
            step="1"
            value={value}
            disabled={saving}
            aria-label="智能润色输出方式"
            aria-valuetext={selected.label}
            aria-busy={saving || undefined}
            onChange={(event) => selectLevel(Number(event.currentTarget.value))}
          />
        </div>
        <span className="polish-endpoint" aria-hidden="true">Smarter</span>
      </div>

      <div className="polish-range-labels" aria-hidden="true">
        {LEVELS.map((level) => (
          <span key={level.value} className={value === level.value ? "is-active" : ""}>
            {level.label}
          </span>
        ))}
      </div>

      <p className="polish-setting-result" role="status" aria-live={dragging ? "off" : "polite"}>
        <span className="polish-desc-window">
          {outgoing ? (
            <span
              key={"dout-" + outgoing.level}
              className={"polish-desc-slot is-leaving" + enterDir}
              aria-hidden="true"
            >
              {LEVELS[outgoing.level].description}
            </span>
          ) : null}
          <span
            key={"din-" + previewTier}
            className={"polish-desc-slot" + (outgoing ? " is-entering" + enterDir : "")}
          >
            {shown.description}
          </span>
        </span>
      </p>

      {error ? (
        <p className="field-error" role="alert" title={error}>
          暂时没保存成功，请再试一次。
        </p>
      ) : null}
    </div>
  );
}

function writeTrackPosition(track: HTMLElement | null, u: number) {
  track?.style.setProperty("--polish-pos", u.toFixed(4));
}

function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState<boolean>(() => matchReduced());
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    const onChange = () => setReduced(query.matches);
    query.addEventListener?.("change", onChange);
    return () => query.removeEventListener?.("change", onChange);
  }, []);
  return reduced;
}

function matchReduced(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return false;
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
