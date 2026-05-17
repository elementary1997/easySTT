//! Управление жизненным циклом плагинов easySTT.
//!
//! Каждый плагин — отдельный исполняемый файл с HTTP-сервером.
//! PluginManager запускает/останавливает процессы и вызывает
//! POST /intercept у каждого активного плагина перед инжектом текста.

use crate::config::PluginEntry;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

pub struct PluginManager {
    /// Запущенные дочерние процессы плагинов: id → Child.
    children: Mutex<HashMap<String, std::process::Child>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            children: Mutex::new(HashMap::new()),
        }
    }

    /// Запускает плагин с флагом --background (окно скрыто, только трей).
    pub fn spawn(&self, entry: &PluginEntry) -> Result<(), String> {
        let mut map = self.children.lock().unwrap();
        // Если процесс уже жив — не запускаем снова.
        if let Some(child) = map.get_mut(&entry.id) {
            if matches!(child.try_wait(), Ok(None)) {
                return Ok(());
            }
        }
        let child = std::process::Command::new(&entry.path)
            .arg("--background")
            .arg("--port")
            .arg(entry.port.to_string())
            .spawn()
            .map_err(|e| format!("Не удалось запустить «{}»: {e}", entry.name))?;
        map.insert(entry.id.clone(), child);
        Ok(())
    }

    /// Убивает процесс плагина.
    pub fn kill(&self, id: &str) {
        if let Some(mut child) = self.children.lock().unwrap().remove(id) {
            let _ = child.kill();
            let _ = child.wait(); // не оставляем зомби
        }
    }

    /// Возвращает true если процесс плагина запущен и ещё не завершился.
    pub fn is_running(&self, id: &str) -> bool {
        let mut map = self.children.lock().unwrap();
        match map.get_mut(id) {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Убивает все плагины (вызывается при завершении easySTT).
    pub fn kill_all(&self) {
        let mut map = self.children.lock().unwrap();
        for (_, mut child) in map.drain() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        self.kill_all();
    }
}

// ─── HTTP helpers ────────────────────────────────────────────────────────────

/// Запрашивает /plugin-manifest у запущенного плагина.
/// Таймаут 5 с — для ожидания старта процесса.
pub async fn fetch_manifest(port: u16) -> anyhow::Result<serde_json::Value> {
    let url = format!("http://127.0.0.1:{port}/plugin-manifest");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let resp = client.get(&url).send().await?;
    Ok(resp.json().await?)
}

/// Открывает окно настроек плагина через POST /open-settings.
/// Делает до 10 попыток с паузой 1 с — сервер плагина может ещё стартовать.
pub async fn open_settings(port: u16) -> anyhow::Result<()> {
    let url = format!("http://127.0.0.1:{port}/open-settings");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;
    let mut last_err: anyhow::Error = anyhow::anyhow!("плагин не отвечает на порту {port}");
    for attempt in 0..=10 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        match client.post(&url).send().await {
            Ok(_) => return Ok(()),
            Err(e) => last_err = e.into(),
        }
    }
    Err(last_err)
}

/// Сбрасывает конфиг плагина и завершает его процесс через POST /reset.
pub async fn reset_plugin(port: u16) -> anyhow::Result<()> {
    let url = format!("http://127.0.0.1:{port}/reset");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;
    client.post(&url).send().await?;
    Ok(())
}

pub struct InterceptResult {
    /// At least one plugin consumed the command.
    pub intercepted: bool,
    /// Agent name was detected in the text (even if no command matched).
    pub agent_detected: bool,
}

/// Пробует перехватить текст у всех включённых плагинов.
/// Таймаут на один плагин — 500 мс, не блокирует инжект при недоступности.
pub async fn try_intercept(text: &str, plugins: &[PluginEntry]) -> InterceptResult {
    let mut intercepted = false;
    let mut agent_detected = false;
    for plugin in plugins.iter().filter(|p| p.enabled && p.port > 0) {
        let url = format!("http://127.0.0.1:{}/intercept", plugin.port);
        let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
        else {
            continue;
        };
        let Ok(resp) = client
            .post(&url)
            .json(&serde_json::json!({"text": text}))
            .send()
            .await
        else {
            continue;
        };
        if let Ok(body) = resp.json::<serde_json::Value>().await {
            if body.get("intercept").and_then(|v| v.as_bool()).unwrap_or(false) {
                intercepted = true;
            }
            if body.get("agentDetected").and_then(|v| v.as_bool()).unwrap_or(false) {
                agent_detected = true;
            }
        }
    }
    InterceptResult { intercepted, agent_detected }
}
