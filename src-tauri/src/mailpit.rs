use crate::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailpitNotification {
    pub id: String,
    pub from_name: String,
    pub from_address: String,
    pub subject: String,
    pub snippet: String,
    pub url: String,
    pub unread: usize,
    pub total: usize,
    pub theme: String,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailpitUnreadPayload {
    pub unread: usize,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMailpitMessage {
    pub id: String,
    pub from_name: String,
    pub from_address: String,
    pub subject: String,
    pub snippet: String,
    pub url: String,
}

pub fn parse_messages_response(
    json: &serde_json::Value,
    http_port: u16,
) -> (usize, usize, Vec<ParsedMailpitMessage>) {
    let total = json.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let unread = json.get("unread").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let mut out = Vec::new();
    if let Some(messages) = json.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            let Some(id) = msg.get("ID").and_then(|v| v.as_str()) else {
                continue;
            };
            if id.is_empty() {
                continue;
            }
            let from_obj = msg.get("From");
            let from_name = from_obj
                .and_then(|f| f.get("Name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let from_address = from_obj
                .and_then(|f| f.get("Address"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let subject = msg
                .get("Subject")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let snippet = msg
                .get("Snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            out.push(ParsedMailpitMessage {
                id: id.to_string(),
                from_name,
                from_address,
                subject,
                snippet,
                url: format!("http://127.0.0.1:{http_port}/view/{id}"),
            });
        }
    }

    (unread, total, out)
}

fn position_bottom_right(win: &tauri::WebviewWindow, logical_w: f64, logical_h: f64) {
    if let Ok(Some(monitor)) = win.primary_monitor() {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let margin = (14.0 * scale) as i32;
        let phys_w = (logical_w * scale) as i32;
        let phys_h = (logical_h * scale) as i32;
        let x = area.position.x + (area.size.width as i32) - phys_w - margin;
        let y = area.position.y + (area.size.height as i32) - phys_h - margin;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
}

pub fn show_mailpit_popup(app: &AppHandle, notif: MailpitNotification) {
    {
        let state = app.state::<AppState>();
        *state.latest_mailpit_notification.lock().unwrap() = Some(notif.clone());
    }

    let logical_w = 364.0;
    let logical_h = 110.0;

    if let Some(win) = app.get_webview_window("mailpit-toast") {
        position_bottom_right(&win, logical_w, logical_h);
        let _ = win.emit("mailpit-toast-update", &notif);
        let _ = win.show();
        return;
    }

    if let Ok(win) = WebviewWindowBuilder::new(
        app,
        "mailpit-toast",
        WebviewUrl::App("index.html?mode=mail-toast".into()),
    )
    .title("Mailpit")
    .inner_size(logical_w, logical_h)
    .decorations(false)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false)
    .build()
    {
        position_bottom_right(&win, logical_w, logical_h);
        let _ = win.show();
        let _ = win.emit("mailpit-toast-update", &notif);
    }
}

pub fn start_mailpit_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let mut initialized = false;
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut last_unread: Option<(usize, usize)> = None;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let (is_running, http_port, notify_enabled, theme, language) = {
                let state = app.state::<AppState>();
                let cfg = state.config.lock().unwrap().clone();
                let inv = state.inventory.lock().unwrap().clone();
                let statuses = state.services.get_statuses(&cfg, &inv);
                let running = statuses
                    .iter()
                    .any(|s| s.id == "mailpit" && s.state == "running");
                (
                    running,
                    cfg.ports.mailpit_http,
                    cfg.mailpit_notifications,
                    cfg.theme,
                    cfg.language,
                )
            };

            if !is_running {
                if initialized || last_unread.is_some() {
                    initialized = false;
                    seen_ids.clear();
                    last_unread = None;
                    let _ = app.emit_to(
                        "main",
                        "mailpit-unread",
                        MailpitUnreadPayload { unread: 0, total: 0 },
                    );
                }
                continue;
            }

            let url = format!("http://127.0.0.1:{http_port}/api/v1/messages?limit=5");
            let Ok(resp) = client.get(&url).send().await else {
                continue;
            };
            if !resp.status().is_success() {
                continue;
            }
            let Ok(json) = resp.json::<serde_json::Value>().await else {
                continue;
            };

            let (unread, total, messages) = parse_messages_response(&json, http_port);

            if last_unread != Some((unread, total)) {
                last_unread = Some((unread, total));
                let _ = app.emit_to(
                    "main",
                    "mailpit-unread",
                    MailpitUnreadPayload { unread, total },
                );
            }

            if !initialized {
                for m in &messages {
                    seen_ids.insert(m.id.clone());
                }
                initialized = true;
                continue;
            }

            let mut newest_unseen: Option<ParsedMailpitMessage> = None;
            for m in messages {
                if seen_ids.insert(m.id.clone()) && newest_unseen.is_none() {
                    newest_unseen = Some(m);
                }
            }

            if let Some(msg) = newest_unseen {
                if notify_enabled {
                    let notif = MailpitNotification {
                        id: msg.id,
                        from_name: msg.from_name,
                        from_address: msg.from_address,
                        subject: msg.subject,
                        snippet: msg.snippet,
                        url: msg.url,
                        unread,
                        total,
                        theme,
                        language,
                    };
                    show_mailpit_popup(&app, notif);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mailpit_messages_payload() {
        let payload = serde_json::json!({
            "total": 4,
            "unread": 2,
            "messages": [
                {
                    "ID": "abc123",
                    "From": { "Name": "Laravel App", "Address": "noreply@demo.test" },
                    "Subject": "Verify Email Address",
                    "Snippet": "Click the button below to verify your email."
                }
            ]
        });

        let (unread, total, msgs) = parse_messages_response(&payload, 8025);
        assert_eq!(unread, 2);
        assert_eq!(total, 4);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].id, "abc123");
        assert_eq!(msgs[0].from_name, "Laravel App");
        assert_eq!(msgs[0].from_address, "noreply@demo.test");
        assert_eq!(msgs[0].subject, "Verify Email Address");
        assert_eq!(msgs[0].url, "http://127.0.0.1:8025/view/abc123");
    }
}
