import { useEffect, useCallback, useState, useMemo, useRef } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useRecording } from "../hooks/useRecording";
import { loadConfig, DEFAULT_CONFIG, type AppConfig } from "../lib/store";
import { normalizeHex, widgetClassFromConfig, widgetStyleFromConfig } from "../lib/widgetTheme";
import { IS_LINUX } from "../lib/platform";
import "./FloatingWidget.css";

/** Tauri `ResizeDirection` — тип не экспортирован из API, дублируем для `startResizeDragging`. */
type ResizeDirection =
  | "East"
  | "North"
  | "NorthEast"
  | "NorthWest"
  | "South"
  | "SouthEast"
  | "SouthWest"
  | "West";

const RESIZE_CORNERS: readonly { suffix: string; dir: ResizeDirection }[] = [
  { suffix: "nw", dir: "NorthWest" },
  { suffix: "ne", dir: "NorthEast" },
  { suffix: "sw", dir: "SouthWest" },
  { suffix: "se", dir: "SouthEast" },
];

const RESIZE_EDGES: readonly { suffix: string; dir: ResizeDirection }[] = [
  { suffix: "n", dir: "North" },
  { suffix: "s", dir: "South" },
  { suffix: "e", dir: "East" },
  { suffix: "w", dir: "West" },
];

/** Проверяет, что точка (cx, cy) попадает внутрь скруглённого прямоугольника el.
 *  Без этой проверки клик в «прозрачном» угловом треугольнике запускает
 *  startDragging/startResizeDragging → DWM рисует угол чёрным на один кадр. */
function isInsideRoundedRect(cx: number, cy: number, el: HTMLElement): boolean {
  const r = parseFloat(getComputedStyle(el).borderTopLeftRadius) || 0;
  if (r === 0) return true;
  const rect = el.getBoundingClientRect();
  const dx = Math.max(rect.left + r - cx, 0, cx - (rect.right - r));
  const dy = Math.max(rect.top + r - cy, 0, cy - (rect.bottom - r));
  return dx * dx + dy * dy <= r * r;
}

function beginResize(direction: ResizeDirection) {
  return (e: ReactMouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const widget = (e.currentTarget as HTMLElement).closest<HTMLElement>(".widget");
    if (widget && !isInsideRoundedRect(e.clientX, e.clientY, widget)) return;
    void getCurrentWindow().startResizeDragging(direction);
  };
}

/** data-tauri-drag-region на WebView2/прозрачных окнах не всегда срабатывает; курсор — из CSS. */
function tryStartWindowMove(e: ReactMouseEvent) {
  if (e.button !== 0) return;
  const el = e.target as HTMLElement;
  if (el.closest("button, a, input, textarea, select, label")) return;
  if (el.closest(".widget-resize")) return;
  if (!isInsideRoundedRect(e.clientX, e.clientY, e.currentTarget as HTMLElement)) return;
  void getCurrentWindow().startDragging();
}

export default function FloatingWidget() {
  const [widgetConfig, setWidgetConfig] = useState<AppConfig>(DEFAULT_CONFIG);
  const [ready, setReady] = useState(false);
  const [errorTooltipOpen, setErrorTooltipOpen] = useState(false);
  const [agentFeedback, setAgentFeedback] = useState<string | null>(null);
  const agentFeedbackTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const hideTooltipTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const { state, lastText, error, startRecording, stopRecording, cancelTranscription, duration } =
    useRecording();

  // Close tooltip when error clears
  useEffect(() => {
    if (state !== "error") setErrorTooltipOpen(false);
  }, [state]);

  const onTooltipEnter = useCallback(() => {
    if (hideTooltipTimer.current) clearTimeout(hideTooltipTimer.current);
    setErrorTooltipOpen(true);
  }, []);

  const onTooltipLeave = useCallback(() => {
    hideTooltipTimer.current = setTimeout(() => setErrorTooltipOpen(false), 120);
  }, []);

  const isRecording = state === "recording";
  const isTranscribing = state === "transcribing";

  const widgetStyle = useMemo(() => widgetStyleFromConfig(widgetConfig), [widgetConfig]);
  const widgetClassName = useMemo(
    () => [
      "widget",
      "widget-animated-in",
      widgetClassFromConfig(widgetConfig),
      isRecording ? "is-recording" : "",
    ].filter(Boolean).join(" "),
    [widgetConfig, isRecording],
  );

  useEffect(() => {
    const setup = async () => {
      const u1 = await listen("ptt-pressed", () => startRecording());
      const u2 = await listen("ptt-released", () => stopRecording());
      const u3 = await listen<string>("plugin-command", () => {
        // Command was handled — clear thinking badge
        if (agentFeedbackTimer.current) clearTimeout(agentFeedbackTimer.current);
        setAgentFeedback(null);
      });
      const u4 = await listen<string>("plugin-feedback", (e) => {
        setAgentFeedback(e.payload);
        if (agentFeedbackTimer.current) clearTimeout(agentFeedbackTimer.current);
        agentFeedbackTimer.current = setTimeout(() => setAgentFeedback(null), 15000);
      });
      return () => { u1(); u2(); u3(); u4(); };
    };
    const cleanup = setup();
    return () => { cleanup.then((fn) => fn?.()); };
  }, [startRecording, stopRecording]);

  useEffect(() => {
    const refresh = () => {
      loadConfig()
        .then((cfg) => { setWidgetConfig(cfg); setReady(true); })
        .catch(() => { setWidgetConfig(DEFAULT_CONFIG); setReady(true); });
    };
    // На старте: загружаем конфиг из store И синхронизируем его с Rust-бэкендом,
    // чтобы PTT-хоткей и транскрипция работали сразу (без открытия настроек).
    loadConfig()
      .then((cfg) => {
        setWidgetConfig(cfg);
        setReady(true);
        void invoke("apply_config", { config: cfg });
      })
      .catch(() => { setWidgetConfig(DEFAULT_CONFIG); setReady(true); });
    const p = listen("config-updated", refresh);
    return () => { p.then((u) => u()); };
  }, []);

  /* Скругление должно жить на :root: иначе body/#root остаются 0px, у WebView2 видны квадратные углы. */
  useEffect(() => {
    const isRound = widgetConfig.widgetCornerStyle === "round";
    const r = 10;
    const el = document.documentElement;
    el.style.setProperty("--widget-corners", isRound ? `${r}px` : "0");
    el.setAttribute("data-corners", isRound ? "round" : "sharp");
    return () => {
      el.style.removeProperty("--widget-corners");
      el.removeAttribute("data-corners");
    };
  }, [widgetConfig.widgetCornerStyle]);

  /* Подложка под весь document = цвет фона виджета, иначе у top/left видна «чёрная кромка» WebView.
     На Linux пропускаем: окно прозрачное, фон обеспечивает .widget div, иначе углы будут тёмными. */
  useEffect(() => {
    if (IS_LINUX) return;
    const bg = normalizeHex(widgetConfig.widgetBgTo, DEFAULT_CONFIG.widgetBgTo);
    const root = document.documentElement;
    const b = document.body;
    const mount = document.getElementById("root");
    root.style.backgroundColor = bg;
    b.style.backgroundColor = bg;
    if (mount) mount.style.backgroundColor = bg;
    return () => {
      root.style.removeProperty("background-color");
      b.style.removeProperty("background-color");
      if (mount) mount.style.removeProperty("background-color");
    };
  }, [widgetConfig.widgetBgTo]);

  /* Показываем окно только после первого рендера с правильными цветами.
     При автозапуске (--autostart) окно остаётся скрытым — только трей. */
  useEffect(() => {
    if (!ready) return;
    invoke<boolean>("is_autostart_launch").then((autostart) => {
      if (!autostart) {
        requestAnimationFrame(() => void getCurrentWindow().show());
      }
    }).catch(() => {
      requestAnimationFrame(() => void getCurrentWindow().show());
    });
  }, [ready]);

  const handleMouseDown = useCallback(() => startRecording(), [startRecording]);
  const handleMouseUp = useCallback(() => stopRecording(), [stopRecording]);

  const openSettings = useCallback(() => {
    invoke("open_settings").catch(console.error);
  }, []);

  const minimizeWidget = useCallback(() => {
    invoke("minimize_widget").catch(console.error);
  }, []);

  const quitApp = useCallback(() => {
    invoke("quit_app").catch(console.error);
  }, []);

  return (
    <div
      className={widgetClassName}
      style={widgetStyle}
      onMouseDown={tryStartWindowMove}
    >
      <div className="widget-fx" aria-hidden="true" />
      {RESIZE_CORNERS.map(({ suffix, dir }) => (
        <div
          key={suffix}
          role="presentation"
          className={`widget-resize widget-resize--corner widget-resize--${suffix}`}
          title="Изменить размер"
          onMouseDown={beginResize(dir)}
        />
      ))}
      {RESIZE_EDGES.map(({ suffix, dir }) => (
        <div
          key={suffix}
          role="presentation"
          className={`widget-resize widget-resize--edge widget-resize--${suffix}`}
          title="Изменить размер"
          onMouseDown={beginResize(dir)}
        />
      ))}
      <div className="widget-header">
        <span className="widget-title">easySTT</span>
        <div className="header-actions">
          <button className="minimize-btn" onClick={minimizeWidget} title="Свернуть">—</button>
          <button className="settings-btn" onClick={openSettings} title="Настройки">⚙</button>
          <button className="close-btn" onClick={quitApp} title="Закрыть">✕</button>
        </div>
      </div>

      <div className="widget-body">
        <div className="widget-body-row">
          <button
            className={`record-btn ${isRecording ? "recording" : ""} ${isTranscribing ? "transcribing" : ""}`}
            onMouseDown={handleMouseDown}
            onMouseUp={handleMouseUp}
            onTouchStart={handleMouseDown}
            onTouchEnd={handleMouseUp}
            disabled={isTranscribing}
          >
            {isRecording && <><span className="pulse-ring" /><span className="pulse-ring-2" /></>}
            <span className="record-icon">
              {isTranscribing ? "⏳" : isRecording ? "⏹" : "🎙"}
            </span>
          </button>

          <div className="widget-status">
            {isRecording && (
              <>
                <div className="wave-bars">
                  <span className="wave-bar" />
                  <span className="wave-bar" />
                  <span className="wave-bar" />
                  <span className="wave-bar" />
                  <span className="wave-bar" />
                </div>
                <span className="recording-timer">{duration}с</span>
              </>
            )}
            {isTranscribing && (
              <div className="transcribe-row" aria-live="polite">
                <div className="dots" aria-hidden>
                  <span className="dot" /><span className="dot" /><span className="dot" />
                </div>
                <span className="transcribing-label">Обработка...</span>
                <button
                  type="button"
                  className="btn-cancel-transcribe"
                  onClick={() => {
                    void cancelTranscription();
                  }}
                  title="Прервать, если зависло"
                >
                  Отмена
                </button>
              </div>
            )}
            {agentFeedback && state === "idle" && (
              <span className="status-agent-thinking">
                <span className="agent-thinking-dot" />
                {agentFeedback}
              </span>
            )}
            {state === "idle" && !lastText && !agentFeedback && (
              <span className="status-hint">
                Удерживай для&nbsp;записи
                {widgetConfig.translateEnabled && (
                  <span className="translate-badge">
                    {widgetConfig.translateDirection === "en_to_ru" ? "EN → RU" : "RU → EN"}
                  </span>
                )}
              </span>
            )}
            {state === "idle" && lastText && !agentFeedback && (
              <span className="status-ok">✓ Вставлено</span>
            )}
            {state === "error" && !error && (
              <span className="status-error-fallback">✕ Ошибка</span>
            )}
            {state === "error" && error && (
              <span
                className="error-tooltip-trigger status-error-title"
                onMouseEnter={onTooltipEnter}
                onMouseLeave={onTooltipLeave}
              >Ошибка</span>
            )}
          </div>
        </div>

        {/* Error tooltip */}
        {state === "error" && error && errorTooltipOpen && (
          <div
            className="error-tooltip"
            role="tooltip"
            onMouseEnter={onTooltipEnter}
            onMouseLeave={onTooltipLeave}
          >{error}</div>
        )}

      </div>
    </div>
  );
}
