import { Store } from "@tauri-apps/plugin-store";

export interface AppConfig {
  sttBackend: "local" | "cloudru" | "openrouter";
  language: "ru" | "en" | "auto";
  injectionMethod: "clipboard" | "typing";
  hotkey: string;
  localModelName: string;
  cloudruApiKey: string;
  cloudruBaseUrl: string;
  openrouterApiKey: string;
  openrouterModel: string;
  injectDelayMs: number;
  restoreClipboard: boolean;
}

export const DEFAULT_CONFIG: AppConfig = {
  sttBackend: "cloudru",
  language: "ru",
  injectionMethod: "clipboard",
  hotkey: "Alt+`",
  localModelName: "base",
  cloudruApiKey: "",
  cloudruBaseUrl: "https://foundation-models.api.cloud.ru/v1",
  openrouterApiKey: "",
  openrouterModel: "openai/whisper-1",
  injectDelayMs: 150,
  restoreClipboard: false,
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
}
