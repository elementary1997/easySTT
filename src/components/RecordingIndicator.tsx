import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./RecordingIndicator.css";

type State = "listening" | "recording" | "transcribing" | "command";

export default function RecordingIndicator() {
  const [state, setState] = useState<State>("listening");

  useEffect(() => {
    const subs = [
      listen("always-on-vad",            () => setState("listening")),
      listen("ptt-pressed",              () => setState("recording")),
      listen("ptt-released",             () => setState("transcribing")),
      listen("transcription-done",       () => setState("transcribing")),
      listen("transcription-error",      () => setState("transcribing")),
      listen("transcription-cancelled",  () => setState("transcribing")),
      listen("plugin-command",           () => setState("command")),
    ];
    return () => { subs.forEach(p => p.then(u => u())); };
  }, []);

  return (
    <div className="indicator">
      {state === "listening" && (
        <>
          <span className="indicator-wave" />
          <span>Слушаю...</span>
        </>
      )}
      {state === "recording" && (
        <>
          <span className="indicator-dot" />
          <span>Запись...</span>
        </>
      )}
      {state === "transcribing" && (
        <>
          <div className="indicator-spinner">
            <span /><span /><span />
          </div>
          <span>Обработка...</span>
        </>
      )}
      {state === "command" && (
        <>
          <span className="indicator-check" />
          <span>Команда выполнена</span>
        </>
      )}
    </div>
  );
}
