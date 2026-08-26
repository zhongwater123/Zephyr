import { useEffect, useRef } from "preact/hooks";

const PARTICLE_COUNT = 54;

export function RoseCurveLoader({ compact = false }: { compact?: boolean }) {
  const groupRef = useRef<SVGGElement | null>(null);
  const pathRef = useRef<SVGPathElement | null>(null);
  const particleRefs = useRef<Array<SVGCircleElement | null>>([]);

  useEffect(() => {
    const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    if (reducedMotion) {
      renderRoseFrame(groupRef.current, pathRef.current, particleRefs.current, 0, 1);
      return;
    }

    let frameId = 0;
    const startedAt = performance.now();

    const renderFrame = (now: number) => {
      const elapsed = now - startedAt;
      const progress = (elapsed % 3600) / 3600;
      renderRoseFrame(
        groupRef.current,
        pathRef.current,
        particleRefs.current,
        -((elapsed % 22000) / 22000) * 360,
        progress,
        getRoseDetailScale(elapsed),
      );
      frameId = requestAnimationFrame(renderFrame);
    };

    frameId = requestAnimationFrame(renderFrame);
    return () => cancelAnimationFrame(frameId);
  }, []);

  return (
    <span className={`preinput-loader${compact ? " preinput-loader--compact" : ""}`} aria-label="正在聆听">
      <svg className="preinput-loader__curve" viewBox="0 0 100 100" aria-hidden="true">
        <g ref={groupRef}>
          <path ref={pathRef} className="preinput-loader__track" />
          {Array.from({ length: PARTICLE_COUNT }, (_, index) => (
            <circle
              key={index}
              ref={(node) => {
                particleRefs.current[index] = node;
              }}
              className="preinput-loader__particle"
            />
          ))}
        </g>
      </svg>
      {compact ? null : <span className="preinput-loader__label">正在聆听</span>}
    </span>
  );
}

function renderRoseFrame(
  group: SVGGElement | null,
  path: SVGPathElement | null,
  particles: Array<SVGCircleElement | null>,
  rotation: number,
  progress: number,
  detailScale = 1,
) {
  group?.setAttribute("transform", `rotate(${rotation.toFixed(2)} 50 50)`);
  path?.setAttribute("d", buildRosePath(detailScale));
  particles.forEach((node, index) => {
    if (!node) return;
    const particle = getRoseParticle(index, progress, detailScale);
    node.setAttribute("cx", particle.x.toFixed(2));
    node.setAttribute("cy", particle.y.toFixed(2));
    node.setAttribute("r", particle.radius.toFixed(2));
    node.setAttribute("opacity", particle.opacity.toFixed(3));
  });
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
  const tailOffset = index / (PARTICLE_COUNT - 1);
  const point = getRosePoint(normalizeRoseProgress(progress - tailOffset * 0.34), detailScale);
  const fade = Math.pow(1 - tailOffset, 0.58);

  return {
    x: point.x,
    y: point.y,
    radius: 0.62 + fade * 2.1,
    opacity: 0.03 + fade * 0.82,
  };
}
