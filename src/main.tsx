import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import "./index.css";
import { IS_LINUX } from "./lib/platform";

// На Linux виджет использует прозрачное окно (tauri.linux.conf.json transparent:true),
// поэтому фон html/body не выставляем — его обеспечит сам .widget div через border-radius.
document.documentElement.setAttribute("data-os", IS_LINUX ? "linux" : "other");

const WIDGET_BG = "#1a1a2e";
try {
  if (getCurrentWindow().label === "widget") {
    const h = document.documentElement;
    const b = document.body;
    h.classList.add("widget-surface");
    b.classList.add("widget-surface");
    h.setAttribute("data-corners", "round");
    h.style.setProperty("--widget-corners", "10px");
    if (!IS_LINUX) {
      h.style.backgroundColor = WIDGET_BG;
      b.style.backgroundColor = WIDGET_BG;
      const root = document.getElementById("root");
      if (root) root.style.backgroundColor = WIDGET_BG;
    }
  }
} catch {
  /* не в оболочке Tauri */
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
