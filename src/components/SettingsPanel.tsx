import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { enable as autostartEnable, disable as autostartDisable, isEnabled as autostartIsEnabled } from "@tauri-apps/plugin-autostart";
import { AppConfig, DEFAULT_CONFIG, loadConfig, saveConfig } from "../lib/store";
import PluginsTab from "./PluginsTab";
import "./SettingsPanel.css";

type Tab = "general" | "look" | "backend" | "hotkey" | "plugins";


type ModelInfo = {
  id: string;
  displayName: string;
  filename: string;
  sizeMb: number;
  url: string;
  description: string;
  tag: string | null;
};

type DownloadProgress = {
  modelId: string;
  downloadedBytes: number;
  totalBytes: number;
  percent: number;
};

type ModelDownloadState = {
  modelId: string;
  percent: number;
  downloadedMb: number;
  totalMb: number;
} | null;

function codeToTauriKey(code: string): string {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  const map: Record<string, string> = {
    Space: "Space", Backquote: "`", Minus: "-", Equal: "=",
    BracketLeft: "[", BracketRight: "]", Backslash: "\\",
    Semicolon: ";", Quote: "'", Comma: ",", Period: ".", Slash: "/",
    Tab: "Tab", Enter: "Enter", Backspace: "Backspace", Delete: "Delete",
    Escape: "Escape", Insert: "Insert", Home: "Home", End: "End",
    PageUp: "PageUp", PageDown: "PageDown",
    ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
  };
  return map[code] ?? code;
}

export default function SettingsPanel() {
  const [config, setConfig] = useState<AppConfig>(DEFAULT_CONFIG);
  const [tab, setTab] = useState<Tab>("general");
  const [saved, setSaved] = useState(false);
  const [microphones, setMicrophones] = useState<string[]>([]);
  const [cloudTestStatus, setCloudTestStatus] = useState("");
  const [cloudruModelList, setCloudruModelList] = useState<string[]>([]);
  const [cloudTesting, setCloudTesting] = useState(false);
  const [openrouterTestStatus, setOpenrouterTestStatus] = useState("");
  const [openrouterModelList, setOpenrouterModelList] = useState<string[]>([]);
  const [openrouterTesting, setOpenrouterTesting] = useState(false);
  const [modelCatalog, setModelCatalog] = useState<ModelInfo[]>([]);
  const [modelExistMap, setModelExistMap] = useState<Record<string, boolean>>({});
  const [downloadState, setDownloadState] = useState<ModelDownloadState>(null);
  const [downloadError, setDownloadError] = useState<string>("");
  const [modelsDir, setModelsDir] = useState("");
  const [hotkeyCapturing, setHotkeyCapturing] = useState(false);
  const [capturedCombo, setCapturedCombo] = useState("");
  const captureRef = useRef(false);
  const [autostartEnabled, setAutostartEnabled] = useState(false);

  const refreshModelExistence = useCallback(async (catalog: ModelInfo[]) => {
    const entries = await Promise.all(
      catalog.map(async (m) => {
        const path = await invoke<string>("get_model_path", { name: m.id });
        const exists = await invoke<boolean>("model_exists", { path });
        return [m.id, exists] as [string, boolean];
      })
    );
    setModelExistMap(Object.fromEntries(entries));
  }, []);

  useEffect(() => {
    loadConfig().then(setConfig);
    invoke<string[]>("list_microphones").then(setMicrophones).catch(() => {});
    invoke<string>("get_local_accel_info").catch(() => {});
    invoke<string>("get_models_dir").then(setModelsDir).catch(() => {});
    invoke<ModelInfo[]>("get_model_catalog").then((catalog) => {
      setModelCatalog(catalog);
      refreshModelExistence(catalog);
    }).catch(() => {});
    autostartIsEnabled().then(setAutostartEnabled).catch(() => {});
  }, [refreshModelExistence]);

  // Listen for download progress/done/error events
  useEffect(() => {
    const unlisteners = Promise.all([
      listen<DownloadProgress>("model-download-progress", (e) => {
        const p = e.payload;
        setDownloadState({
          modelId: p.modelId,
          percent: p.percent,
          downloadedMb: Math.round(p.downloadedBytes / 1024 / 1024),
          totalMb: Math.round(p.totalBytes / 1024 / 1024),
        });
        setDownloadError("");
      }),
      listen<string>("model-download-done", (e) => {
        setDownloadState(null);
        setDownloadError("");
        invoke<ModelInfo[]>("get_model_catalog").then((catalog) => {
          refreshModelExistence(catalog);
        }).catch(() => {});
        // Auto-select the just-downloaded model
        setConfig((c) => ({ ...c, localModelName: e.payload }));
      }),
      listen<string>("model-download-error", () => {
        setDownloadState(null);
      }),
    ]);
    return () => {
      unlisteners.then((uns) => uns.forEach((u) => u()));
    };
  }, [refreshModelExistence]);


  const update = useCallback(<K extends keyof AppConfig>(key: K, val: AppConfig[K]) => {
    setConfig((c) => ({ ...c, [key]: val }));
  }, []);

  const handleSave = useCallback(async () => {
    await saveConfig(config);
    await invoke("apply_config", { config });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }, [config]);

  const handleDownload = useCallback(async (modelId: string) => {
    setDownloadError("");
    setDownloadState({ modelId, percent: 0, downloadedMb: 0, totalMb: 0 });
    try {
      await invoke<void>("download_model", { modelId });
    } catch (e) {
      setDownloadState(null);
      setDownloadError(String(e));
    }
  }, []);

  const handleCancelDownload = useCallback(() => {
    invoke("cancel_model_download").catch(() => {});
  }, []);

  const handleDeleteModel = useCallback(async (modelId: string) => {
    if (!confirm(`Удалить модель «${modelId}» с диска?`)) return;
    try {
      await invoke("delete_model", { name: modelId });
      // сброс активной модели если удалили выбранную
      setConfig((c) => c.localModelName === modelId ? { ...c, localModelName: "tiny" } : c);
      invoke<ModelInfo[]>("get_model_catalog").then((catalog) => {
        refreshModelExistence(catalog);
      }).catch(() => {});
    } catch (e) {
      alert(`Ошибка: ${e}`);
    }
  }, [refreshModelExistence]);

  const handleTestCloudru = useCallback(async () => {
    setCloudTesting(true);
    setCloudTestStatus("Проверяю...");
    setCloudruModelList([]);
    try {
      const res = await invoke<{ message: string; models: string[] }>("test_cloudru", {
        apiKey: config.cloudruApiKey,
        keyId: config.cloudruKeyId,
        baseUrl: config.cloudruBaseUrl,
      });
      setCloudTestStatus(res.message);
      setCloudruModelList(res.models);
      if (res.models.length) {
        setConfig((c) => {
          const next =
            c.cloudruModel && res.models.includes(c.cloudruModel)
              ? c.cloudruModel
              : res.models[0]!;
          return { ...c, cloudruModel: next };
        });
      }
    } catch (e) {
      setCloudTestStatus(String(e));
    } finally {
      setCloudTesting(false);
    }
  }, [config.cloudruApiKey, config.cloudruKeyId, config.cloudruBaseUrl]);

  const handleTestOpenrouter = useCallback(async () => {
    setOpenrouterTesting(true);
    setOpenrouterTestStatus("Проверяю...");
    setOpenrouterModelList([]);
    try {
      const res = await invoke<{ message: string; models: string[] }>("test_openrouter", {
        apiKey: config.openrouterApiKey,
      });
      setOpenrouterTestStatus(res.message);
      setOpenrouterModelList(res.models);
      if (res.models.length) {
        setConfig((c) => {
          const next =
            c.openrouterModel && res.models.includes(c.openrouterModel)
              ? c.openrouterModel
              : res.models[0]!;
          return { ...c, openrouterModel: next };
        });
      }
    } catch (e) {
      setOpenrouterTestStatus(String(e));
    } finally {
      setOpenrouterTesting(false);
    }
  }, [config.openrouterApiKey]);

  // Hotkey capture
  const startCapture = useCallback(() => {
    setCapturedCombo("");
    setHotkeyCapturing(true);
    captureRef.current = true;
  }, []);

  const cancelCapture = useCallback(() => {
    setHotkeyCapturing(false);
    captureRef.current = false;
    setCapturedCombo("");
  }, []);

  useEffect(() => {
    if (!hotkeyCapturing) return;

    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (!captureRef.current) return;

      const modifiers: string[] = [];
      if (e.ctrlKey) modifiers.push("Ctrl");
      if (e.altKey) modifiers.push("Alt");
      if (e.shiftKey) modifiers.push("Shift");
      if (e.metaKey) modifiers.push("Super");

      const isModOnly = ["Control", "Alt", "Shift", "Meta"].includes(e.key);

      if (isModOnly) {
        setCapturedCombo(modifiers.join("+") || "");
        return;
      }

      if (e.key === "Escape") {
        cancelCapture();
        return;
      }

      const key = codeToTauriKey(e.code);
      const combo = [...modifiers, key].join("+");
      setCapturedCombo(combo);
      update("hotkey", combo);
      setHotkeyCapturing(false);
      captureRef.current = false;
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [hotkeyCapturing, cancelCapture, update]);

  return (
    <div className="settings">
      <div className="settings-titlebar">
        <span className="settings-app-name">easySTT</span>
        <span className="settings-title">Настройки</span>
      </div>

      <div className="tabs">
        {(["general", "look", "backend", "hotkey", "plugins"] as Tab[]).map((t) => (
          <button key={t} className={`tab ${tab === t ? "active" : ""}`} onClick={() => setTab(t)}>
            {t === "general" ? "Основные"
              : t === "look" ? "Внешний вид"
              : t === "backend" ? "Распознавание"
              : t === "hotkey" ? "Горячие клавиши"
              : "🧩 Плагины"}
          </button>
        ))}
      </div>

      <div className="tab-content">
        {tab === "general" && (
          <div className="section">
            <label className="field">
              <span className="field-label">Микрофон</span>
              <select value={config.micDeviceName} onChange={(e) => update("micDeviceName", e.target.value)}>
                <option value="">По умолчанию</option>
                {microphones.map((m) => (
                  <option key={m} value={m}>{m}</option>
                ))}
              </select>
            </label>

            <label className="field">
              <span className="field-label">Язык распознавания</span>
              <select value={config.language} onChange={(e) => update("language", e.target.value as AppConfig["language"])}>
                <option value="auto">Авто (определить)</option>
                <option value="ru">Русский</option>
                <option value="en">English</option>
              </select>
            </label>

            <label className="field">
              <span className="field-label">Способ вставки текста</span>
              <select value={config.injectionMethod} onChange={(e) => update("injectionMethod", e.target.value as AppConfig["injectionMethod"])}>
                <option value="clipboard">Буфер обмена + Ctrl+V</option>
                <option value="typing">Прямой ввод (enigo)</option>
              </select>
            </label>

            <label className="field">
              <span className="field-label">Задержка вставки (мс)</span>
              <input
                type="number" min={50} max={1000} step={50}
                value={config.injectDelayMs}
                onChange={(e) => update("injectDelayMs", Number(e.target.value))}
              />
            </label>

            <label className="field checkbox">
              <input
                type="checkbox"
                checked={config.restoreClipboard}
                onChange={(e) => update("restoreClipboard", e.target.checked)}
              />
              <span>Восстанавливать буфер обмена после вставки</span>
            </label>

            <label className="field checkbox">
              <input
                type="checkbox"
                checked={autostartEnabled}
                onChange={async (e) => {
                  try {
                    if (e.target.checked) {
                      await autostartEnable();
                    } else {
                      await autostartDisable();
                    }
                    setAutostartEnabled(await autostartIsEnabled());
                  } catch (err) {
                    console.error("Autostart error:", err);
                  }
                }}
              />
              <span>Запускать при входе в систему</span>
            </label>

            <div className="field plugins-status-row">
              <span className="field-label">🧩 Голосовые команды</span>
              {config.plugins.length === 0 ? (
                <span className="plugins-status-none">
                  Не подключено —{" "}
                  <button className="btn-link" onClick={() => setTab("plugins")}>
                    добавить плагин
                  </button>
                </span>
              ) : (
                <span className="plugins-status-active">
                  {config.plugins.filter((p) => p.enabled).length} из {config.plugins.length} активен —{" "}
                  <button className="btn-link" onClick={() => setTab("plugins")}>
                    настроить
                  </button>
                </span>
              )}
            </div>

          </div>
        )}

        {tab === "look" && (
          <div className="section">
            <div className="field field-row-label">
              <span className="field-label">Палитра</span>
              <button
                type="button"
                className="btn-secondary btn-sm"
                onClick={() => {
                  setConfig((c) => ({
                    ...c,
                    widgetBgFrom: DEFAULT_CONFIG.widgetBgFrom,
                    widgetBgTo: DEFAULT_CONFIG.widgetBgTo,
                    widgetWave1: DEFAULT_CONFIG.widgetWave1,
                    widgetWave2: DEFAULT_CONFIG.widgetWave2,
                    widgetWave3: DEFAULT_CONFIG.widgetWave3,
                    widgetAnimation: DEFAULT_CONFIG.widgetAnimation,
                    widgetWaveForm: DEFAULT_CONFIG.widgetWaveForm,
                    widgetCornerStyle: DEFAULT_CONFIG.widgetCornerStyle,
                  }));
                }}
              >
                Сброс внешнего вида
              </button>
            </div>
            <div className="widget-color-grid">
              {(
                [
                  ["widgetBgFrom", "Фон (начало)"],
                  ["widgetBgTo", "Фон (конец)"],
                  ["widgetWave1", "Волна: цвет 1"],
                  ["widgetWave2", "Волна: цвет 2"],
                  ["widgetWave3", "Волна: цвет 3"],
                ] as const
              ).map(([key, label]) => (
                <label key={key} className="field color-swatch">
                  <span className="field-label">{label}</span>
                  <input
                    type="color"
                    value={config[key]}
                    onChange={(e) => update(key, e.target.value)}
                  />
                </label>
              ))}
            </div>

            <label className="field">
              <span className="field-label">Анимация верхней волны</span>
              <select
                value={config.widgetAnimation}
                onChange={(e) => update("widgetAnimation", e.target.value as AppConfig["widgetAnimation"])}
              >
                <option value="flow">Поток — перелив и движение (по умолчанию)</option>
                <option value="breathe">Дыхание — мягче, медленнее</option>
                <option value="aurora">Аврора — плавающие цветные орбы</option>
                <option value="static">Статик — без анимации, только градиент</option>
              </select>
            </label>

            <label className="field">
              <span className="field-label">Силуэт волны</span>
              <select
                value={config.widgetWaveForm}
                onChange={(e) => update("widgetWaveForm", e.target.value as AppConfig["widgetWaveForm"])}
              >
                <option value="rolling">Перекаты — два изгиба</option>
                <option value="smooth">Плавная — одна дуга</option>
                <option value="line">Почти линия — лёгкое волнение</option>
              </select>
            </label>

            <label className="field">
              <span className="field-label">Углы плавающего окна</span>
              <select
                value={config.widgetCornerStyle}
                onChange={(e) => update("widgetCornerStyle", e.target.value as AppConfig["widgetCornerStyle"])}
              >
                <option value="none">Прямоугольник</option>
                <option value="round">Скруглённые (10px)</option>
              </select>
            </label>

            <p className="hint">Сохраните, чтобы настройки применились к плавающему виджету: цвета, силуэт волны, анимация, скругление.</p>
          </div>
        )}

        {tab === "backend" && (
          <div className="section">
            <label className="field">
              <span className="field-label">Бэкенд распознавания</span>
              <select value={config.sttBackend} onChange={(e) => update("sttBackend", e.target.value as AppConfig["sttBackend"])}>
                <option value="cloudru">Cloud.ru (Foundation Models)</option>
                <option value="openrouter">OpenRouter</option>
                <option value="local">Локальная модель</option>
              </select>
            </label>

            {config.sttBackend === "cloudru" && (
              <>
                <label className="field">
                  <span className="field-label">Key Secret (секрет ключа)</span>
                  <input
                    type="password"
                    placeholder="секрет, показанный один раз при создании"
                    value={config.cloudruApiKey}
                    onChange={(e) => update("cloudruApiKey", e.target.value)}
                  />
                </label>
                <p className="hint">
                  Вставьте Key Secret из консоли. При 401 к тому же URL выполняется повтор с заголовком
                  «Api-Key» (тот же секрет).
                </p>
                <label className="field">
                  <span className="field-label">Base URL</span>
                  <input
                    type="text"
                    value={config.cloudruBaseUrl}
                    onChange={(e) => update("cloudruBaseUrl", e.target.value)}
                  />
                </label>
                <div className="model-status-row">
                  <button className="btn-secondary" onClick={handleTestCloudru} disabled={cloudTesting || !config.cloudruApiKey}>
                    {cloudTesting ? "Проверяю..." : "Проверить соединение"}
                  </button>
                  {cloudTestStatus && (
                    <span className={`model-status ${cloudTestStatus.startsWith("✓") ? "status-success" : "status-fail"}`}>
                      {cloudTestStatus}
                    </span>
                  )}
                </div>
                {cloudruModelList.length > 0 ? (
                  <label className="field">
                    <span className="field-label">Модель</span>
                    <select
                      value={
                        cloudruModelList.includes(config.cloudruModel)
                          ? config.cloudruModel
                          : cloudruModelList[0]
                      }
                      onChange={(e) => update("cloudruModel", e.target.value)}
                    >
                      {!cloudruModelList.includes(config.cloudruModel) && config.cloudruModel && (
                        <option value={config.cloudruModel}>{config.cloudruModel} (текущее, не в списке)</option>
                      )}
                      {cloudruModelList.map((id) => (
                        <option key={id} value={id}>
                          {id}
                        </option>
                      ))}
                    </select>
                    <p className="hint">Список с сервера (GET /v1/models). Сохраните, чтобы применить.</p>
                  </label>
                ) : (
                  <label className="field">
                    <span className="field-label">Id модели</span>
                    <input
                      type="text"
                      value={config.cloudruModel}
                      onChange={(e) => update("cloudruModel", e.target.value)}
                      placeholder="openai/whisper-large-v3"
                    />
                    <p className="hint">После успешной проверки соединения появится выпадающий список доступных id.</p>
                  </label>
                )}
              </>
            )}

            {config.sttBackend === "openrouter" && (
              <>
                <label className="field">
                  <span className="field-label">API ключ OpenRouter</span>
                  <input
                    type="password" placeholder="sk-or-..."
                    value={config.openrouterApiKey}
                    onChange={(e) => update("openrouterApiKey", e.target.value)}
                  />
                </label>
                <div className="model-status-row">
                  <button
                    className="btn-secondary"
                    onClick={handleTestOpenrouter}
                    disabled={openrouterTesting || !config.openrouterApiKey?.trim()}
                  >
                    {openrouterTesting ? "Проверяю..." : "Проверить соединение"}
                  </button>
                  {openrouterTestStatus && (
                    <span
                      className={`model-status ${
                        openrouterTestStatus.startsWith("✓") ? "status-success" : "status-fail"
                      }`}
                    >
                      {openrouterTestStatus}
                    </span>
                  )}
                </div>
                {openrouterModelList.length > 0 ? (
                  <label className="field">
                    <span className="field-label">Модель</span>
                    <select
                      value={
                        openrouterModelList.includes(config.openrouterModel)
                          ? config.openrouterModel
                          : openrouterModelList[0]
                      }
                      onChange={(e) => update("openrouterModel", e.target.value)}
                    >
                      {!openrouterModelList.includes(config.openrouterModel) && config.openrouterModel && (
                        <option value={config.openrouterModel}>
                          {config.openrouterModel} (текущее, не в списке)
                        </option>
                      )}
                      {openrouterModelList.map((id) => (
                        <option key={id} value={id}>
                          {id}
                        </option>
                      ))}
                    </select>
                    <p className="hint">Список с API (GET /v1/models). Сохраните, чтобы применить.</p>
                  </label>
                ) : (
                  <label className="field">
                    <span className="field-label">Модель</span>
                    <input
                      type="text"
                      placeholder="openai/whisper-1"
                      value={config.openrouterModel}
                      onChange={(e) => update("openrouterModel", e.target.value)}
                    />
                    <p className="hint">
                      Нажмите «Проверить соединение» — появится список id. Для STT чаще всего подойдут id с
                      «whisper».
                    </p>
                  </label>
                )}
              </>
            )}

            {config.sttBackend === "local" && (
              <>
                {/* ── Каталог моделей ── */}
                <div className="model-catalog">
                  {modelCatalog.length === 0 ? (
                    <p className="hint">Загрузка каталога...</p>
                  ) : (
                    modelCatalog.map((m) => {
                      const exists = modelExistMap[m.id] ?? false;
                      const isSelected = config.localModelName === m.id;
                      const isDownloading =
                        downloadState !== null && downloadState.modelId === m.id;
                      return (
                        <div
                          key={m.id}
                          className={`model-card ${isSelected ? "model-card--active" : ""}`}
                          onClick={() => exists && update("localModelName", m.id)}
                          role="button"
                          tabIndex={0}
                          onKeyDown={(e) => e.key === "Enter" && exists && update("localModelName", m.id)}
                        >
                          <div className="model-card-header">
                            <span className="model-card-name">
                              {m.displayName}
                              {m.tag && <span className="model-tag">{m.tag}</span>}
                            </span>
                            <span className="model-card-size">{m.sizeMb >= 1024
                              ? `${(m.sizeMb / 1024).toFixed(1)} ГБ`
                              : `${m.sizeMb} МБ`}
                            </span>
                          </div>
                          <p className="model-card-desc">{m.description}</p>
                          {isDownloading && downloadState && (
                            <div className="model-download-bar">
                              <div
                                className="model-download-fill"
                                style={{ width: `${downloadState.percent}%` }}
                              />
                              <span className="model-download-label">
                                {downloadState.downloadedMb} / {downloadState.totalMb} МБ
                                ({downloadState.percent.toFixed(0)}%)
                              </span>
                            </div>
                          )}
                          <div className="model-card-actions">
                            <div className="model-card-actions-left">
                              {exists ? (
                                <span className="model-status-badge model-status-badge--ok">✓ Скачана</span>
                              ) : (
                                <span className="model-status-badge model-status-badge--missing">Не скачана</span>
                              )}
                              {isDownloading ? (
                                <button
                                  className="btn-secondary btn-sm"
                                  onClick={(e) => { e.stopPropagation(); handleCancelDownload(); }}
                                >
                                  Отмена
                                </button>
                              ) : (
                                <button
                                  className="btn-secondary btn-sm"
                                  disabled={downloadState !== null}
                                  onClick={(e) => { e.stopPropagation(); void handleDownload(m.id); }}
                                  title={exists ? "Скачать заново (перезаписать)" : "Скачать с HuggingFace"}
                                >
                                  {exists ? "Обновить" : "Скачать"}
                                </button>
                              )}
                              {exists && !isSelected && (
                                <button
                                  className="btn-primary btn-sm"
                                  onClick={(e) => { e.stopPropagation(); update("localModelName", m.id); }}
                                >
                                  Выбрать
                                </button>
                              )}
                              {isSelected && (
                                <span className="model-selected-badge">● Активна</span>
                              )}
                            </div>
                            {exists && !isDownloading && (
                              <button
                                className="btn-icon-danger"
                                disabled={downloadState !== null}
                                onClick={(e) => { e.stopPropagation(); void handleDeleteModel(m.id); }}
                                title="Удалить файл модели с диска"
                              >
                                <svg viewBox="0 0 16 16" fill="currentColor" width="14" height="14">
                                  <path d="M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5zm2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5zm3 .5a.5.5 0 0 0-1 0v6a.5.5 0 0 0 1 0V6z"/>
                                  <path fillRule="evenodd" d="M14.5 3a1 1 0 0 1-1 1H13v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4h-.5a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1H6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1h3.5a1 1 0 0 1 1 1v1zM4.118 4 4 4.059V13a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.059L11.882 4H4.118zM2.5 3V2h11v1h-11z"/>
                                </svg>
                              </button>
                            )}
                          </div>
                        </div>
                      );
                    })
                  )}
                </div>

                {downloadError && (
                  <p className="hint hint--error">{downloadError}</p>
                )}
                {modelsDir && (
                  <p className="hint">Папка моделей: <code>{modelsDir}</code></p>
                )}

              </>
            )}
          </div>
        )}


        {tab === "hotkey" && (
          <div className="section">
            <div className="field">
              <span className="field-label">Push-to-Talk (PTT) хоткей</span>
              <div className="hotkey-row">
                <div className={`hotkey-display ${hotkeyCapturing ? "capturing" : ""}`}>
                  {hotkeyCapturing
                    ? (capturedCombo || "Нажмите клавишу...")
                    : config.hotkey || "Не задан"}
                </div>
                {hotkeyCapturing ? (
                  <button className="btn-secondary" onClick={cancelCapture}>Отмена</button>
                ) : (
                  <button className="btn-secondary" onClick={startCapture}>Задать</button>
                )}
              </div>
            </div>
            <p className="hint">
              Нажмите «Задать» и удержите нужную комбинацию клавиш.
              Удерживайте хоткей во время речи.
            </p>
          </div>
        )}

        {tab === "plugins" && (
          <PluginsTab
            plugins={config.plugins}
            onChange={(plugins) => setConfig((c) => ({ ...c, plugins }))}
          />
        )}
      </div>

      <div className="settings-footer">
        <button className="btn-primary" onClick={handleSave}>
          {saved ? "✓ Сохранено" : "Сохранить"}
        </button>
      </div>
    </div>
  );
}
