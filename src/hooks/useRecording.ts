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

  useEffect(() => {
    const setup = async () => {
      const u1 = await listen<string>("transcription-done", (e) => {
        setLastText(e.payload);
        setState("idle");
      });
      const u2 = await listen<string>("transcription-error", (e) => {
        setError(e.payload);
        setState("error");
      });
      const u3 = await listen<string>("transcription-cancelled", () => {
        setState("idle");
        setError(null);
      });
      unlistenRef.current = [u1, u2, u3];
    };
    setup();
    return () => {
      unlistenRef.current.forEach((u) => u());
    };
  }, []);

  const startRecording = useCallback(async () => {
    setError(null);
    setState("recording");
    setDuration(0);
    timerRef.current = setInterval(() => setDuration((d) => d + 1), 1000);
    try {
      await invoke("start_recording");
    } catch (e) {
      setState("error");
      setError(String(e));
    }
  }, []);

  const stopRecording = useCallback(async () => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    if (state !== "recording") return;
    setState("transcribing");
    try {
      await invoke("stop_and_transcribe");
    } catch (e) {
      setState("error");
      setError(String(e));
    }
  }, [state]);

  const cancelTranscription = useCallback(async () => {
    // Optimistically return UI to idle immediately, backend continues cancellation.
    setState("idle");
    setError(null);
    try {
      await invoke("cancel_transcription");
    } catch (e) {
      setState("error");
      setError(String(e));
    }
  }, []);

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
