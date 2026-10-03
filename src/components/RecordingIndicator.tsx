import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./RecordingIndicator.css";

type State = "recording" | "transcribing" | "done" | "error";

export default function RecordingIndicator() {
  const [state, setState] = useState<State>("recording");

  useEffect(() => {
    // Capture success and transcription lifecycle come from the backend;
    // a key press alone does not mean that a microphone is recording.
    const subscription = listen<State>("recording-status", (event) => {
      if (["recording", "transcribing", "done", "error"].includes(event.payload)) setState(event.payload);
    });
    return () => { void subscription.then((unlisten) => unlisten()); };
  }, []);

  const labels: Record<State, string> = {
    recording: "Запись…", transcribing: "Обработка…", done: "Готово", error: "Ошибка обработки",
  };

  return (
    <div className="indicator" role="status" aria-live="polite" aria-label={labels[state]}>
      {state === "recording" && <><span className="indicator-dot" /><span>Запись…</span></>}
      {state === "transcribing" && <>
        <div className="indicator-spinner"><span /><span /><span /></div>
        <span>Обработка…</span>
      </>}
      {state === "done" && <><span className="indicator-check" /><span>Готово</span></>}
      {state === "error" && <><span className="indicator-error" /><span>Ошибка обработки</span></>}
    </div>
  );
}
