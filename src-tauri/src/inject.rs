use crate::config::InjectionMethod;
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::time::Duration;

/// `restore`: при `true` сохранить текущее содержимое буфера, выполнить вставку,
/// затем восстановить исходный текст. Применяется только к Clipboard-методу
/// (включая fallback из Typing → Clipboard).
pub fn inject_text(
    text: &str,
    method: &InjectionMethod,
    delay_ms: u64,
    restore: bool,
) -> anyhow::Result<()> {
    std::thread::sleep(Duration::from_millis(delay_ms));
    match method {
        InjectionMethod::Clipboard => inject_clipboard(text, restore),
        InjectionMethod::Typing => inject_typing(text).or_else(|_| inject_clipboard(text, restore)),
    }
}

fn inject_clipboard(text: &str, restore: bool) -> anyhow::Result<()> {
    let mut cb = Clipboard::new()?;
    // Сохраняем предыдущий текст ДО перезаписи. Игнорируем ошибки —
    // буфер мог содержать не-текст (картинку), тогда восстанавливать нечего.
    let prev_text = if restore { cb.get_text().ok() } else { None };

    cb.set_text(text)?;
    std::thread::sleep(Duration::from_millis(30));

    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.key(Key::Control, Direction::Press)?;
    #[cfg(target_os = "windows")]
    enigo.key(Key::V, Direction::Click)?;
    #[cfg(not(target_os = "windows"))]
    enigo.key(Key::Unicode('v'), Direction::Click)?;
    enigo.key(Key::Control, Direction::Release)?;

    // Возвращаем оригинал. Целевому окну нужно время довыполнить paste
    // прежде чем мы перезаписываем буфер: 250 мс — компромисс между
    // надёжностью и временем до восстановления.
    if let Some(old) = prev_text {
        std::thread::sleep(Duration::from_millis(250));
        if let Ok(mut cb2) = Clipboard::new() {
            let _ = cb2.set_text(old);
        }
    }
    Ok(())
}

fn inject_typing(text: &str) -> anyhow::Result<()> {
    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.text(text)?;
    Ok(())
}
