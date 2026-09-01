import type { JSX } from "preact";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import type { PolishLevel } from "../../domain";
import { createPolishField, type FieldHandle } from "./polishFieldRenderer";
import {
  POLISH_STOPS,
  POLISH_THUMB,
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
    label: "极速模式",
    description: "适合高频短对话",
  },
  {
    value: 1,
    label: "轻微整理",
    description: "Zephyr轻微整理",
  },
  {
    value: 2,
    label: "自然表达",
    description: "让表达更顺，合适时自动整理要点。",
  },
  {
    value: 3,
    label: "理清重点",
    description: "深度整理优化",
  },
];

/**
 * Directional swap of the level name/description. Latest-wins.
 *
 * Deliberately much slower than the 120-160ms the Issue suggests: at that
 * speed the travel was over before the eye caught the direction, which is the
 * whole point of the motion. The clip windows are also taller than one line's
 * text so a bigger slice of the roll is visible while it moves.
 */
const SWAP_MS = 380;

type Gesture = {
  pointerId: number;
  startLevel: PolishLevel;
  tier: PolishLevel;
  /** Where the gesture began, to tell a tap from a drag. */
  startX: number;
  /** Set once the pointer has travelled far enough to count as a drag. */
  moved: boolean;
};

/** Pointer travel, in px, before a press is treated as a drag rather than a tap. */
const DRAG_THRESHOLD = 3;

/** One row of the roll: the level's name and what it means, side by side. */
function ReadoutLine({ tier, muted }: { tier: PolishLevel; muted: boolean }) {
  const level = LEVELS[tier];
  return (
    <span className="polish-readout-line" aria-hidden={muted || undefined}>
      {/* Fast carries its own hue, matching its blue plume: it is a different
          mode, not a weaker level, and the name should say so too. */}
      <span className={"polish-readout-name" + (tier === 0 ? " is-fast" : "")}>
        {level.label}
      </span>
      <span className="polish-readout-desc">{level.description}</span>
    </span>
  );
}

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
  // During a swap both lines are mounted so the old one can slide out.
  const [swap, setSwap] = useState<{ from: PolishLevel; dir: 1 | -1 } | null>(null);
  const prevTierRef = useRef<PolishLevel>(value);
  const swapIdRef = useRef(0);
  const readoutStripRef = useRef<HTMLSpanElement>(null);
  const thumbRef = useRef<HTMLSpanElement>(null);
  // Measured once per gesture: the track cannot resize mid-drag, and calling
  // getBoundingClientRect() on every pointermove forces a layout flush.
  const rectRef = useRef<{ left: number; width: number } | null>(null);
  const travelRef = useRef(0);
  const swapActiveRef = useRef(false);

  const reduced = usePrefersReducedMotion();
  const shown = LEVELS[previewTier];

  // Keep the visual position in sync when the level changes from outside a
  // gesture: keyboard, a rollback after a failed save, or a fresh mount.
  useLayoutEffect(() => {
    if (gestureRef.current) return;
    posRef.current = POLISH_STOPS[value];
    levelRef.current = value;
    writeTrackPosition(thumbRef.current, POLISH_STOPS[value]);
    setPreviewTier(value);
  }, [value]);

  /*
   * Directional scroll of the readout.
   *
   * Odometer model: the levels form a vertical strip with the HIGH end above
   * and the low end below. Raising the level pulls content DOWN from above —
   * the strip travels downward while the scale climbs, which is what gives an
   * increase its sense of ascent. Lowering it runs the other way.
   *
   * Both lines are mounted for the duration, so the outgoing one slides out as
   * the incoming one slides in. One animation per strip; starting a new one
   * cancels whatever is still running, so this is latest-wins by construction
   * and a fast drag cannot queue a chain of stale scrolls.
   */
  useLayoutEffect(() => {
    const prev = prevTierRef.current;
    if (prev === previewTier) return;
    const dir: 1 | -1 = previewTier > prev ? 1 : -1;
    prevTierRef.current = previewTier;
    if (reduced) {
      setSwap(null);
      return;
    }
    setSwap({ from: prev, dir });
  }, [previewTier, reduced]);

  // Runs once the outgoing line is actually in the DOM, so both lines move
  // together as one strip.
  useLayoutEffect(() => {
    if (!swap || reduced) return;
    const id = (swapIdRef.current += 1);
    swapActiveRef.current = true;
    /*
     * The strip holds TWO lines, so one line is 50% of it — not 100%. Using
     * 100% shifted the strip by its whole height, which parked both lines
     * outside the window and swallowed the outgoing line entirely.
     *
     * Rising: [incoming, outgoing] parked one line up, brought to 0 — content
     * moves DOWN. Falling: [outgoing, incoming] from 0 up one line — content
     * moves UP.
     */
    const fromPct = swap.dir > 0 ? -50 : 0;
    const toPct = swap.dir > 0 ? 0 : -50;
    const running: Animation[] = [];
    for (const el of [readoutStripRef.current]) {
      if (!el || typeof el.animate !== "function") continue;
      // animate() APPENDS rather than replaces, so old ones must be cancelled
      // or a fast drag leaves several fighting over the transform.
      el.getAnimations?.().forEach((a) => a.cancel());
      /*
       * `transform` is composited, but `filter: blur()` is a REPAINT of the
       * text on every frame and is the one expensive part of this animation.
       * Promoting the strip for the duration lets the compositor keep the
       * blurred result instead of re-rasterising the glyphs each frame; it is
       * cleared on teardown so the layer is not kept alive at rest.
       */
      el.style.willChange = "transform, filter";
      running.push(
        el.animate(
          [
            { transform: `translateY(${fromPct}%)`, filter: "blur(0px)" },
            { transform: `translateY(${(fromPct + toPct) / 2}%)`, filter: "blur(4px)", offset: 0.5 },
            { transform: `translateY(${toPct}%)`, filter: "blur(0px)" },
          ],
          // `both`, so the end pose holds until the outgoing line is
          // unmounted — with `backwards` the transform snapped to 0 while both
          // lines were still mounted, flashing the old text back on a
          // downward swap.
          { duration: SWAP_MS, easing: "cubic-bezier(0.33, 1, 0.68, 1)", fill: "both" },
        ),
      );
    }
    if (running.length === 0) {
      setSwap(null);
      return;
    }
    /*
     * Cancel and unmount in the SAME commit. The end pose puts the incoming
     * line in the window at strip offset 0 (rising) or -1 line (falling); once
     * the outgoing line is gone the single remaining line sits at offset 0
     * either way, so dropping the transform and the line together leaves the
     * text exactly where it already was. Doing one without the other is what
     * left the text parked half outside the window.
     */
    let torn = false;
    const teardown = () => {
      if (torn || swapIdRef.current !== id) return;
      torn = true;
      swapActiveRef.current = false;
      for (const el of [readoutStripRef.current]) {
        if (el) el.style.willChange = "";
      }
      running.forEach((a) => a.cancel());
      setSwap(null);
    };
    // Whichever comes first. Waiting on ALL of them meant one animation that
    // never resolves would leave the outgoing line mounted for good, and the
    // timeout alone would be at the mercy of timer throttling.
    Promise.race(running.map((a) => a.finished)).then(teardown, teardown);
    const guard = window.setTimeout(teardown, SWAP_MS + 60);
    return () => window.clearTimeout(guard);
  }, [swap, reduced]);

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
        getBusy: () => gestureRef.current !== null || swapActiveRef.current,
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
      observer = new ResizeObserver(() => {
        field.resize();
        publishTravel();
      });
      observer.observe(canvas);
    }
    publishTravel();

    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      observer?.disconnect();
      field.destroy();
    };
  }, [reduced]);

  function publishTravel() {
    const track = trackRef.current;
    if (!track) return;
    const rect = track.getBoundingClientRect();
    travelRef.current = Math.max(0, rect.width - POLISH_THUMB);
    track.style.setProperty("--polish-travel", travelRef.current.toFixed(2) + "px");
  }

  /** The gesture's cached geometry, measured at pointerdown. */
  function trackRect() {
    if (rectRef.current) return rectRef.current;
    const track = trackRef.current;
    if (!track) return null;
    const box = track.getBoundingClientRect();
    return { left: box.left, width: box.width };
  }

  /**
   * Position and level must move in the SAME frame.
   *
   * The renderer reads both every frame. Updating the position on pointerdown
   * but the level only on release meant that for the frames in between it drew
   * the OLD level's energy at the NEW position — the previous plume visibly
   * stretched to the new spot, then faded, and only then did the new plume
   * sweep out. Three events where there should be one.
   */
  function setLive(u: number, tier: PolishLevel) {
    levelRef.current = tier;
    setLivePosition(u);
  }

  function setLivePosition(u: number) {
    posRef.current = u;
    // Written on the handle itself, not on the track: only the handle consumes
    // --polish-pos, and setting it on an ancestor made the whole subtree
    // recompute style on every move.
    writeTrackPosition(thumbRef.current, u);
  }

  /** Toggle the handle's settle easing synchronously, not via async state. */
  function setSettling(on: boolean) {
    trackRef.current?.style.setProperty("--polish-settle", on ? "200ms" : "0ms");
  }

  function handlePointerDown(event: JSX.TargetedPointerEvent<HTMLDivElement>) {
    // Deliberately NOT gated on `saving`: persisting is background work and
    // must never block the next gesture. The optimistic position stands and a
    // failed save rolls it back through the `value` prop.
    const track = trackRef.current;
    if (!track) return;
    const box = track.getBoundingClientRect();
    const u = positionFromClientX(event.clientX, box);
    // No usable layout box (happy-dom, or a collapsed sidebar): leave the
    // environment's own behaviour untouched rather than preventDefault-ing.
    if (u === null) return;
    rectRef.current = { left: box.left, width: box.width };

    // This is what stops the native range from firing its own input events
    // mid-drag, which would commit the config repeatedly during one gesture.
    event.preventDefault();
    inputRef.current?.focus({ preventScroll: true });
    // Capture is a nice-to-have; it throws for a pointer id the platform no
    // longer knows about, and letting that escape would abandon the gesture
    // before it is even registered — the drag would silently do nothing.
    try {
      track.setPointerCapture?.(event.pointerId);
    } catch {
      /* keep going without capture */
    }
    const tier = tierFor(u, value);
    gestureRef.current = {
      pointerId: event.pointerId,
      startLevel: value,
      tier,
      startX: event.clientX,
      moved: false,
    };
    setDragging(true);
    /*
     * A press goes STRAIGHT to the stop, with the settle easing left on.
     * Sending it to magnetize(u) first — a point between stops — and only
     * snapping on release meant a plain click moved the handle twice: once to
     * the finger, then again to the stop. magnetize() exists to shape the feel
     * of a continuous drag, so it only applies once the pointer actually moves.
     */
    setSettling(true);
    setLive(POLISH_STOPS[tier], tier);
    if (tier !== previewTier) setPreviewTier(tier);
  }

  function handlePointerMove(event: JSX.TargetedPointerEvent<HTMLDivElement>) {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    const box = trackRect();
    if (!box) return;
    if (!gesture.moved) {
      // Ignore jitter, so a click with a shaky hand still behaves as a click.
      if (Math.abs(event.clientX - gesture.startX) < DRAG_THRESHOLD) return;
      gesture.moved = true;
      // Now it is a drag: drop the easing so the handle tracks the pointer.
      setSettling(false);
    }
    const u = positionFromClientX(event.clientX, box);
    if (u === null) return;
    const next = tierFor(u, gesture.tier);
    setLive(magnetize(u), next);
    if (next !== gesture.tier) {
      gesture.tier = next;
      setPreviewTier(next);
    }
  }

  function endGesture(commit: boolean) {
    const gesture = gestureRef.current;
    gestureRef.current = null;
    rectRef.current = null;
    setDragging(false);
    if (!gesture) return;
    const target = commit ? gesture.tier : value;
    setSettling(true);
    setLive(POLISH_STOPS[target], target);
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

  return (
    <div className="polish-setting">
      <div className="polish-setting-heading">
        <strong>智能润色</strong>
        <span
          className="polish-readout-window"
          role="status"
          aria-live={dragging ? "off" : "polite"}
        >
          <span ref={readoutStripRef} className="polish-readout-strip">
            {swap
              ? (swap.dir > 0
                  ? [previewTier, swap.from]
                  : [swap.from, previewTier]
                ).map((tier, i) => (
                  <ReadoutLine
                    key={"l" + i + "-" + tier}
                    tier={tier}
                    muted={tier !== previewTier}
                  />
                ))
              : <ReadoutLine tier={previewTier} muted={false} />}
          </span>
        </span>
      </div>

      <div className="polish-control">
      <div className="polish-scale" aria-hidden="true">
        <span>Faster</span>
        <span>Smarter</span>
      </div>

      <div className="polish-track-row">
        <div
          ref={trackRef}
          className={
            "polish-track" +
            (dragging ? " is-dragging" : "") +
            (reduced ? " is-static" : "")
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
          <span ref={thumbRef} className="polish-thumb" aria-hidden="true" />
          <input
            ref={inputRef}
            className="polish-range"
            type="range"
            min="0"
            max="3"
            step="1"
            value={value}
            aria-label="智能润色输出方式"
            aria-valuetext={selected.label}
            /* announced, never shown: saving must be invisible to the user */
            aria-busy={saving || undefined}
            onChange={(event) => selectLevel(Number(event.currentTarget.value))}
          />
        </div>
      </div>
      </div>


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
