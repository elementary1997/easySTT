import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import FloatingWidget from "./components/FloatingWidget";
import SettingsPanel from "./components/SettingsPanel";

type WindowLabel = "widget" | "settings";

export default function App() {
  const [windowLabel, setWindowLabel] = useState<WindowLabel>("widget");

  useEffect(() => {
    getCurrentWindow().label === "settings"
      ? setWindowLabel("settings")
      : setWindowLabel("widget");
  }, []);

  if (windowLabel === "settings") return <SettingsPanel />;
  return <FloatingWidget />;
}
