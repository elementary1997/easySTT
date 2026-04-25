import { useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useRecording } from "../hooks/useRecording";
import "./FloatingWidget.css";

export default function FloatingWidget() {
  const { state, lastText, error, startRecording, stopRecording, duration } =
    useRecording();

  // PTT events from Rust (global hotkey)
  useEffect(() => {
    const setup = async () => {
      const u1 = await listen("ptt-pressed", () => startRecording());
      const u2 = await listen("ptt-released", () => stopRecording());
      return () => { u1(); u2(); };
    };
    const cleanup = setup();
    return () => { cleanup.then((fn) => fn?.()); };
  }, [startRecording, stopRecording]);

  const handleMouseDown = useCallback(() => startRecording(), [startRecording]);
  const handleMouseUp = useCallback(() => stopRecording(), [stopRecording]);

  const openSettings = useCallback(() => {
    invoke("open_settings").catch(console.error);
  }, []);

  const isRecording = state === "recording";
  const isTranscribing = state === "transcribing";

  return (
    <div className="widget" data-tauri-drag-region>
      <div className="widget-header" data-tauri-drag-region>
        <span className="widget-title">easySTT</span>
        <button className="settings-btn" onClick={openSettings} title="Настройки">
          ⚙
        </button>
      </div>

      <div className="widget-body">
        <button
          className={`record-btn ${isRecording ? "recording" : ""} ${isTranscribing ? "transcribing" : ""}`}
          onMouseDown={handleMouseDown}
          onMouseUp={handleMouseUp}
          onTouchStart={handleMouseDown}
          onTouchEnd={handleMouseUp}
          disabled={isTranscribing}
        >
          {isRecording && (
            <span className="pulse-ring" />
          )}
          <span className="record-icon">
            {isTranscribing ? "⏳" : isRecording ? "⏹" : "🎙"}
          </span>
          <span className="record-label">
            {isTranscribing
              ? "Обработка..."
              : isRecording
              ? `${duration}с`
              : "Держи"}
          </span>
        </button>

        {state === "error" && (
          <div className="status-error" title={error ?? ""}>
            ✕ Ошибка
          </div>
        )}
        {lastText && state === "idle" && (
          <div className="status-ok" title={lastText}>
            ✓ Вставлено
          </div>
        )}
      </div>
    </div>
  );
}
