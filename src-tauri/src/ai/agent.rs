//! Agents installés sur le poste (Claude Code, Codex), lancés en programme
//! séparé, sans interaction. Arguments et lecture du flux : logique pure.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    #[default]
    None,
    Claude,
    Codex,
}

impl Agent {
    /// Nom du programme à chercher.
    pub fn binary(self) -> Option<&'static str> {
        match self {
            Agent::None => None,
            Agent::Claude => Some("claude"),
            Agent::Codex => Some("codex"),
        }
    }
}

/// Arguments de la ligne de commande. La consigne passe par l'entrée standard,
/// jamais par les arguments (taille, et rien de visible dans la liste des processus).
///
/// Claude Code (vérifié avec la version 2.1.282) :
/// - `--tools ""` : aucun outil. Un message piégé (« lis ~/.ssh… ») ne peut rien
///   faire d'autre que produire du texte ;
/// - `--setting-sources ""`, `--strict-mcp-config` : ni réglages, ni hooks, ni
///   serveurs MCP de l'utilisateur ;
/// - `--system-prompt` remplace celui de l'agent de code : un simple assistant ;
/// - `stream-json` + messages partiels : le texte arrive au fil de l'eau.
///
/// Codex (`codex exec`, d'après la documentation officielle, non testé) : lecture
/// seule, sans session enregistrée. Il ne permet pas de couper ses outils : il peut
/// lire des fichiers, d'où l'avertissement dans les réglages.
pub fn args(agent: Agent, model: Option<&str>, system: &str) -> Vec<String> {
    let mut a: Vec<String> = match agent {
        Agent::None => return Vec::new(),
        Agent::Claude => [
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--tools",
            "",
            "--setting-sources",
            "",
            "--strict-mcp-config",
            "--no-session-persistence",
            "--system-prompt",
            system,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        Agent::Codex => [
            "exec",
            "--json",
            "--ephemeral",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    };
    if let Some(m) = model.map(str::trim).filter(|m| !m.is_empty()) {
        a.push(if agent == Agent::Claude { "--model" } else { "-m" }.into());
        a.push(m.into());
    }
    if agent == Agent::Codex {
        // Consigne sur l'entrée standard.
        a.push("-".into());
    }
    a
}

/// Options de Claude Code pour le panneau IA : les seuls outils disponibles
/// sont ceux du serveur MCP de Lunas PDF (`--tools ""` retire les outils intégrés,
/// `--strict-mcp-config` tout autre serveur MCP), autorisés sans demande. Vérifié
/// avec la version 2.1.282.
pub fn chat_args(url: &str, token: &str) -> Vec<String> {
    let config = serde_json::json!({
        "mcpServers": { "lunas": { "type": "http", "url": url, "headers": { "Authorization": format!("Bearer {token}") } } }
    });
    vec![
        "--mcp-config".into(),
        config.to_string(),
        "--allowedTools".into(),
        "mcp__lunas".into(),
        "--max-turns".into(),
        "30".into(),
    ]
}

/// Codex n'a pas d'invite système séparée : elle précède la demande.
pub fn stdin_payload(agent: Agent, system: &str, prompt: &str) -> String {
    match agent {
        Agent::Codex => format!("{system}\n\n---\n\n{prompt}"),
        _ => prompt.to_owned(),
    }
}

/// Ce qu'une ligne du flux apporte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chunk {
    /// Morceau de texte à ajouter.
    Delta(String),
    /// Texte complet d'un message (Codex ne livre ses messages qu'entiers ;
    /// Claude Code répète à la fin le texte déjà reçu par morceaux).
    Message(String),
    /// Fin, avec le texte final complet.
    Done(String),
    Error(String),
    /// Appel d'outil (nom, arguments), pour montrer ce que fait l'agent.
    Tool(String, Value),
}

pub fn parse_line(agent: Agent, line: &str) -> Option<Chunk> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let s = |v: &Value| v.as_str().map(str::to_owned);
    match agent {
        Agent::Claude => match v.get("type")?.as_str()? {
            "stream_event" => {
                let e = v.get("event")?;
                let d = e.get("delta")?;
                (e.get("type")?.as_str()? == "content_block_delta" && d.get("type")?.as_str()? == "text_delta")
                    .then(|| s(d.get("text")?).map(Chunk::Delta))
                    .flatten()
            }
            // Message complet de l'agent : on n'en retient que les appels d'outils (le
            // texte est déjà arrivé par morceaux).
            "assistant" => v.pointer("/message/content")?.as_array()?.iter().find_map(|c| {
                if c.get("type")?.as_str()? == "tool_use" {
                    Some(Chunk::Tool(
                        c.get("name")?.as_str()?.to_owned(),
                        c.get("input").cloned().unwrap_or(Value::Null),
                    ))
                } else {
                    None
                }
            }),
            "result" => {
                let text = v.get("result").and_then(s).unwrap_or_default();
                if v.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
                    Some(Chunk::Error(if text.is_empty() {
                        v.get("subtype").and_then(s).unwrap_or_default()
                    } else {
                        text
                    }))
                } else {
                    Some(Chunk::Done(text))
                }
            }
            _ => None,
        },
        Agent::Codex => match v.get("type")?.as_str()? {
            "item.completed" => {
                let item = v.get("item")?;
                (item.get("type")?.as_str()? == "agent_message")
                    .then(|| s(item.get("text")?).map(Chunk::Message))
                    .flatten()
            }
            "turn.failed" => Some(Chunk::Error(
                v.pointer("/error/message")
                    .and_then(s)
                    .unwrap_or_else(|| "turn.failed".into()),
            )),
            "error" => Some(Chunk::Error(v.get("message").and_then(s).unwrap_or_else(|| "error".into()))),
            _ => None,
        },
        Agent::None => None,
    }
}

/// Valeur encadrée par des marqueurs : les fichiers de démarrage du shell
/// (`.zshrc`…) peuvent écrire autre chose sur la sortie.
pub const MARK: &str = "__LUNAS__";

pub fn extract_marked(output: &str) -> Option<String> {
    let start = output.find(MARK)? + MARK.len();
    let len = output[start..].find(MARK)?;
    Some(output[start..start + len].trim().to_owned()).filter(|v| !v.is_empty())
}

/// L'installation accepte-t-elle les options utilisées ? Claude Code avant 2.1
/// (ex. 2.0.1) ne connaît pas `--tools` : on le refuse plutôt que de le lancer
/// avec ses outils.
pub fn supports(agent: Agent, help: &str) -> bool {
    match agent {
        Agent::Claude => help.contains("--tools ") && help.contains("--system-prompt"),
        Agent::Codex => help.contains("--json") || help.contains("exec"),
        Agent::None => false,
    }
}

/// « 2.1.282 (Claude Code) » → « 2.1.282 ».
pub fn parse_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.contains('.'))
        .map(str::to_owned)
}

/// Emplacements habituels, quand le shell de connexion ne trouve rien (l'app
/// lancée depuis le Dock n'hérite pas du PATH du terminal).
pub fn candidates(home: &str, binary: &str) -> Vec<String> {
    [
        format!("{home}/.local/bin/{binary}"),
        format!("{home}/.claude/local/{binary}"),
        format!("/opt/homebrew/bin/{binary}"),
        format!("/usr/local/bin/{binary}"),
        format!("{home}/.npm-global/bin/{binary}"),
        format!("{home}/.bun/bin/{binary}"),
        format!("{home}/.volta/bin/{binary}"),
        format!("/usr/bin/{binary}"),
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments() {
        let a = args(Agent::Claude, Some("sonnet"), "SYS");
        let i = a.iter().position(|x| x == "--tools").unwrap();
        assert_eq!(a[i + 1], "", "aucun outil");
        assert!(a.windows(2).any(|w| w == ["--system-prompt", "SYS"]));
        assert!(a.ends_with(&["--model".into(), "sonnet".into()]));
        let c = args(Agent::Codex, None, "SYS");
        assert!(c.windows(2).any(|w| w == ["--sandbox", "read-only"]));
        assert_eq!(c.last().unwrap(), "-");
        assert!(args(Agent::None, None, "").is_empty());
        assert_eq!(stdin_payload(Agent::Codex, "S", "P"), "S\n\n---\n\nP");
        assert_eq!(stdin_payload(Agent::Claude, "S", "P"), "P");
    }

    #[test]
    fn flux_claude() {
        // Lignes réelles (Claude Code 2.1.282), abrégées.
        let delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"1\n2"}}}"#;
        assert_eq!(parse_line(Agent::Claude, delta), Some(Chunk::Delta("1\n2".into())));
        let start = r#"{"type":"stream_event","event":{"type":"message_start","message":{}}}"#;
        assert_eq!(parse_line(Agent::Claude, start), None);
        let done = r#"{"type":"result","subtype":"success","is_error":false,"result":"1\n2\n3"}"#;
        assert_eq!(parse_line(Agent::Claude, done), Some(Chunk::Done("1\n2\n3".into())));
        let err = r#"{"type":"result","subtype":"error_during_execution","is_error":true}"#;
        assert_eq!(
            parse_line(Agent::Claude, err),
            Some(Chunk::Error("error_during_execution".into()))
        );
        assert_eq!(parse_line(Agent::Claude, "pas du json"), None);
    }

    #[test]
    fn outils() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__lunas__get_page_text","input":{"page":1}}]}}"#;
        assert_eq!(
            parse_line(Agent::Claude, line),
            Some(Chunk::Tool(
                "mcp__lunas__get_page_text".into(),
                serde_json::json!({ "page": 1 })
            ))
        );
        let text = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"42"}]}}"#;
        assert_eq!(parse_line(Agent::Claude, text), None, "texte déjà reçu par morceaux");
        let a = chat_args("http://127.0.0.1:5/mcp", "k");
        assert!(a.windows(2).any(|w| w == ["--allowedTools", "mcp__lunas"]));
        let cfg: Value = serde_json::from_str(&a[1]).unwrap();
        assert_eq!(cfg["mcpServers"]["lunas"]["headers"]["Authorization"], "Bearer k");
    }

    #[test]
    fn flux_codex() {
        let msg = r#"{"type":"item.completed","item":{"id":"item_3","type":"agent_message","text":"Bonjour"}}"#;
        assert_eq!(parse_line(Agent::Codex, msg), Some(Chunk::Message("Bonjour".into())));
        let reasoning = r#"{"type":"item.completed","item":{"id":"item_1","type":"reasoning","text":"..."}}"#;
        assert_eq!(parse_line(Agent::Codex, reasoning), None);
        let failed = r#"{"type":"turn.failed","error":{"message":"quota"}}"#;
        assert_eq!(parse_line(Agent::Codex, failed), Some(Chunk::Error("quota".into())));
    }

    #[test]
    fn detection() {
        assert_eq!(
            extract_marked("bienvenue !\n__LUNAS__/u/.local/bin/claude__LUNAS__").as_deref(),
            Some("/u/.local/bin/claude")
        );
        assert_eq!(extract_marked("__LUNAS____LUNAS__"), None);
        assert_eq!(extract_marked("rien"), None);
        assert!(supports(
            Agent::Claude,
            "  --tools <tools...>  Specify\n  --system-prompt <prompt>"
        ));
        assert!(
            !supports(Agent::Claude, "  --allowedTools <tools...>\n  --system-prompt <prompt>"),
            "2.0.1"
        );
        assert_eq!(parse_version("2.1.282 (Claude Code)\n").as_deref(), Some("2.1.282"));
        assert_eq!(parse_version("codex-cli 0.46.0").as_deref(), Some("0.46.0"));
    }
}
