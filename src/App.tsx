import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import FloatingWidget from "./components/FloatingWidget";
import SettingsPanel from "./components/SettingsPanel";
import RecordingIndicator from "./components/RecordingIndicator";

type WindowLabel = "widget" | "settings" | "indicator";

export default function App() {
  const [windowLabel, setWindowLabel] = useState<WindowLabel>("widget");

  useEffect(() => {
    const label = getCurrentWindow().label as WindowLabel;
    setWindowLabel(label);
    if (label === "widget") {
      document.documentElement.classList.add("widget-surface");
      document.body.classList.add("widget-surface");
    } else if (label === "settings") {
      document.documentElement.classList.add("settings-surface");
    } else if (label === "indicator") {
      document.documentElement.classList.add("indicator-surface");
    }
    return () => {
      document.documentElement.classList.remove("widget-surface");
      document.body.classList.remove("widget-surface");
      document.documentElement.classList.remove("settings-surface");
      document.documentElement.classList.remove("indicator-surface");
    };
  }, []);

  if (windowLabel === "settings")  return <SettingsPanel />;
  if (windowLabel === "indicator") return <RecordingIndicator />;
  return <FloatingWidget />;
}
