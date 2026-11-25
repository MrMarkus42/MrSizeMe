//! Asking an AI about a folder.
//!
//! What is sent is always shown to the user first and only goes out after
//! a click. It contains the folder name (with at most two folders above it),
//! its size and the kind of system – never the full path, the user name or
//! file contents. The answer may be wrong; the program never deletes
//! anything because of it.

use std::io;
use std::time::Duration;

use serde_json::{Value, json};

use crate::format;
use crate::i18n::{Lang, lang};
use crate::settings::Settings;
use crate::tree::{NO_PARENT, NodeId, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// Google search in AI mode, opened in the browser.
    GoogleAi,
    /// Plain Google search, opened in the browser.
    GoogleSearch,
    /// A local model through Ollama (nothing leaves the computer).
    Ollama,
    /// Google Gemini with the user's own key.
    Gemini,
    /// Anthropic Claude with the user's own key.
    Claude,
    /// OpenRouter: many models through one key of the user.
    OpenRouter,
}

impl Provider {
    /// All providers in the order of the buttons (and of `Texts::providers`).
    pub const ALL: [Provider; 6] = [
        Provider::GoogleAi,
        Provider::GoogleSearch,
        Provider::Ollama,
        Provider::Gemini,
        Provider::Claude,
        Provider::OpenRouter,
    ];

    /// Short code for the settings file.
    pub fn code(self) -> &'static str {
        match self {
            Provider::GoogleAi => "google-ai",
            Provider::GoogleSearch => "google",
            Provider::Ollama => "ollama",
            Provider::Gemini => "gemini",
            Provider::Claude => "claude",
            Provider::OpenRouter => "openrouter",
        }
    }

    pub fn from_code(code: &str) -> Option<Provider> {
        Provider::ALL.into_iter().find(|p| p.code() == code)
    }

    /// Position in [`Provider::ALL`] (for the button texts).
    pub fn index(self) -> usize {
        Provider::ALL.iter().position(|&p| p == self).unwrap_or(0)
    }

    /// Where the user's key for this service is stored, if it needs one.
    pub fn key_service(self) -> Option<&'static str> {
        match self {
            Provider::Gemini => Some(crate::secrets::GEMINI),
            Provider::Claude => Some(crate::secrets::CLAUDE),
            Provider::OpenRouter => Some(crate::secrets::OPENROUTER),
            _ => None,
        }
    }

    /// Opens the browser instead of answering inside the program.
    pub fn uses_browser(self) -> bool {
        matches!(self, Provider::GoogleAi | Provider::GoogleSearch)
    }

    /// Leaves the computer (the local model does not).
    pub fn is_online(self) -> bool {
        self != Provider::Ollama
    }
}

fn os_name() -> &'static str {
    if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    }
}

/// Name parts that would reveal who the user is.
fn private_names() -> Vec<String> {
    let mut v = Vec::new();
    for var in ["USERNAME", "USER", "LOGNAME"] {
        if let Some(u) = std::env::var_os(var) {
            v.push(u.to_string_lossy().to_lowercase());
        }
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    if let Some(name) = home.as_ref().and_then(|h| std::path::Path::new(h).file_name()) {
        v.push(name.to_string_lossy().to_lowercase());
    }
    v.retain(|s| !s.is_empty());
    v
}

/// The short, anonymous description of a node: up to three name parts.
pub fn anonymous_name(tree: &Tree, id: NodeId) -> String {
    anonymous_name_with(tree, id, &private_names())
}

fn anonymous_name_with(tree: &Tree, id: NodeId, private: &[String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = id;
    while cur != NO_PARENT && parts.len() < 3 {
        let n = tree.node(cur);
        if n.parent == NO_PARENT {
            // The root is a full path: take its last parts only.
            let comps: Vec<String> = tree
                .root_path
                .components()
                .filter_map(|c| match c {
                    std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect();
            for c in comps.iter().rev() {
                if parts.len() >= 3 {
                    break;
                }
                parts.push(c.clone());
            }
            break;
        }
        parts.push(n.name_lossy().into_owned());
        cur = n.parent;
    }
    let user = match lang() {
        Lang::De => "<Benutzer>",
        Lang::En => "<user>",
    };
    parts
        .into_iter()
        .rev()
        .map(|p| if private.contains(&p.to_lowercase()) { user.to_string() } else { p })
        .collect::<Vec<_>>()
        .join(if cfg!(windows) { "\\" } else { "/" })
}

/// The question that would be sent, in the current language.
pub fn question(tree: &Tree, id: NodeId) -> String {
    let n = tree.node(id);
    let name = anonymous_name(tree, id);
    let size = format::size(n.size);
    match lang() {
        Lang::De => {
            let what = if n.is_dir() { "der Ordner" } else { "die Datei" };
            format!(
                "Auf meinem Computer ({}) belegt {what} „{name}“ {size}. Was ist das, darf ich es löschen, und wie räume ich es sicher auf?",
                os_name()
            )
        }
        Lang::En => {
            let what = if n.is_dir() { "folder" } else { "file" };
            format!(
                "On my computer ({}) the {what} “{name}” uses {size}. What is it, may I delete it, and how do I clean it up safely?",
                os_name()
            )
        }
    }
}

/// Instructions for the AI (not shown in the question).
fn system_prompt() -> &'static str {
    match lang() {
        Lang::De => {
            "Du hilfst beim Aufräumen einer Festplatte. Antworte auf Deutsch, kurz und praktisch (höchstens 8 Sätze). Sag klar, ob Löschen sicher ist. Empfiehl die eingebauten Aufräumwerkzeuge des Systems statt Löschen von Hand, wo es sie gibt. Warne deutlich, wenn Löschen Programme oder das System beschädigen kann."
        }
        Lang::En => {
            "You help clean up a disk. Answer briefly and practically (at most 8 sentences). Say clearly whether deleting is safe. Recommend the system's built-in clean-up tools over deleting by hand where they exist. Warn clearly if deleting can damage programs or the system."
        }
    }
}

/// Percent-encoding for a URL query value.
pub fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The Google address for a question (AI mode or plain search).
pub fn google_url(question: &str, ai_mode: bool) -> String {
    let q = url_encode(question);
    if ai_mode {
        format!("https://www.google.com/search?udm=50&q={q}")
    } else {
        format!("https://www.google.com/search?q={q}")
    }
}

pub(crate) fn agent() -> ureq::Agent {
    #[cfg(any(windows, target_os = "macos"))]
    let provider = ureq::tls::TlsProvider::NativeTls;
    #[cfg(not(any(windows, target_os = "macos")))]
    let provider = ureq::tls::TlsProvider::Rustls;
    let tls = ureq::tls::TlsConfig::builder().provider(provider).build();
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .http_status_as_error(false)
        .tls_config(tls)
        .build()
        .into()
}

fn check_allowed(settings: &Settings, provider: Provider) -> io::Result<()> {
    if crate::external::is_blocked() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "blocked in test mode"));
    }
    if settings.offline && provider.is_online() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "offline mode"));
    }
    Ok(())
}

fn post(url: &str, headers: &[(&str, &str)], body: &Value) -> io::Result<Value> {
    let mut req = agent().post(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let mut resp = req.send_json(body).map_err(|e| io::Error::other(e.to_string()))?;
    let status = resp.status().as_u16();
    let value: Value = resp.body_mut().read_json().map_err(|e| io::Error::other(e.to_string()))?;
    if status >= 400 {
        let msg = value["error"]["message"]
            .as_str()
            .or(value["error"].as_str())
            .unwrap_or("")
            .chars()
            .take(300)
            .collect::<String>();
        return Err(io::Error::other(format!("HTTP {status}: {msg}")));
    }
    Ok(value)
}

/// Asks the chosen AI and returns its answer. Blocks; call it from a
/// background thread. Browser providers are not handled here.
pub fn ask(provider: Provider, question: &str, settings: &Settings, key: Option<&str>) -> io::Result<String> {
    check_allowed(settings, provider)?;
    let need_key =
        || key.filter(|k| !k.is_empty()).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no key"));
    match provider {
        Provider::GoogleAi | Provider::GoogleSearch => Err(io::Error::other("opens in the browser")),
        Provider::Ollama => {
            if settings.ollama_model.trim().is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "no model chosen"));
            }
            let url = format!("{}/api/generate", settings.ollama_url.trim_end_matches('/'));
            let body = json!({
                "model": settings.ollama_model,
                "system": system_prompt(),
                "prompt": question,
                "stream": false,
            });
            let v = post(&url, &[], &body)?;
            text_or_error(v["response"].as_str().map(str::to_string))
        }
        Provider::Gemini => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
                url_encode(&settings.gemini_model)
            );
            let body = json!({
                "systemInstruction": {"parts": [{"text": system_prompt()}]},
                "contents": [{"parts": [{"text": question}]}],
            });
            let v = post(&url, &[("x-goog-api-key", need_key()?)], &body)?;
            let parts = v["candidates"][0]["content"]["parts"].as_array().cloned().unwrap_or_default();
            let text: Vec<&str> = parts.iter().filter_map(|p| p["text"].as_str()).collect();
            text_or_error((!text.is_empty()).then(|| text.concat()))
        }
        Provider::Claude => {
            let model = settings.claude_model.as_str();
            let mut body = json!({
                "model": model,
                "max_tokens": 16000,
                "system": system_prompt(),
                "messages": [{"role": "user", "content": question}],
            });
            let mut headers = vec![("x-api-key", need_key()?), ("anthropic-version", "2026-10-21")];
            // On the current large models a declined answer is retried on a
            // suitable fallback model by the server.
            if [
                "claude-opus-5-5",
                "claude-opus-5",
                "claude-sonnet-5-5",
                "claude-fable-5-1",
                "claude-fable-5-2",
            ]
            .contains(&model)
            {
                body["fallbacks"] = json!("default");
                headers.push(("anthropic-beta", "server-side-fallback-2026-07-01"));
            }
            let v = post("https://api.anthropic.com/v1/messages", &headers, &body)?;
            if v["stop_reason"] == "refusal" {
                return Err(io::Error::other("the AI declined to answer"));
            }
            let blocks = v["content"].as_array().cloned().unwrap_or_default();
            let text: Vec<&str> =
                blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
            text_or_error((!text.is_empty()).then(|| text.join("\n")))
        }
        Provider::OpenRouter => {
            if settings.openrouter_model.trim().is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "no model chosen"));
            }
            let body = json!({
                "model": settings.openrouter_model.trim(),
                "max_tokens": 4000,
                "messages": [
                    {"role": "system", "content": system_prompt()},
                    {"role": "user", "content": question},
                ],
            });
            let auth = format!("Bearer {}", need_key()?);
            let headers = [
                ("Authorization", auth.as_str()),
                // Shown on openrouter.ai as the calling program.
                ("HTTP-Referer", "https://mrmarkus42.github.io/MrSizeMe/"),
                ("X-Title", "MrSizeMe"),
            ];
            let v = post("https://openrouter.ai/api/v1/chat/completions", &headers, &body)?;
            text_or_error(v["choices"][0]["message"]["content"].as_str().map(str::to_string))
        }
    }
}

fn text_or_error(t: Option<String>) -> io::Result<String> {
    t.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).ok_or_else(|| io::Error::other("empty answer"))
}

/// A model OpenRouter offers.
#[derive(Debug, Clone, PartialEq)]
pub struct RouterModel {
    pub id: String,
    pub name: String,
    /// Costs nothing (prompt and answer free).
    pub free: bool,
}

/// Reads the model list of OpenRouter (an answer of `/api/v1/models`).
pub fn parse_router_models(v: &Value) -> Vec<RouterModel> {
    let mut list: Vec<RouterModel> = v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m["id"].as_str()?.to_string();
            let name = m["name"].as_str().unwrap_or(&id).chars().take(120).collect();
            let zero =
                |k: &str| m["pricing"][k].as_str().is_some_and(|p| p.parse::<f64>().is_ok_and(|x| x == 0.0));
            Some(RouterModel { free: zero("prompt") && zero("completion"), id, name })
        })
        .collect();
    list.sort_by_key(|m| m.name.to_lowercase());
    list
}

/// The models OpenRouter offers (no key needed for the list).
pub fn openrouter_models(settings: &Settings) -> io::Result<Vec<RouterModel>> {
    check_allowed(settings, Provider::OpenRouter)?;
    let mut resp = agent()
        .get("https://openrouter.ai/api/v1/models")
        .call()
        .map_err(|e| io::Error::other(e.to_string()))?;
    let v: Value = resp.body_mut().read_json().map_err(|e| io::Error::other(e.to_string()))?;
    Ok(parse_router_models(&v))
}

/// The models a local Ollama offers.
pub fn ollama_models(settings: &Settings) -> io::Result<Vec<String>> {
    check_allowed(settings, Provider::Ollama)?;
    let url = format!("{}/api/tags", settings.ollama_url.trim_end_matches('/'));
    let mut resp = agent().get(&url).call().map_err(|e| io::Error::other(e.to_string()))?;
    let v: Value = resp.body_mut().read_json().map_err(|e| io::Error::other(e.to_string()))?;
    Ok(v["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["name"].as_str().map(str::to_string))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Kind, Node, ROOT};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn names_are_anonymous() {
        let root = if cfg!(windows) { r"C:\Users\anna" } else { "/home/anna" };
        let mut t = Tree::new(PathBuf::from(root), Kind::Dir);
        let lib = t.add_children(ROOT, [Node::new(OsStr::new("Library").into(), NO_PARENT, Kind::Dir)]);
        let caches = t.add_children(lib, [Node::new(OsStr::new("Caches").into(), NO_PARENT, Kind::Dir)]);
        let deep = t.add_children(caches, [Node::new(OsStr::new("com.app").into(), NO_PARENT, Kind::Dir)]);
        t.sum_up();
        let private = vec!["anna".to_string()];
        let sep = if cfg!(windows) { "\\" } else { "/" };
        assert_eq!(anonymous_name_with(&t, deep, &private), ["Library", "Caches", "com.app"].join(sep));
        // The user name in the root path is replaced.
        let shown = anonymous_name_with(&t, lib, &private);
        assert!(!shown.contains("anna") && shown.ends_with("Library"), "{shown}");
        let shown = anonymous_name_with(&t, ROOT, &private);
        assert!(!shown.contains("anna"), "{shown}");
    }

    #[test]
    fn google_addresses() {
        assert_eq!(url_encode("a b&c=ü"), "a+b%26c%3D%C3%BC");
        let u = google_url("Was ist WinSxS?", true);
        assert!(u.starts_with("https://www.google.com/search?udm=50&q=Was+ist+WinSxS%3F"));
        assert!(!google_url("x", false).contains("udm"));
    }

    #[test]
    fn nothing_goes_out_in_tests() {
        let s = Settings::default();
        for p in [Provider::Ollama, Provider::Gemini, Provider::Claude, Provider::OpenRouter] {
            assert!(ask(p, "frage", &s, Some("key")).is_err());
        }
        assert!(ollama_models(&s).is_err());
        assert!(openrouter_models(&s).is_err());
    }

    #[test]
    fn providers_and_router_models() {
        for p in Provider::ALL {
            assert_eq!(Provider::from_code(p.code()), Some(p));
            assert_eq!(Provider::ALL[p.index()], p);
        }
        assert!(Provider::OpenRouter.is_online() && !Provider::OpenRouter.uses_browser());
        let v = json!({"data": [
            {"id": "b/paid", "name": "Bezahlt", "pricing": {"prompt": "0.000001", "completion": "0.000002"}},
            {"id": "a/free:free", "name": "Gratis", "pricing": {"prompt": "0", "completion": "0"}},
            {"name": "ohne id"}
        ]});
        let m = parse_router_models(&v);
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].id.as_str(), m[0].free), ("b/paid", false));
        assert_eq!((m[1].id.as_str(), m[1].free), ("a/free:free", true));
    }
}
