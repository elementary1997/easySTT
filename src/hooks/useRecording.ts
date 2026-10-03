import { useState, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

export type RecordingState = "idle" | "recording" | "transcribing" | "error";

export interface UseRecordingReturn {
  state: RecordingState;
  lastText: string;
  error: string | null;
  startRecording: () => Promise<void>;
  stopRecording: () => Promise<void>;
  /** Прервать зависшую обработку (локально, облако, OpenRouter). */
  cancelTranscription: () => Promise<void>;
  duration: number;
}

export function useRecording(): UseRecordingReturn {
  const [state, setState] = useState<RecordingState>("idle");
  const [lastText, setLastText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [duration, setDuration] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const unlistenRef = useRef<UnlistenFn[]>([]);
  const stateRef = useRef<RecordingState>("idle");
  const startingRef = useRef<Promise<unknown> | null>(null);
  const setRecordingState = useCallback((next: RecordingState) => {
    stateRef.current = next;
    setState(next);
  }, []);

  useEffect(() => {
    const setup = async () => {
      const u1 = await listen<string>("transcription-done", (e) => {
        setLastText(e.payload);
        setRecordingState("idle");
      });
      const u2 = await listen<string>("transcription-error", (e) => {
        setError(e.payload);
        setRecordingState("error");
      });
      const u3 = await listen<string>("transcription-cancelled", () => {
        setRecordingState("idle");
        setError(null);
      });
      unlistenRef.current = [u1, u2, u3];
    };
    setup();
    return () => {
      unlistenRef.current.forEach((u) => u());
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [setRecordingState]);

  const startRecording = useCallback(async () => {
    if (stateRef.current === "recording" || stateRef.current === "transcribing") return;
    setError(null);
    setRecordingState("recording");
    setDuration(0);
    timerRef.current = setInterval(() => setDuration((d) => d + 1), 1000);
    const starting = invoke("start_recording");
    startingRef.current = starting;
    try {
      await starting;
    } catch (e) {
      if (timerRef.current) { clearInterval(timerRef.current); timerRef.current = null; }
      setRecordingState("error");
      setError(String(e));
    } finally {
      if (startingRef.current === starting) startingRef.current = null;
    }
  }, [setRecordingState]);

  const stopRecording = useCallback(async () => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    // A quick tap may be released before the sound server finishes opening.
    await startingRef.current?.catch(() => {});
    if (stateRef.current !== "recording") return;
    setRecordingState("transcribing");
    try {
      await invoke("stop_and_transcribe");
    } catch (e) {
      setRecordingState("error");
      setError(String(e));
    }
  }, [setRecordingState]);

  const cancelTranscription = useCallback(async () => {
    // Optimistically return UI to idle immediately, backend continues cancellation.
    setRecordingState("idle");
    setError(null);
    try {
      await invoke("cancel_transcription");
    } catch (e) {
      setRecordingState("error");
      setError(String(e));
    }
  }, [setRecordingState]);

  return {
    state,
    lastText,
    error,
    startRecording,
    stopRecording,
    cancelTranscription,
    duration,
  };
}
