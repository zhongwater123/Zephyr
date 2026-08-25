import { render } from "preact";
import "./styles.css";
import { FrontendErrorBoundary, installGlobalFrontendIncidentCapture } from "./app/FrontendErrorBoundary";
import "./styles-v2.css";

const isPreinput = new URLSearchParams(window.location.search).get("window") === "preinput";

installGlobalFrontendIncidentCapture();
async function mount() {
  const root = document.getElementById("app");
  if (!root) throw new Error("missing app root");
  if (isPreinput) {
    const { PreInputOverlay } = await import("./preinput/PreInputOverlay");
    render(<FrontendErrorBoundary><PreInputOverlay /></FrontendErrorBoundary>, root);
    return;
  }
  const { App } = await import("./app/App");
  render(<FrontendErrorBoundary><App /></FrontendErrorBoundary>, root);
}

void mount();
