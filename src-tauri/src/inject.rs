use crate::config::InjectionMethod;
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::time::Duration;

pub fn inject_text(text: &str, method: &InjectionMethod, delay_ms: u64) -> anyhow::Result<()> {
    std::thread::sleep(Duration::from_millis(delay_ms));
    match method {
        InjectionMethod::Clipboard => inject_clipboard(text),
        InjectionMethod::Typing => inject_typing(text).or_else(|_| inject_clipboard(text)),
    }
}

fn inject_clipboard(text: &str) -> anyhow::Result<()> {
    let mut cb = Clipboard::new()?;
    cb.set_text(text)?;
    std::thread::sleep(Duration::from_millis(30));

    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.key(Key::Control, Direction::Press)?;
    #[cfg(target_os = "windows")]
    enigo.key(Key::V, Direction::Click)?;
    #[cfg(not(target_os = "windows"))]
    enigo.key(Key::Unicode('v'), Direction::Click)?;
    enigo.key(Key::Control, Direction::Release)?;
    Ok(())
}

fn inject_typing(text: &str) -> anyhow::Result<()> {
    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.text(text)?;
    Ok(())
}
