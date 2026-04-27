# easySTT

[English](README.md) · [Релизы](https://github.com/elementary1997/easySTT/releases)

Лёгкая утилита **речь → текст** для **Windows** и **Linux** (Tauri 2 + React). Удерживайте хоткей, говорите, текст вставляется в **активное окно** (буфер + Ctrl+V или ввод с клавиатуры).

## Возможности

- **Бэкенды:** локальный [whisper.cpp](https://github.com/ggerganov/whisper.cpp) (без сети), [Cloud.ru](https://cloud.ru) Foundation Models, [OpenRouter](https://openrouter.ai)
- **Глобальный хоткей** и **плавающий** always-on-top виджет
- **Трей** — показ/скрытие, настройки, выход
- **Тема виджета** (градиенты, волна, скругления; на Windows окно может обрезаться по скруглённой области, без «клиньев» DWM)

## Установка

- **Windows:** скачайте `.exe` / `.msi` из [Releases](https://github.com/elementary1997/easySTT/releases)
- **Linux (Debian/Ubuntu):**
  ```bash
  wget https://github.com/elementary1997/easySTT/releases/latest/download/easystt.deb
  sudo dpkg -i easystt.deb
  ```

## Быстрый старт

1. Запустите easySTT — в трее иконка, откроется виджет
2. **Настройки** (⚙ на виджете или трей)
3. Выберите бэкенд (ключи для облака или **локальная** модель)
4. Хоткей по умолчанию: `` Alt+` `` — удерживайте во время речи, отпустите — распознавание и вставка

## Локальное STT: почему «очень медленно»

| Ситуация | Что происходит |
|----------|----------------|
| Обычная/релизная **сборка** | В бинаре по умолчанию **только CPU**, без ускорителя |
| **Модель** | Для скорости по умолчанию в профиле — **tiny**; **small+** на CPU — часто **минуты** на фразу |
| **Ускорение** | Соберите с **`whisper-vulkan`** (много GPU на Linux) или **`whisper-cuda`** (NVIDIA) и в настройках включите **«Использовать GPU»** |
| **Релизы** | В выдачах GitHub кроме «обычного» `.deb` может быть пакет с **Vulkan** в имени (см. release notes) — он быстрее на GPU |

Переменные окружения (см. `src-tauri/src/stt/local.rs`):

- `EASYSTT_WHISPER_NO_GPU=1` — в GPU-сборке принудительно считать на **CPU**
- `EASYSTT_WHISPER_FULL_AUDIO_CTX=1` — отключить укороченный контекст для коротких фраз (чуть ближе к настройкам «по умолчанию» whisper, может быть медленнее)

**Папка моделей:**

- Windows: `%APPDATA%\easystt\models\`
- Linux: `~/.local/share/easystt/models/`

Имена файлов: `ggml-tiny.bin`, `ggml-base.bin` и т.д. (как в списке в настройках). Квантованные веса можно положить, если переименовать в тот же шаблон `ggml-<имя>.bin`.

## Сборка из исходников

**Нужны:** [Rust](https://rustup.rs/) 1.88+, [Node](https://nodejs.org/) 18+

**Пакеты Linux (пример Debian/Ubuntu):**
```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  libasound2-dev libxdo-dev pkg-config
```

```bash
git clone https://github.com/elementary1997/easySTT.git
cd easySTT
npm install
cargo install tauri-cli@^2 --locked
npm run tauri build
```

**GPU (ровно одна фича при сборке):**
```bash
npm run tauri:build:gpu     # CUDA / NVIDIA
npm run tauri:build:vulkan  # Vulkan (часто Linux)
```

## Разработка

```bash
npm run tauri dev
npm run tauri:dev:gpu   # с CUDA, если поставлен CUDA Toolkit
```

## Стек

| Слой | Технология |
|------|------------|
| UI | React 18, TypeScript, Vite |
| Оболочка | Tauri 2 |
| Аудио | cpal |
| Локальное STT | whisper-rs (whisper.cpp) |
| Вставка | enigo, arboard |
| Настройки | tauri-plugin-store |

## Лицензия

MIT
