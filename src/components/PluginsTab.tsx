import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { PluginEntry } from "../lib/store";

interface PluginsTabProps {
  plugins: PluginEntry[];
  onChange: (plugins: PluginEntry[]) => void;
}

const DEFAULT_PLUGIN_PORT = 8790;

export default function PluginsTab({ plugins, onChange }: PluginsTabProps) {
  const [status, setStatus] = useState<Record<string, boolean>>({});
  const [adding, setAdding] = useState(false);
  const [addError, setAddError] = useState("");

  const refreshStatus = useCallback(() => {
    invoke<Record<string, boolean>>("get_plugins_status")
      .then(setStatus)
      .catch(() => {});
  }, []);

  useEffect(() => {
    refreshStatus();
    const id = setInterval(refreshStatus, 3000);
    return () => clearInterval(id);
  }, [refreshStatus]);

  const handleAdd = useCallback(async () => {
    setAddError("");
    try {
      const selected = await openDialog({
        multiple: false,
        title: "Выберите исполняемый файл плагина",
        filters: [
          { name: "Плагин (.exe, .zip, AppImage)", extensions: ["exe", "zip", "AppImage", ""] },
        ],
      });
      if (!selected) return;
      const path = typeof selected === "string" ? selected : selected[0];
      if (!path) return;

      setAdding(true);
      const entry = await invoke<PluginEntry>("add_plugin", { path, port: DEFAULT_PLUGIN_PORT });
      onChange([...plugins, entry]);
      refreshStatus();
      // Auto-open plugin settings so user can configure agent name and commands
      await invoke("open_plugin_settings", { id: entry.id }).catch(() => {});
    } catch (e) {
      setAddError(String(e));
    } finally {
      setAdding(false);
    }
  }, [plugins, onChange, refreshStatus]);

  const handleRemove = useCallback(async (id: string) => {
    if (!confirm("Удалить плагин? Процесс будет остановлен.")) return;
    try {
      await invoke("remove_plugin", { id });
      onChange(plugins.filter((p) => p.id !== id));
      setStatus((s) => { const n = { ...s }; delete n[id]; return n; });
    } catch (e) {
      alert(String(e));
    }
  }, [plugins, onChange]);

  const handleToggle = useCallback(async (id: string, enabled: boolean) => {
    try {
      await invoke("toggle_plugin", { id, enabled });
      onChange(plugins.map((p) => p.id === id ? { ...p, enabled } : p));
      refreshStatus();
    } catch (e) {
      alert(String(e));
    }
  }, [plugins, onChange, refreshStatus]);

  const handleOpenSettings = useCallback(async (id: string) => {
    try {
      await invoke("open_plugin_settings", { id });
    } catch (e) {
      alert(`Не удалось открыть настройки: ${e}`);
    }
  }, []);

  return (
    <div className="section plugins-section">
      {plugins.length === 0 ? (
        <div className="plugins-empty">
          <span className="plugins-empty-icon">🧩</span>
          <p>Нет подключённых плагинов</p>
          <p className="plugins-empty-hint">
            Нажмите «Добавить плагин» — выберите исполняемый файл
          </p>
        </div>
      ) : (
        <div className="plugins-list">
          {plugins.map((p) => (
            <div key={p.id} className={`plugin-card ${!p.enabled ? "plugin-card--disabled" : ""}`}>
              <div className="plugin-card-left">
                <span
                  className={`plugin-status-dot ${status[p.id] ? "dot-running" : "dot-stopped"}`}
                  title={status[p.id] ? "Запущен" : "Остановлен"}
                />
                <div className="plugin-info">
                  <span className="plugin-name">{p.name || p.path.split(/[\\/]/).pop()}</span>
                  <span className="plugin-meta">v{p.version}</span>
                  <span className="plugin-path" title={p.path}>{p.path}</span>
                </div>
              </div>
              <div className="plugin-card-actions">
                <label className="plugin-toggle" title="Вкл / Выкл">
                  <input
                    type="checkbox"
                    checked={p.enabled}
                    onChange={(e) => handleToggle(p.id, e.target.checked)}
                  />
                  <span>{p.enabled ? "Вкл" : "Выкл"}</span>
                </label>
                <button
                  className="btn-secondary btn-sm"
                  onClick={() => handleOpenSettings(p.id)}
                  title="Открыть настройки плагина"
                >
                  ⚙ Настройки
                </button>
                <button
                  className="btn-danger btn-sm"
                  onClick={() => handleRemove(p.id)}
                  title="Удалить плагин"
                >
                  Удалить
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="plugins-add-row">
        <button
          className="btn-primary btn-sm"
          onClick={handleAdd}
          disabled={adding}
        >
          {adding ? "Подключаю..." : "+ Добавить плагин"}
        </button>
      </div>

      {addError && <div className="plugins-error">{addError}</div>}

    </div>
  );
}
