import type { JSX } from "preact";
import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import * as THREE from "three";

type ZephyrAsciiFieldProps = {
  state: string;
  muted: boolean;
  shortcut: string;
};

type Glyph = {
  id: number;
  x: number;
  y: number;
  size: number;
  opacity: number;
  drift: number;
};

type Point = {
  x: number;
  y: number;
};

const glyphTokens = [
  "言",
  "语",
  "声",
  "字",
  "词",
  "句",
  "文",
  "译",
  "意",
  "音",
  "云",
  "流",
  "录",
  "写",
  "听",
  "读",
  "思",
  "问",
  "答",
  "述",
  "记",
  "篇",
  "章",
  "段",
  "行",
  "点",
  "墨",
  "白",
  "风",
  "息",
  "轻",
  "响",
  "明",
  "暗",
  "浮",
  "落",
  "连",
  "续",
  "变",
  "化",
  "识",
  "别",
  "输",
  "入",
];

const vertexShader = `
varying vec2 vUv;
uniform float uTime;
uniform float uEnableWaves;

void main() {
  vUv = uv;
  vec3 transformed = position;
  transformed.x += sin(uTime + position.y * 1.4) * 0.018 * uEnableWaves;
  transformed.y += cos(uTime * 0.75 + position.x * 0.8) * 0.007 * uEnableWaves;
  transformed.z += sin(uTime * 0.6 + position.x) * 0.014 * uEnableWaves;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(transformed, 1.0);
}
`;

const fragmentShader = `
varying vec2 vUv;
uniform float uTime;
uniform sampler2D uTexture;

void main() {
  vec2 pos = vUv;
  float ripple = sin(uTime + pos.y * 8.0) * 0.0003;
  vec4 color = texture2D(uTexture, vec2(pos.x + ripple, pos.y));
  gl_FragColor = color;
}
`;

export function ZephyrAsciiField({ state, muted, shortcut }: ZephyrAsciiFieldProps) {
  const fieldRef = useRef<HTMLDivElement>(null);
  const logoRef = useRef<HTMLDivElement>(null);
  const rafRef = useRef<number | undefined>(undefined);
  const pendingMouse = useRef<Point | null>(null);
  const [mouse, setMouse] = useState<Point>({ x: -100, y: -100 });
  const [glyphVersion, setGlyphVersion] = useState(0);

  const glyphs = useMemo(() => buildGlyphs(), []);
  const tone = state.toLowerCase();

  useEffect(() => {
    const timer = window.setInterval(() => {
      setGlyphVersion((version) => version + 1);
    }, 720);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    const container = logoRef.current;
    if (!container) return;

    let disposed = false;
    let renderer: ZephyrAsciiRenderer | null = null;

    const setup = async () => {
      renderer = new ZephyrAsciiRenderer(container);
      await renderer.init();
      if (disposed) {
        renderer.dispose();
        return;
      }
      renderer.start();
    };

    setup();

    return () => {
      disposed = true;
      renderer?.dispose();
    };
  }, []);

  function handlePointerMove(event: JSX.TargetedPointerEvent<HTMLDivElement>) {
    const rect = fieldRef.current?.getBoundingClientRect();
    if (!rect) return;
    pendingMouse.current = {
      x: ((event.clientX - rect.left) / rect.width) * 100,
      y: ((event.clientY - rect.top) / rect.height) * 100,
    };
    if (rafRef.current !== undefined) return;
    rafRef.current = window.requestAnimationFrame(() => {
      rafRef.current = undefined;
      if (pendingMouse.current) setMouse(pendingMouse.current);
    });
  }

  useEffect(() => {
    return () => {
      if (rafRef.current !== undefined) window.cancelAnimationFrame(rafRef.current);
    };
  }, []);

  return (
    <div
      ref={fieldRef}
      className={`zephyr-field ${muted ? "muted" : ""} tone-${tone}`}
      onPointerMove={handlePointerMove}
    >
      <div className="glyph-field" aria-hidden="true">
        {glyphs.map((glyph) => (
          <span key={glyph.id} className="glyph" style={glyphStyle(glyph, mouse)}>
            {glyphText(glyph, glyphVersion)}
          </span>
        ))}
      </div>
      <div ref={logoRef} className="zephyr-ascii-logo" aria-label="Zephyr" />
      <div className="zephyr-caption" aria-hidden="true">
        <span>Zephyr</span>
        <span>按下 {shortcut} 语音输入</span>
      </div>
    </div>
  );
}

function buildGlyphs(): Glyph[] {
  const columns = 30;
  const rows = 22;
  const glyphs: Glyph[] = [];

  for (let col = 0; col < columns; col += 1) {
    for (let row = 0; row < rows; row += 1) {
      const id = col * rows + row;
      glyphs.push({
        id,
        x: 2 + (col / (columns - 1)) * 96 + (((row + col) % 3) - 1) * 0.22,
        y: 1 + (row / (rows - 1)) * 98,
        size: 12 + ((col + row) % 4),
        opacity: 0.08 + (((id * 7) % 10) / 100),
        drift: 18 + ((id * 5) % 18),
      });
    }
  }

  return glyphs;
}

function glyphStyle(glyph: Glyph, mouse: Point) {
  const dx = glyph.x - mouse.x;
  const dy = glyph.y - mouse.y;
  const distance = Math.sqrt(dx * dx + dy * dy);
  const influence = Math.max(0, 1 - distance / 18);
  const pushX = dx * influence * 0.22;
  const pushY = dy * influence * 0.22;
  const glow = influence * 0.2;
  const ink = Math.round(38 - influence * 22);

  return {
    left: `${glyph.x}%`,
    top: `${glyph.y}%`,
    fontSize: `${glyph.size}px`,
    opacity: glyph.opacity + glow,
    "--drift": `${glyph.drift}s`,
    color: `rgba(${ink}, ${ink + 10}, ${ink + 18}, ${0.64 + influence * 0.28})`,
    transform: `translate(${pushX}px, ${pushY}px) rotate(${(pushX - pushY) * 0.22}deg)`,
  } as JSX.CSSProperties;
}

function glyphText(glyph: Glyph, version: number) {
  const phase = Math.floor(version / (1 + (glyph.id % 5)));
  return glyphTokens[(glyph.id * 11 + phase * 7 + (glyph.id % 13)) % glyphTokens.length];
}

class ZephyrAsciiRenderer {
  private container: HTMLDivElement;
  private renderer: THREE.WebGLRenderer | null = null;
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(45, 1, 1, 1000);
  private mesh: THREE.Mesh<THREE.PlaneGeometry, THREE.ShaderMaterial> | null = null;
  private texture: THREE.CanvasTexture | null = null;
  private textCanvas = document.createElement("canvas");
  private sampleCanvas = document.createElement("canvas");
  private sampleContext = this.sampleCanvas.getContext("2d", { willReadFrequently: true });
  private pre = document.createElement("pre");
  private resizeObserver: ResizeObserver | null = null;
  private animationFrame = 0;
  private lastAsciiAt = 0;
  private mouse = { x: 0.5, y: 0.5 };
  private running = false;

  constructor(container: HTMLDivElement) {
    this.container = container;
    this.camera.position.z = 30;
    this.handlePointerMove = this.handlePointerMove.bind(this);
    this.handleVisibilityChange = this.handleVisibilityChange.bind(this);
  }

  async init() {
    this.pre.className = "ascii-logo-pre";
    this.container.appendChild(this.pre);

    this.renderer = new THREE.WebGLRenderer({ antialias: false, alpha: true });
    this.renderer.setPixelRatio(1);
    this.renderer.setClearColor(0xffffff, 0);

    this.createMesh();
    this.resize();

    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(this.container);
    this.container.addEventListener("pointermove", this.handlePointerMove);
    document.addEventListener("visibilitychange", this.handleVisibilityChange);
  }

  start() {
    this.running = true;
    this.animate();
  }

  dispose() {
    this.running = false;
    if (this.animationFrame) cancelAnimationFrame(this.animationFrame);
    this.resizeObserver?.disconnect();
    this.container.removeEventListener("pointermove", this.handlePointerMove);
    document.removeEventListener("visibilitychange", this.handleVisibilityChange);
    this.mesh?.geometry.dispose();
    this.mesh?.material.dispose();
    this.texture?.dispose();
    this.scene.clear();
    this.renderer?.dispose();
    this.renderer?.forceContextLoss();
    this.pre.remove();
  }

  private createMesh() {
    const context = this.textCanvas.getContext("2d");
    if (!context) return;

    this.texture = new THREE.CanvasTexture(this.textCanvas);
    this.texture.minFilter = THREE.NearestFilter;
    this.texture.magFilter = THREE.NearestFilter;

    const geometry = new THREE.PlaneGeometry(1, 1, 36, 16);
    const material = new THREE.ShaderMaterial({
      vertexShader,
      fragmentShader,
      transparent: true,
      uniforms: {
        uTime: { value: 0 },
        uTexture: { value: this.texture },
        uEnableWaves: { value: 1 },
      },
    });

    this.mesh = new THREE.Mesh(geometry, material);
    this.scene.add(this.mesh);
  }

  private resize() {
    const rect = this.container.getBoundingClientRect();
    const width = Math.max(1, rect.width);
    const height = Math.max(1, rect.height);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer?.setSize(width, height, false);
    this.drawTextTexture(width);
    this.resetAsciiCanvas(width, height);
  }

  private drawTextTexture(width: number) {
    const context = this.textCanvas.getContext("2d");
    if (!context || !this.texture) return;

    const fontSize = Math.round(Math.min(240, Math.max(96, width * 0.18)));
    const font = `600 ${fontSize}px Georgia, Cambria, "Times New Roman", serif`;
    context.font = font;
    const metrics = context.measureText("Zephyr");
    const canvasWidth = Math.ceil(metrics.width + 48);
    const canvasHeight = Math.ceil(fontSize * 1.2);
    this.textCanvas.width = canvasWidth;
    this.textCanvas.height = canvasHeight;
    context.clearRect(0, 0, canvasWidth, canvasHeight);
    context.fillStyle = "rgba(35, 43, 49, 0.88)";
    context.font = font;
    context.textBaseline = "middle";
    context.fillText("Zephyr", 24, canvasHeight / 2);
    this.texture.needsUpdate = true;

    if (this.mesh) {
      const aspect = canvasWidth / canvasHeight;
      const fov = THREE.MathUtils.degToRad(this.camera.fov);
      const visibleHeight = 2 * Math.tan(fov / 2) * this.camera.position.z;
      const visibleWidth = visibleHeight * this.camera.aspect;
      const targetWidth = Math.min(visibleWidth * 0.86, visibleHeight * 2.35);
      const targetHeight = targetWidth / aspect;
      this.mesh.position.set(0, 0, 0);
      this.mesh.scale.set(targetWidth, targetHeight, 1);
    }
  }

  private resetAsciiCanvas(width: number, height: number) {
    const fontSize = width < 640 ? 9 : 10;
    const cols = Math.max(48, Math.floor(width / (fontSize * 0.62)));
    const rows = Math.max(28, Math.floor(height / fontSize));
    this.sampleCanvas.width = cols;
    this.sampleCanvas.height = rows;
    this.pre.style.fontSize = `${fontSize}px`;
    this.pre.style.lineHeight = `${fontSize}px`;
  }

  private animate = () => {
    if (!this.running) return;
    if (document.visibilityState !== "visible") {
      this.animationFrame = 0;
      return;
    }

    const time = performance.now() * 0.001;
    if (this.mesh) {
      this.mesh.rotation.x += ((0.5 - this.mouse.y) * 0.026 - this.mesh.rotation.x) * 0.014;
      this.mesh.rotation.y += ((this.mouse.x - 0.5) * 0.038 - this.mesh.rotation.y) * 0.014;
      this.mesh.material.uniforms.uTime.value = time * 0.35;
    }

    this.renderer?.render(this.scene, this.camera);
    if (time - this.lastAsciiAt > 0.034) {
      this.lastAsciiAt = time;
      this.asciify();
    }
    this.animationFrame = requestAnimationFrame(this.animate);
  };

  private asciify() {
    if (!this.renderer || !this.sampleContext) return;
    const width = this.sampleCanvas.width;
    const height = this.sampleCanvas.height;
    this.sampleContext.clearRect(0, 0, width, height);
    this.sampleContext.drawImage(this.renderer.domElement, 0, 0, width, height);
    const data = this.sampleContext.getImageData(0, 0, width, height).data;
    const charset = " .'`^,:;Il!i~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";
    let output = "";

    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        const index = (x + y * width) * 4;
        const alpha = data[index + 3];
        if (alpha < 10) {
          output += " ";
          continue;
        }
        const gray = (0.3 * data[index] + 0.6 * data[index + 1] + 0.1 * data[index + 2]) / 255;
        const charIndex = Math.max(0, Math.min(charset.length - 1, Math.floor((1 - gray) * (charset.length - 1))));
        output += charset[charIndex];
      }
      output += "\n";
    }

    this.pre.textContent = output;
  }

  private handlePointerMove(event: PointerEvent) {
    const rect = this.container.getBoundingClientRect();
    this.mouse = {
      x: (event.clientX - rect.left) / rect.width,
      y: (event.clientY - rect.top) / rect.height,
    };
  }

  private handleVisibilityChange() {
    if (document.visibilityState !== "visible" && this.animationFrame) {
      cancelAnimationFrame(this.animationFrame);
      this.animationFrame = 0;
      return;
    }
    if (document.visibilityState === "visible" && this.running && !this.animationFrame) {
      this.animate();
    }
  }
}
