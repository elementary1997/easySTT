# easySTT

> Лёгкая утилита речь-в-текст для Windows и Linux

Нажми кнопку или зажми хоткей → говори → текст вставляется в активное окно.

---

## Возможности

- **Push-to-Talk** — глобальный хоткей (удерживай во время речи) и кнопка в плавающем виджете
- **Системный трей** — утилита живёт в трее, не мешает работе
- **Три бэкенда распознавания**:
  - ☁️ **Cloud.ru** — Whisper-large-v3, лучшее качество русского языка
  - ☁️ **OpenRouter** — любая аудио-модель через прокси (whisper-1, gpt-4o-audio и др.)
  - 💻 **Локально** — whisper.cpp без интернета (модели tiny → large-v3)
- **Два способа вставки текста**: буфер обмена + Ctrl+V или прямой ввод символов
- **Языки**: русский, английский, авто-определение
- **Размер**: < 5 МБ (модели загружаются отдельно)

## Скриншот

```
┌─────────────────────┐
│ easySTT          ⚙  │
│  ┌───┐             │
│  │🎙 │  ✓ Вставлено │
│  │Держи│            │
│  └───┘             │
└─────────────────────┘
```

## Установка

### Windows
Скачайте `.exe` с [Releases](../../releases) и запустите установщик.

### Linux (Debian/Ubuntu)
```bash
wget https://github.com/elementary1997/easySTT/releases/latest/download/easystt.deb
sudo dpkg -i easystt.deb
```

## Быстрый старт

1. Запустите easySTT — в трее появится иконка, откроется плавающий виджет
2. Откройте настройки (⚙ или правая кнопка по трею → Настройки)
3. Выберите бэкенд и введите API ключ (или скачайте локальную модель)
4. Зажмите хоткей (по умолчанию `Alt+\``) и говорите → отпустите → текст вставится

## Настройка бэкендов

### Cloud.ru (рекомендуется для русского языка)
1. Зарегистрируйтесь на [cloud.ru](https://cloud.ru)
2. Перейдите: Пользователи → Сервисные аккаунты → Создать API-ключ
3. Выберите сервис **Foundation Models**, задайте срок действия
4. Скопируйте ключ (показывается один раз!) и вставьте в настройки

### OpenRouter
1. Получите ключ на [openrouter.ai](https://openrouter.ai)
2. Вставьте в настройки, укажите модель (например `openai/whisper-1`)

### Локальные модели (без интернета)
Скачайте файл модели с [ggerganov/whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp) и поместите в папку моделей:

| Модель | Размер | Скорость | Качество |
|--------|--------|----------|----------|
| tiny   | 75 МБ  | ⚡⚡⚡ | ★★☆ |
| base   | 142 МБ | ⚡⚡  | ★★★ |
| small  | 466 МБ | ⚡    | ★★★★ |
| medium | 1.5 ГБ | 🐢   | ★★★★★ |
| large-v3 | 3 ГБ | 🐢🐢 | ★★★★★ (GPU) |

**Путь к папке моделей:**
- Windows: `%APPDATA%\easystt\models\`
- Linux: `~/.local/share/easystt/models/`

Файлы должны называться: `ggml-base.bin`, `ggml-small.bin` и т.д.

## Сборка из исходников

**Зависимости:**
- [Rust](https://rustup.rs/) 1.88+
- [Node.js](https://nodejs.org/) 18+
- Linux: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev libasound2-dev pkg-config`

```bash
git clone https://github.com/elementary1997/easySTT
cd easySTT
npm install
cargo install tauri-cli --version "^2.0"
cargo tauri build
```

Результат: `src-tauri/target/release/bundle/deb/*.deb` (Linux) или `bundle/nsis/*.exe` (Windows)

## Разработка

```bash
cargo tauri dev   # горячая перезагрузка
```

## Технологии

| Слой | Технология |
|------|-----------|
| UI | React 18 + TypeScript + Vite |
| Десктоп | Tauri 2 (Rust) |
| Аудио | cpal |
| Вставка текста | enigo + arboard |
| Конфиг | tauri-plugin-store |
| Хоткей | tauri-plugin-global-shortcut |

---

---

# easySTT (English)

> Lightweight speech-to-text utility for Windows and Linux

Press a button or hold a hotkey → speak → text is inserted into the active window.

---

## Features

- **Push-to-Talk** — global hotkey (hold while speaking) and floating widget button
- **System tray** — lives in the tray, stays out of your way
- **Three recognition backends**:
  - ☁️ **Cloud.ru** — Whisper-large-v3, best quality for Russian
  - ☁️ **OpenRouter** — any audio model via proxy (whisper-1, gpt-4o-audio, etc.)
  - 💻 **Local** — whisper.cpp, fully offline (models tiny → large-v3)
- **Two text injection modes**: clipboard + Ctrl+V, or direct key input
- **Languages**: Russian, English, auto-detect
- **Binary size**: < 5 MB (models downloaded separately)

## Installation

### Windows
Download the `.exe` installer from [Releases](../../releases).

### Linux (Debian/Ubuntu)
```bash
wget https://github.com/elementary1997/easySTT/releases/latest/download/easystt.deb
sudo dpkg -i easystt.deb
```

## Quick Start

1. Launch easySTT — tray icon appears, floating widget opens
2. Open Settings (⚙ button or right-click tray → Settings)
3. Choose a backend and enter your API key (or download a local model)
4. Hold the hotkey (default `Alt+\``) and speak → release → text is inserted

## Backend Setup

### Cloud.ru (recommended for Russian)
1. Register at [cloud.ru](https://cloud.ru)
2. Go to: Users → Service Accounts → Create API Key
3. Select service **Foundation Models**, set an expiry period
4. Copy the key (shown only once!) and paste it into Settings

### OpenRouter
1. Get a key at [openrouter.ai](https://openrouter.ai)
2. Paste it in Settings and set a model (e.g. `openai/whisper-1`)

### Local Models (offline)
Download a model file from [ggerganov/whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp) and place it in the models folder:

| Model | Size | Speed | Quality |
|-------|------|-------|---------|
| tiny   | 75 MB  | ⚡⚡⚡ | ★★☆ |
| base   | 142 MB | ⚡⚡  | ★★★ |
| small  | 466 MB | ⚡    | ★★★★ |
| medium | 1.5 GB | 🐢   | ★★★★★ |
| large-v3 | 3 GB | 🐢🐢 | ★★★★★ (GPU) |

**Models folder path:**
- Windows: `%APPDATA%\easystt\models\`
- Linux: `~/.local/share/easystt/models/`

Files must be named: `ggml-base.bin`, `ggml-small.bin`, etc.

## Build from Source

**Requirements:**
- [Rust](https://rustup.rs/) 1.88+
- [Node.js](https://nodejs.org/) 18+
- Linux: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev libasound2-dev pkg-config`

```bash
git clone https://github.com/elementary1997/easySTT
cd easySTT
npm install
cargo install tauri-cli --version "^2.0"
cargo tauri build
```

Output: `src-tauri/target/release/bundle/deb/*.deb` (Linux) or `bundle/nsis/*.exe` (Windows)

## Development

```bash
cargo tauri dev   # hot reload
```

## Tech Stack

| Layer | Technology |
|-------|-----------|
| UI | React 18 + TypeScript + Vite |
| Desktop shell | Tauri 2 (Rust) |
| Audio capture | cpal |
| Text injection | enigo + arboard |
| Config storage | tauri-plugin-store |
| Global hotkey | tauri-plugin-global-shortcut |

## License

MIT
