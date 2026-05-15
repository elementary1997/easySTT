import { Store } from "@tauri-apps/plugin-store";
import { invoke } from "@tauri-apps/api/core";

export interface AppConfig {
  sttBackend: "local" | "cloudru" | "openrouter";
  language: "ru" | "en" | "auto";
  injectionMethod: "clipboard" | "typing";
  hotkey: string;
  localModelName: string;
  localUseGpu: boolean;
  cloudruApiKey: string;
  /** Сохраняется из старых настроек; в UI не показываем (доступ IAM по паре id+секрет). */
  cloudruKeyId: string;
  cloudruBaseUrl: string;
  /** Id модели в Cloud.ru (после «Проверить соединение» — из списка /models) */
  cloudruModel: string;
  openrouterApiKey: string;
  openrouterModel: string;
  injectDelayMs: number;
  restoreClipboard: boolean;
  micDeviceName: string;
  /** Плавающий виджет: фон (градиент) + три оттенка волны — hex #RRGGBB */
  widgetBgFrom: string;
  widgetBgTo: string;
  widgetWave1: string;
  widgetWave2: string;
  widgetWave3: string;
  /** Внешний вид: анимация градиента/маски (верхняя волна) */
  widgetAnimation: "flow" | "breathe" | "static" | "aurora";
  /** Форма силуэта волны-разделителя */
  widgetWaveForm: "rolling" | "smooth" | "line";
  /** Скругление угла виджета */
  widgetCornerStyle: "none" | "round";
  /** Перевод в реальном времени */
  translateEnabled: boolean;
  /** Направление перевода: "ru_to_en" | "en_to_ru" */
  translateDirection: "ru_to_en" | "en_to_ru";
  /** Модель для перевода на Cloud.ru */
  translateModelCloudru: string;
  /** Модель для перевода на OpenRouter */
  translateModelOpenrouter: string;
  /** Список подключённых плагинов */
  plugins: PluginEntry[];
}

export interface PluginEntry {
  id: string;
  path: string;
  enabled: boolean;
  name: string;
  version: string;
  port: number;
}

export const DEFAULT_CONFIG: AppConfig = {
  sttBackend: "cloudru",
  language: "auto",
  injectionMethod: "clipboard",
  hotkey: "Alt+`",
  localModelName: "tiny",
  localUseGpu: true,
  cloudruApiKey: "",
  cloudruKeyId: "",
  cloudruBaseUrl: "https://foundation-models.api.cloud.ru/v1",
  cloudruModel: "openai/whisper-large-v3",
  openrouterApiKey: "",
  openrouterModel: "openai/whisper-1",
  injectDelayMs: 150,
  restoreClipboard: false,
  micDeviceName: "",
  widgetBgFrom: "#12121f",
  widgetBgTo: "#1a1a2e",
  widgetWave1: "#4c84ff",
  widgetWave2: "#c762ff",
  widgetWave3: "#ff65a7",
  widgetAnimation: "flow",
  widgetWaveForm: "rolling",
  widgetCornerStyle: "none",
  translateEnabled: false,
  translateDirection: "ru_to_en",
  translateModelCloudru: "Qwen/Qwen2.5-72B-Instruct",
  translateModelOpenrouter: "openai/gpt-4o-mini",
  plugins: [],
};

let _store: Store | null = null;

async function getStore(): Promise<Store> {
  if (!_store) {
    _store = await Store.load("settings.json");
  }
  return _store;
}

export async function loadConfig(): Promise<AppConfig> {
  const store = await getStore();
  const config: Partial<AppConfig> = {};

  for (const key of Object.keys(DEFAULT_CONFIG) as (keyof AppConfig)[]) {
    const val = await store.get<AppConfig[typeof key]>(key);
    if (val !== null && val !== undefined) {
      (config as Record<string, unknown>)[key] = val;
    }
  }

  return { ...DEFAULT_CONFIG, ...config };
}

export async function saveConfig(config: AppConfig): Promise<void> {
  const store = await getStore();
  for (const [key, val] of Object.entries(config)) {
    await store.set(key, val);
  }
  await invoke("apply_config", { config });
}
