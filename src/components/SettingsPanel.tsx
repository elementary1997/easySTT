import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AppConfig, DEFAULT_CONFIG, loadConfig, saveConfig } from "../lib/store";
import "./SettingsPanel.css";

type Tab = "general" | "backend" | "hotkey";

export default function SettingsPanel() {
  const [config, setConfig] = useState<AppConfig>(DEFAULT_CONFIG);
  const [tab, setTab] = useState<Tab>("general");
  const [saved, setSaved] = useState(false);
  const [modelStatus, setModelStatus] = useState<string>("");

  useEffect(() => {
    loadConfig().then(setConfig);
  }, []);

  const update = useCallback(<K extends keyof AppConfig>(key: K, val: AppConfig[K]) => {
    setConfig((c) => ({ ...c, [key]: val }));
  }, []);

  const handleSave = useCallback(async () => {
    await saveConfig(config);
    await invoke("apply_config", { config });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }, [config]);

  const handleCheckModel = useCallback(async () => {
    setModelStatus("Проверяю...");
    try {
      const path = await invoke<string>("get_model_path", { name: config.localModelName });
      const exists = await invoke<boolean>("model_exists", { path });
      setModelStatus(exists ? `✓ Найдена: ${path}` : `✗ Не найдена. Скачайте в: ${path}`);
    } catch (e) {
      setModelStatus(`Ошибка: ${e}`);
    }
  }, [config.localModelName]);

  return (
    <div className="settings">
      <div className="settings-titlebar">
        <span className="settings-app-name">easySTT</span>
        <span className="settings-title">Настройки</span>
      </div>

      <div className="tabs">
        {(["general", "backend", "hotkey"] as Tab[]).map((t) => (
          <button
            key={t}
            className={`tab ${tab === t ? "active" : ""}`}
            onClick={() => setTab(t)}
          >
            {t === "general" ? "Основные" : t === "backend" ? "Распознавание" : "Горячие клавиши"}
          </button>
        ))}
      </div>

      <div className="tab-content">
        {tab === "general" && (
          <div className="section">
            <label className="field">
              <span className="field-label">Язык распознавания</span>
              <select
                value={config.language}
                onChange={(e) => update("language", e.target.value as AppConfig["language"])}
              >
                <option value="ru">Русский</option>
                <option value="en">English</option>
                <option value="auto">Авто</option>
              </select>
            </label>

            <label className="field">
              <span className="field-label">Способ вставки текста</span>
              <select
                value={config.injectionMethod}
                onChange={(e) => update("injectionMethod", e.target.value as AppConfig["injectionMethod"])}
              >
                <option value="clipboard">Буфер обмена + Ctrl+V</option>
                <option value="typing">Прямой ввод (enigo)</option>
              </select>
            </label>

            <label className="field">
              <span className="field-label">Задержка вставки (мс)</span>
              <input
                type="number"
                min={50}
                max={1000}
                step={50}
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
          </div>
        )}

        {tab === "backend" && (
          <div className="section">
            <label className="field">
              <span className="field-label">Бэкенд распознавания</span>
              <select
                value={config.sttBackend}
                onChange={(e) => update("sttBackend", e.target.value as AppConfig["sttBackend"])}
              >
                <option value="cloudru">Cloud.ru (Whisper-large-v3)</option>
                <option value="openrouter">OpenRouter</option>
                <option value="local">Локальная модель</option>
              </select>
            </label>

            {config.sttBackend === "cloudru" && (
              <>
                <label className="field">
                  <span className="field-label">API ключ Cloud.ru</span>
                  <input
                    type="password"
                    placeholder="sk-..."
                    value={config.cloudruApiKey}
                    onChange={(e) => update("cloudruApiKey", e.target.value)}
                  />
                </label>
                <label className="field">
                  <span className="field-label">Base URL</span>
                  <input
                    type="text"
                    value={config.cloudruBaseUrl}
                    onChange={(e) => update("cloudruBaseUrl", e.target.value)}
                  />
                </label>
              </>
            )}

            {config.sttBackend === "openrouter" && (
              <>
                <label className="field">
                  <span className="field-label">API ключ OpenRouter</span>
                  <input
                    type="password"
                    placeholder="sk-or-..."
                    value={config.openrouterApiKey}
                    onChange={(e) => update("openrouterApiKey", e.target.value)}
                  />
                </label>
                <label className="field">
                  <span className="field-label">Модель</span>
                  <input
                    type="text"
                    placeholder="openai/whisper-1"
                    value={config.openrouterModel}
                    onChange={(e) => update("openrouterModel", e.target.value)}
                  />
                </label>
              </>
            )}

            {config.sttBackend === "local" && (
              <>
                <label className="field">
                  <span className="field-label">Модель Whisper</span>
                  <select
                    value={config.localModelName}
                    onChange={(e) => update("localModelName", e.target.value)}
                  >
                    <option value="tiny">tiny (~75 МБ, быстрая)</option>
                    <option value="base">base (~142 МБ, хорошая)</option>
                    <option value="small">small (~466 МБ, отличная)</option>
                    <option value="medium">medium (~1.5 ГБ, превосходная)</option>
                    <option value="large-v3">large-v3 (~3 ГБ, лучшая, нужна GPU)</option>
                  </select>
                </label>
                <div className="model-status-row">
                  <button className="btn-secondary" onClick={handleCheckModel}>
                    Проверить наличие
                  </button>
                  {modelStatus && <span className="model-status">{modelStatus}</span>}
                </div>
                <p className="hint">
                  Скачайте модель с{" "}
                  <span className="link-text">huggingface.co/ggerganov/whisper.cpp</span>{" "}
                  и поместите в папку моделей.
                </p>
              </>
            )}
          </div>
        )}

        {tab === "hotkey" && (
          <div className="section">
            <label className="field">
              <span className="field-label">Push-to-Talk (PTT) хоткей</span>
              <input
                type="text"
                placeholder="Alt+`"
                value={config.hotkey}
                onChange={(e) => update("hotkey", e.target.value)}
              />
            </label>
            <p className="hint">
              Формат: <code>Ctrl+Alt+Space</code>, <code>Alt+`</code>, <code>F9</code>.
              Удерживайте клавишу во время речи.
            </p>
          </div>
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
