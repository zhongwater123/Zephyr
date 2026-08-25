import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "preact/hooks";
import type { PreInputPayload } from "../domain";
import { preinputApi } from "../ipc/client";

const maxOverlayCharacters = 180;

export function PreInputOverlay() {
  const [payload, setPayload] = useState<PreInputPayload>({
    sessionId: 0,
    text: "",
    state: "recording",
    confirmedChars: 0,
    message: "正在聆听",
    seq: 0,
  });
  const [visible, setVisible] = useState(false);
  const latestSession = useRef(0);
  const latestSeq = useRef(0);
  const closedSession = useRef(0);

  useEffect(() => {
    document.documentElement.classList.add("preinput-root");
    document.body.classList.add("preinput-body");
    let disposed = false;
    let fastSyncTimer: number | undefined;
    let fastSyncStopTimer: number | undefined;

    const acceptPayload = (nextPayload: PreInputPayload) => {
      if (nextPayload.sessionId < latestSession.current) return;
      if (nextPayload.sessionId === closedSession.current) return;
      if (nextPayload.sessionId > latestSession.current) {
        latestSession.current = nextPayload.sessionId;
        latestSeq.current = 0;
      }
      if (nextPayload.seq <= latestSeq.current) return;
      latestSeq.current = nextPayload.seq;
      setPayload(nextPayload);
      setVisible(true);
    };

    const syncPayload = async () => {
      try {
        const nextPayload = await preinputApi.getPayload();
        if (disposed) return;
        if (nextPayload) {
          acceptPayload(nextPayload);
        } else {
          setVisible(false);
        }
      } catch {
        // 后端尚未就绪时，悬浮预输入框保持安静。
      }
    };

    const stopFastSync = () => {
      if (fastSyncTimer !== undefined) {
        window.clearInterval(fastSyncTimer);
        fastSyncTimer = undefined;
      }
      if (fastSyncStopTimer !== undefined) {
        window.clearTimeout(fastSyncStopTimer);
        fastSyncStopTimer = undefined;
      }
    };

    const startFastSync = () => {
      stopFastSync();
      void syncPayload();
      fastSyncTimer = window.setInterval(syncPayload, 50);
      fastSyncStopTimer = window.setTimeout(stopFastSync, 1000);
    };

    const unlistenShow = listen<PreInputPayload>("preinput_show", (event) => {
      acceptPayload(event.payload);
      startFastSync();
    });
    const unlistenUpdate = listen<PreInputPayload>("preinput_update", (event) => {
      acceptPayload(event.payload);
    });
    const unlistenHide = listen<PreInputPayload>("preinput_hide", (event) => {
      stopFastSync();
      if (event.payload.sessionId >= latestSession.current) {
        latestSession.current = event.payload.sessionId;
        latestSeq.current = event.payload.seq;
        closedSession.current = event.payload.sessionId;
      }
      setVisible(false);
    });
    startFastSync();

    return () => {
      disposed = true;
      stopFastSync();
      document.documentElement.classList.remove("preinput-root");
      document.body.classList.remove("preinput-body");
      unlistenShow.then((dispose) => dispose());
      unlistenUpdate.then((dispose) => dispose());
      unlistenHide.then((dispose) => dispose());
    };
  }, []);

  const text = payload.text || "";
  const characters = Array.from(text);
  const hiddenPrefixChars = Math.max(0, characters.length - maxOverlayCharacters);
  const visibleCharacters = characters.slice(hiddenPrefixChars);
  const confirmedChars = Math.min(payload.confirmedChars ?? 0, characters.length);
  const visibleConfirmedChars = Math.max(0, confirmedChars - hiddenPrefixChars);
  const confirmedText = visibleCharacters.slice(0, visibleConfirmedChars).join("");
  const pendingText = visibleCharacters.slice(visibleConfirmedChars).join("");

  return (
    <div className={`preinput-shell ${visible ? "visible" : ""} ${payload.state}`}>
      <div className="preinput-topline">
        <span className="preinput-dot" />
        <span>{payload.message || stateLabel(payload.state)}</span>
      </div>
      <div className="preinput-text" aria-live="polite">
        {text ? (
          <>
            {hiddenPrefixChars > 0 ? <span className="prefix-fade">...</span> : null}
            <span className="confirmed">{confirmedText}</span>
            <span>{pendingText}</span>
          </>
        ) : (
          <RoseCurveLoader />
        )}
      </div>
    </div>
  );
}

const ROSE_PARTICLE_COUNT = 54;

function RoseCurveLoader() {
  const groupRef = useRef<SVGGElement | null>(null);
  const pathRef = useRef<SVGPathElement | null>(null);
  const particleRefs = useRef<Array<SVGCircleElement | null>>([]);

  useEffect(() => {
    let frameId = 0;
    const startedAt = performance.now();

    const renderFrame = (now: number) => {
      const elapsed = now - startedAt;
      const progress = (elapsed % 3600) / 3600;
      const detailScale = getRoseDetailScale(elapsed);
      const rotation = -((elapsed % 22000) / 22000) * 360;

      groupRef.current?.setAttribute("transform", `rotate(${rotation.toFixed(2)} 50 50)`);
      pathRef.current?.setAttribute("d", buildRosePath(detailScale));

      particleRefs.current.forEach((node, index) => {
        if (!node) return;
        const particle = getRoseParticle(index, progress, detailScale);
        node.setAttribute("cx", particle.x.toFixed(2));
        node.setAttribute("cy", particle.y.toFixed(2));
        node.setAttribute("r", particle.radius.toFixed(2));
        node.setAttribute("opacity", particle.opacity.toFixed(3));
      });

      frameId = requestAnimationFrame(renderFrame);
    };

    frameId = requestAnimationFrame(renderFrame);
    return () => cancelAnimationFrame(frameId);
  }, []);

  return (
    <span className="rose-loader" aria-label="正在聆听">
      <svg className="rose-curve" viewBox="0 0 100 100" aria-hidden="true">
        <g ref={groupRef}>
          <path ref={pathRef} className="rose-track" />
          {Array.from({ length: ROSE_PARTICLE_COUNT }, (_, index) => (
            <circle
              key={index}
              ref={(node) => {
                particleRefs.current[index] = node;
              }}
              className="rose-particle"
            />
          ))}
        </g>
      </svg>
      <span className="rose-label">正在聆听</span>
    </span>
  );
}

function normalizeRoseProgress(progress: number) {
  return ((progress % 1) + 1) % 1;
}

function getRoseDetailScale(elapsed: number) {
  const pulseProgress = (elapsed % 4300) / 4300;
  return 0.52 + ((Math.sin(pulseProgress * Math.PI * 2 + 0.55) + 1) / 2) * 0.48;
}

function getRosePoint(progress: number, detailScale: number) {
  const t = progress * Math.PI * 2;
  const a = 9.2 + detailScale * 0.6;
  const r = a * (0.72 + detailScale * 0.28) * Math.cos(4 * t);

  return {
    x: 50 + Math.cos(t) * r * 3.25,
    y: 50 + Math.sin(t) * r * 3.25,
  };
}

function buildRosePath(detailScale: number, steps = 360) {
  return Array.from({ length: steps + 1 }, (_, index) => {
    const point = getRosePoint(index / steps, detailScale);
    return `${index === 0 ? "M" : "L"} ${point.x.toFixed(2)} ${point.y.toFixed(2)}`;
  }).join(" ");
}

function getRoseParticle(index: number, progress: number, detailScale: number) {
  const tailOffset = index / (ROSE_PARTICLE_COUNT - 1);
  const point = getRosePoint(normalizeRoseProgress(progress - tailOffset * 0.34), detailScale);
  const fade = Math.pow(1 - tailOffset, 0.58);

  return {
    x: point.x,
    y: point.y,
    radius: 0.62 + fade * 2.1,
    opacity: 0.03 + fade * 0.82,
  };
}

function stateLabel(state: PreInputPayload["state"]) {
  switch (state) {
    case "transcribing":
      return "正在识别";
    case "finalizing":
      return "正在写入";
    case "dismissing":
      return "正在收起";
    case "error":
      return "失败";
    case "recording":
    default:
      return "正在聆听";
  }
}

