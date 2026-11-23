import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { showShellWindow } from "./ipc/showWindow";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
// The window starts hidden. Show it from a task after the render call, not from
// requestAnimationFrame: a hidden webview never runs animation frames.
setTimeout(() => void showShellWindow(), 0);
