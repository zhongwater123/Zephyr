import { Component, type ComponentChildren } from "preact";
import { incidentApi } from "../ipc/client";

export class FrontendErrorBoundary extends Component<
  { children: ComponentChildren },
  { failed: boolean }
> {
  state = { failed: false };

  componentDidCatch(error: Error) {
    this.setState({ failed: true });
    void incidentApi.recordFrontend({
      source: "error_boundary",
      code: "frontend_render_failed",
      message: error.message || "render failed",
      stack: error.stack,
    }).catch(() => undefined);
  }

  render() {
    if (this.state.failed) {
      return (
        <main className="frontend-failure" role="alert">
          <h1>界面暂时无法显示</h1>
          <p>异常信息已留在本机。请重新启动 Zephyr；语音主链路不会因诊断记录失败而改变。</p>
          <button type="button" onClick={() => window.location.reload()}>重新加载</button>
        </main>
      );
    }
    return this.props.children;
  }
}

export function installGlobalFrontendIncidentCapture() {
  const seen = new Map<string, number>();
  const report = (source: string, code: string, message: string, stack?: string | null) => {
    const key = `${source}:${message}`.slice(0, 256);
    const now = Date.now();
    if (now - (seen.get(key) ?? 0) < 10_000) return;
    seen.set(key, now);
    if (seen.size > 64) {
      const oldest = [...seen.entries()].sort((a, b) => a[1] - b[1]).slice(0, 16);
      oldest.forEach(([entry]) => seen.delete(entry));
    }
    void incidentApi.recordFrontend({ source, code, message, stack }).catch(() => undefined);
  };

  window.addEventListener("error", (event) => {
    report(
      "window.error",
      "frontend_uncaught_error",
      event.message || "uncaught frontend error",
      event.error instanceof Error ? event.error.stack : null,
    );
  });
  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    report(
      "unhandledrejection",
      "frontend_unhandled_rejection",
      reason instanceof Error ? reason.message : String(reason),
      reason instanceof Error ? reason.stack : null,
    );
  });
}
