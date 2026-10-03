//! Panneau IA : questions et actions sur le document ouvert, confiées à un agent installé
//! sur le poste (Claude Code ou Codex), comme dans Lunas Mail (voir docs/ia.md).
//!
//! L'agent est lancé comme un programme séparé, dans un dossier temporaire vide, la
//! consigne sur son entrée standard. Le texte remonte au front par l'événement
//! `AiChunkEvent`, au fil de l'eau. Avec Claude Code, un serveur MCP local expose les
//! outils du document le temps de la demande ; ses modifications sont signalées par
//! `AiEditedEvent`. Rien n'est envoyé sans une question de l'utilisateur, et seulement le
//! document ouvert.

pub mod agent;
pub mod mcp;
pub mod prompts;
pub mod tools;

use std::{
    collections::HashMap,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use lunas_pdf_core::{DocId, Engine, annot::EditState};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::OnceCell,
};
use tokio_util::sync::CancellationToken;

use agent::{Agent, Chunk};
use prompts::ChatTurn;

/// Question simple (test des préférences).
const TIMEOUT: Duration = Duration::from_secs(150);
/// Demande sur le document : plusieurs outils, rendus de pages.
const CHAT_TIMEOUT: Duration = Duration::from_secs(900);

/// Tâche demandée par le front.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AiTask {
    /// Vérification depuis les préférences.
    Test,
    /// Question ou action sur le document ouvert.
    Chat {
        doc: DocId,
        name: String,
        /// Taille des pages affichées (points).
        pages: Vec<(f32, f32)>,
        /// Langue de l'interface (« fr », « en »).
        lang: String,
        history: Vec<ChatTurn>,
        question: String,
    },
}

/// Morceau de réponse.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct AiChunkEvent {
    pub request_id: String,
    pub delta: String,
}

/// Appel d'outil par l'agent, pour montrer ce qu'il fait.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct AiToolEvent {
    pub request_id: String,
    /// Nom sans le préfixe MCP (`get_page_text`…).
    pub name: String,
    /// Arguments, en JSON.
    pub input: String,
}

/// Le document a été modifié par l'agent : nouvel état d'édition de l'onglet.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct AiEditedEvent {
    pub doc: DocId,
    pub state: EditState,
}

/// L'agent propose de mémoriser des informations : carte dans le panneau.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryEvent {
    pub request_id: String,
    pub add: Vec<String>,
    /// Informations déjà mémorisées que les nouvelles remplacent.
    pub replace: Vec<String>,
}

/// Erreur d'une demande ; le front affiche le message traduit.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiError {
    pub kind: AiErrorKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiErrorKind {
    NotConfigured,
    NotFound,
    Outdated,
    Failed,
    Cancelled,
    Timeout,
}

impl AiError {
    fn new(kind: AiErrorKind, detail: impl ToString) -> Self {
        AiError {
            kind,
            detail: detail.to_string(),
        }
    }
    fn failed(detail: impl ToString) -> Self {
        Self::new(AiErrorKind::Failed, detail)
    }
}

/// Résultat de la détection, pour les préférences.
#[derive(Debug, Clone, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub path: Option<String>,
    pub version: Option<String>,
    /// Installation trouvée mais trop ancienne (options inconnues).
    pub outdated: Option<OutdatedAgent>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct OutdatedAgent {
    pub path: String,
    pub version: Option<String>,
}

/// Réglages de l'IA (dans les préférences).
#[derive(Debug, Clone)]
pub struct AiConfig {
    pub agent: Agent,
    pub path: Option<String>,
    pub model: Option<String>,
}

pub struct AiService {
    app: AppHandle,
    running: Mutex<HashMap<String, CancellationToken>>,
    /// PATH du shell de connexion : l'app lancée depuis le Dock ne l'a pas, et Codex
    /// (script Node) en a besoin pour trouver `node`.
    login_path: OnceCell<Option<String>>,
}

/// Paramètres d'un lancement de l'agent.
struct Run<'a> {
    request_id: &'a str,
    agent: Agent,
    path: &'a str,
    model: Option<&'a str>,
    system: String,
    extra: Vec<String>,
    timeout: Duration,
}

impl AiService {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            running: Mutex::new(HashMap::new()),
            login_path: OnceCell::new(),
        }
    }

    fn config(&self) -> AiConfig {
        let s = self.app.state::<crate::store::Store>().settings.lock().unwrap().clone();
        let opt = |v: String| Some(v.trim().to_owned()).filter(|v| !v.is_empty());
        AiConfig {
            agent: s.ai_agent,
            path: opt(s.ai_path),
            model: opt(s.ai_model),
        }
    }

    async fn login_path(&self) -> Option<String> {
        self.login_path
            .get_or_init(|| async {
                if cfg!(windows) {
                    return None;
                }
                shell_value("\"$PATH\"").await
            })
            .await
            .clone()
    }

    /// Installation de l'agent : comme le terminal la trouverait (shell interactif, donc
    /// avec `.zshrc`), sinon via le shell de connexion, sinon dans les emplacements
    /// habituels. La première qui accepte nos options gagne ; une installation trop
    /// ancienne est signalée au lieu d'être lancée.
    pub async fn detect(&self, agent: Agent) -> Detected {
        let Some(bin) = agent.binary() else {
            return Detected::default();
        };
        let mut found: Vec<String> = Vec::new();
        if cfg!(windows) {
            if let Ok(out) = Command::new("where.exe").arg(bin).output().await {
                found.extend(
                    String::from_utf8_lossy(&out.stdout)
                        .lines()
                        .map(|l| l.trim().to_owned())
                        .filter(|l| !l.is_empty()),
                );
            }
        } else {
            for interactive in [true, false] {
                if let Some(p) = shell_run(&format!("\"$(command -v {bin})\""), interactive)
                    .await
                    .filter(|p| p.starts_with('/'))
                {
                    found.push(p);
                }
            }
            let home = std::env::var("HOME").unwrap_or_default();
            found.extend(
                agent::candidates(&home, bin)
                    .into_iter()
                    .filter(|p| std::path::Path::new(p).is_file()),
            );
        }
        let mut outdated = None;
        let mut seen = std::collections::HashSet::new();
        for path in found.into_iter().filter(|p| seen.insert(p.clone())) {
            let (help, version) = (probe(&path, "--help").await, probe(&path, "--version").await);
            let version = version.as_deref().and_then(agent::parse_version);
            if help.as_deref().is_some_and(|h| agent::supports(agent, h)) {
                return Detected {
                    path: Some(path),
                    version,
                    outdated,
                };
            }
            outdated.get_or_insert(OutdatedAgent { path, version });
        }
        Detected {
            path: None,
            version: None,
            outdated,
        }
    }

    pub fn cancel(&self, request_id: &str) {
        if let Some(t) = self.running.lock().unwrap().remove(request_id) {
            t.cancel();
        }
    }

    /// Exécute une tâche ; le texte arrive par morceaux et le résultat complet est
    /// renvoyé à la fin.
    pub async fn run(&self, request_id: String, task: AiTask) -> Result<String, AiError> {
        let cfg = self.config();
        if cfg.agent == Agent::None {
            return Err(AiError::new(AiErrorKind::NotConfigured, ""));
        }
        let path = match cfg.path.clone() {
            Some(p) => p,
            None => {
                let d = self.detect(cfg.agent).await;
                match (d.path, d.outdated) {
                    (Some(p), _) => p,
                    (None, Some(old)) => {
                        return Err(AiError::new(
                            AiErrorKind::Outdated,
                            format!("{} {}", old.path, old.version.unwrap_or_default()),
                        ));
                    }
                    (None, None) => return Err(AiError::new(AiErrorKind::NotFound, "")),
                }
            }
        };
        let token = CancellationToken::new();
        self.running.lock().unwrap().insert(request_id.clone(), token.clone());
        let result = match task {
            AiTask::Test => {
                let run = Run {
                    request_id: &request_id,
                    agent: cfg.agent,
                    path: &path,
                    model: cfg.model.as_deref(),
                    system: "You only produce text. Reply with the requested output only.".into(),
                    extra: vec![],
                    timeout: TIMEOUT,
                };
                self.execute(run, "Reply with exactly: OK", token).await
            }
            AiTask::Chat {
                doc,
                name,
                pages,
                lang,
                history,
                question,
            } => {
                self.chat(
                    &request_id,
                    &cfg,
                    &path,
                    ChatDoc { doc, name, pages, lang },
                    &history,
                    &question,
                    token,
                )
                .await
            }
        };
        self.running.lock().unwrap().remove(&request_id);
        result.map(|t| t.trim().to_owned())
    }

    /// Question sur le document : texte du document en contexte ; avec Claude Code, les
    /// outils du document par un serveur MCP local le temps de la demande. Codex : réponse
    /// en texte seulement (pas de serveur MCP).
    #[allow(clippy::too_many_arguments)]
    async fn chat(
        &self,
        request_id: &str,
        cfg: &AiConfig,
        path: &str,
        d: ChatDoc,
        history: &[ChatTurn],
        question: &str,
        token: CancellationToken,
    ) -> Result<String, AiError> {
        let engine = self.app.state::<Engine>().inner().clone();
        let (doc, n) = (d.doc, d.pages.len());
        let texts = {
            let engine = engine.clone();
            tauri::async_runtime::spawn_blocking(move || {
                (0..n as u32)
                    .map(|p| {
                        engine
                            .text(doc, p)
                            .map(|t| {
                                t.runs
                                    .iter()
                                    .map(|r| if r.eol { format!("{}\n", r.text) } else { r.text.clone() })
                                    .collect::<String>()
                            })
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .map_err(AiError::failed)?
        };
        let memory = self.app.state::<crate::store::Store>().memory();
        let prompt = prompts::chat(&prompts::document_block(&d.name, &texts), &memory, history, question);
        let tools = cfg.agent == Agent::Claude;
        let system = prompts::system(&d.lang, &d.name, n, tools);
        if !tools {
            let run = Run {
                request_id,
                agent: cfg.agent,
                path,
                model: cfg.model.as_deref(),
                system,
                extra: vec![],
                timeout: CHAT_TIMEOUT,
            };
            return self.execute(run, &prompt, token).await;
        }
        let (app, app2, rid) = (self.app.clone(), self.app.clone(), request_id.to_owned());
        let toolbox = Arc::new(tools::Toolbox {
            engine,
            doc,
            pages: d.pages,
            edited: Arc::new(move |state| {
                let _ = AiEditedEvent { doc, state }.emit(&app);
            }),
            propose: Arc::new(move |add, replace| {
                let _ = AiMemoryEvent {
                    request_id: rid.clone(),
                    add,
                    replace,
                }
                .emit(&app2);
            }),
        });
        let call: mcp::ToolFn = Arc::new(move |name, args| {
            let tb = toolbox.clone();
            Box::pin(async move { tb.call(&name, &args).await })
        });
        let key = random_key();
        let server = mcp::start(key.clone(), tools::definitions(), call)
            .await
            .map_err(AiError::failed)?;
        let run = Run {
            request_id,
            agent: cfg.agent,
            path,
            model: cfg.model.as_deref(),
            system,
            extra: agent::chat_args(&server.url, &key),
            timeout: CHAT_TIMEOUT,
        };
        let result = self.execute(run, &prompt, token).await;
        drop(server);
        result
    }

    async fn execute(&self, run: Run<'_>, prompt: &str, token: CancellationToken) -> Result<String, AiError> {
        let Run {
            request_id,
            agent,
            path,
            model,
            system,
            extra,
            timeout,
        } = run;
        let mut args = agent::args(agent, model, &system);
        args.extend(extra);
        // Windows : claude et codex sont des scripts .cmd, lancés par cmd.exe.
        let mut cmd = if cfg!(windows) && (path.ends_with(".cmd") || path.ends_with(".bat")) {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(path);
            c
        } else {
            Command::new(path)
        };
        let workdir = std::env::temp_dir().join("lunas-pdf-ai");
        let _ = std::fs::create_dir_all(&workdir);
        cmd.args(&args)
            .current_dir(&workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(p) = self.login_path().await {
            cmd.env("PATH", p);
        }
        let mut child = cmd.spawn().map_err(|e| AiError::failed(format!("{path} : {e}")))?;

        let payload = agent::stdin_payload(agent, &system, prompt);
        let mut stdin = child.stdin.take().expect("stdin demandé");
        tokio::spawn(async move {
            let _ = stdin.write_all(payload.as_bytes()).await;
            let _ = stdin.shutdown().await;
        });
        let mut stderr = child.stderr.take().expect("stderr demandé");
        let err_task = tokio::spawn(async move {
            let mut s = String::new();
            let _ = stderr.read_to_string(&mut s).await;
            s
        });
        let mut lines = BufReader::new(child.stdout.take().expect("stdout demandé")).lines();

        let emit = |delta: String| {
            let _ = AiChunkEvent {
                request_id: request_id.to_owned(),
                delta,
            }
            .emit(&self.app);
        };
        let read = async {
            let mut text = String::new();
            let mut done: Option<String> = None;
            while let Some(line) = lines.next_line().await.map_err(AiError::failed)? {
                match agent::parse_line(agent, &line) {
                    Some(Chunk::Delta(d)) => {
                        text.push_str(&d);
                        emit(d);
                    }
                    Some(Chunk::Message(m)) => {
                        let piece = if text.is_empty() { m } else { format!("\n\n{m}") };
                        text.push_str(&piece);
                        emit(piece);
                    }
                    Some(Chunk::Done(t)) => done = Some(if t.trim().is_empty() { text.clone() } else { t }),
                    Some(Chunk::Error(e)) => return Err(AiError::failed(e)),
                    Some(Chunk::Tool(name, input)) => {
                        let _ = AiToolEvent {
                            request_id: request_id.to_owned(),
                            name: name.trim_start_matches("mcp__lunas__").to_owned(),
                            input: input.to_string(),
                        }
                        .emit(&self.app);
                    }
                    None => {}
                }
            }
            Ok::<_, AiError>(done.unwrap_or(text))
        };
        let outcome = tokio::select! {
            r = read => r,
            _ = token.cancelled() => Err(AiError::new(AiErrorKind::Cancelled, "")),
            _ = tokio::time::sleep(timeout) => Err(AiError::new(AiErrorKind::Timeout, "")),
        };
        let text = match outcome {
            Ok(t) => t,
            Err(e) => {
                let _ = child.kill().await;
                return Err(e);
            }
        };
        let status = child.wait().await.map_err(AiError::failed)?;
        let stderr = err_task.await.unwrap_or_default();
        if text.trim().is_empty() {
            let detail = stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim()
                .to_owned();
            // Chemin saisi à la main vers une version trop ancienne.
            if detail.contains("unknown option") {
                return Err(AiError::new(AiErrorKind::Outdated, format!("{path} ({detail})")));
            }
            let detail = if detail.is_empty() {
                format!("exit {}", status.code().unwrap_or(-1))
            } else {
                detail
            };
            return Err(AiError::failed(detail));
        }
        Ok(text)
    }
}

/// Document de la demande.
struct ChatDoc {
    doc: DocId,
    name: String,
    pages: Vec<(f32, f32)>,
    lang: String,
}

/// Clé du serveur MCP (32 octets aléatoires, en hexadécimal).
fn random_key() -> String {
    let mut b = [0u8; 32];
    let _ = getrandom::getrandom(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Valeur d'une expression shell (`"$PATH"`…), telle que le terminal la voit : shell
/// interactif d'abord (lit `.zshrc`), puis shell de connexion seul.
async fn shell_value(expr: &str) -> Option<String> {
    match shell_run(expr, true).await {
        Some(v) => Some(v),
        None => shell_run(expr, false).await,
    }
}

async fn shell_run(expr: &str, interactive: bool) -> Option<String> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/zsh".into());
    let script = format!("printf '{m}%s{m}' {expr}", m = agent::MARK);
    let run = Command::new(shell)
        .arg(if interactive { "-lic" } else { "-lc" })
        .arg(script)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(Duration::from_secs(8), run).await.ok()?.ok()?;
    agent::extract_marked(&String::from_utf8_lossy(&out.stdout))
}

/// Sortie de `programme <arg>` (aide, version), en 8 s au plus.
async fn probe(path: &str, arg: &str) -> Option<String> {
    let run = Command::new(path).arg(arg).stdin(Stdio::null()).kill_on_drop(true).output();
    let out = tokio::time::timeout(Duration::from_secs(8), run).await.ok()?.ok()?;
    Some(format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    ))
}

/// Essais réels avec Claude Code (réseau et abonnement requis) :
/// `LUNAS_CLAUDE=~/.local/bin/claude cargo test -p lunas-pdf ai::essais::<essai> -- --ignored --nocapture`
/// (un essai par commande : PDFium ne se charge qu'une fois par processus).
#[cfg(test)]
mod essais {
    use super::*;
    use lunas_pdf_core::EditRequest;
    use lunas_pdf_core::annot::AnnotBody;

    fn engine() -> Engine {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdfium/lib");
        Engine::start(Some(&dir)).expect("libpdfium : lancer `bun pdfium`")
    }

    /// Ouvre une copie du fixture, pose la question à Claude Code avec les outils (et les
    /// informations mémorisées), renvoie le moteur, le document et les propositions de
    /// mémorisation.
    async fn ask(fixture: &str, memory: &[String], question: &str) -> Option<(Engine, DocId, Vec<String>)> {
        let claude = std::env::var("LUNAS_CLAUDE").ok()?;
        let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures")
            .join(fixture);
        let tmp = std::env::temp_dir().join(format!("lunas-pdf-ai-{}-{fixture}", std::process::id()));
        std::fs::copy(&src, &tmp).unwrap();
        let engine = engine();
        let info = engine.open(&tmp, None).unwrap();
        let pages: Vec<(f32, f32)> = info.pages.iter().map(|p| (p.width, p.height)).collect();
        let texts: Vec<String> = (0..pages.len() as u32)
            .map(|p| {
                engine
                    .text(info.id, p)
                    .unwrap()
                    .runs
                    .iter()
                    .map(|r| if r.eol { format!("{}\n", r.text) } else { r.text.clone() })
                    .collect()
            })
            .collect();
        let proposals = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = proposals.clone();
        let toolbox = Arc::new(tools::Toolbox {
            engine: engine.clone(),
            doc: info.id,
            pages: pages.clone(),
            edited: Arc::new(|_| {}),
            propose: Arc::new(move |add, replace| {
                println!("mémoriser : {add:?} (remplace {replace:?})");
                seen.lock().unwrap().extend(add);
            }),
        });
        let call: mcp::ToolFn = Arc::new(move |name, args| {
            let tb = toolbox.clone();
            Box::pin(async move { tb.call(&name, &args).await })
        });
        let key = random_key();
        let server = mcp::start(key.clone(), tools::definitions(), call).await.unwrap();
        let mut args = agent::args(
            Agent::Claude,
            Some("sonnet"),
            &prompts::system("fr", fixture, pages.len(), true),
        );
        args.extend(agent::chat_args(&server.url, &key));
        let mut child = Command::new(claude)
            .args(&args)
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(prompts::chat(&prompts::document_block(fixture, &texts), memory, &[], question).as_bytes())
            .await
            .unwrap();
        drop(stdin);
        let out = child.wait_with_output().await.unwrap();
        let mut answer = String::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            match agent::parse_line(Agent::Claude, line) {
                Some(Chunk::Tool(n, i)) => println!("outil : {n} {i}"),
                Some(Chunk::Done(t)) => answer = t,
                Some(Chunk::Error(e)) => panic!("{e}"),
                _ => {}
            }
        }
        println!("réponse :\n{answer}");
        let proposed = proposals.lock().unwrap().clone();
        Some((engine, info.id, proposed))
    }

    #[tokio::test]
    #[ignore]
    async fn remplit_un_formulaire() {
        let Some((engine, doc, proposed)) = ask(
            "formulaire-acroform.pdf",
            &[],
            "Remplis ce formulaire pour Jean Dupont, né le 12/03/1985, 10 rue de la Paix 75002 Paris, jean.dupont@example.com.",
        )
        .await
        else {
            return;
        };
        let st = engine.edit(doc, EditRequest::Load).unwrap();
        for f in &st.fields {
            println!("champ {} = {:?}", f.id, f.value);
        }
        assert!(
            st.fields.iter().any(|f| f.value.iter().any(|v| v.contains("Dupont"))),
            "nom rempli"
        );
        assert!(
            st.annots.iter().all(|a| !matches!(a.body, AnnotBody::FreeText { .. })),
            "pas de texte par-dessus les champs"
        );
        assert!(
            proposed.iter().any(|f| f.contains("jean.dupont@example.com")),
            "propose de mémoriser : {proposed:?}"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn utilise_la_memoire() {
        let memory = [
            "Nom : Martin".to_owned(),
            "Prénom : Claire".to_owned(),
            "E-mail : claire.martin@example.com".to_owned(),
        ];
        let Some((engine, doc, proposed)) = ask("formulaire-acroform.pdf", &memory, "Remplis ce formulaire.").await else {
            return;
        };
        let st = engine.edit(doc, EditRequest::Load).unwrap();
        let value = |id: &str| {
            st.fields
                .iter()
                .find(|f| f.id == id)
                .map(|f| f.value.clone())
                .unwrap_or_default()
        };
        assert_eq!(value("nom"), ["Martin"]);
        assert_eq!(value("courriel"), ["claire.martin@example.com"]);
        assert!(proposed.is_empty(), "rien de nouveau à mémoriser : {proposed:?}");
    }

    #[tokio::test]
    #[ignore]
    async fn place_texte_et_coche() {
        let Some((engine, doc, _)) = ask(
            "texte-simple.pdf",
            &[],
            "Sur la page 1, écris « VU » juste à droite de la fin de la ligne 3, et coche à gauche du début de la ligne 5.",
        )
        .await
        else {
            return;
        };
        let segs = tools::segments(&engine.text(doc, 0).unwrap().runs);
        let line = |n: &str| {
            segs.iter()
                .find(|(_, t)| t.starts_with(&format!("Ligne {n} ")))
                .map(|(r, _)| *r)
                .unwrap()
        };
        let (l3, l5) = (line("3"), line("5"));
        let st = engine.edit(doc, EditRequest::Load).unwrap();
        for a in &st.annots {
            println!("annotation {:?} {:?}", a.body, a.rect);
        }
        let vu = st
            .annots
            .iter()
            .find(|a| matches!(&a.body, AnnotBody::FreeText { text, .. } if text.contains("VU")))
            .expect("texte « VU »");
        let cy = vu.rect.y + vu.rect.h / 2.0;
        assert!(
            (cy - (l3.y + l3.h / 2.0)).abs() < l3.h,
            "« VU » sur la ligne 3 : {:?} / {:?}",
            vu.rect,
            l3
        );
        assert!(vu.rect.x >= l3.x + l3.w - 4.0, "à droite de la ligne 3");
        let check = st
            .annots
            .iter()
            .find(|a| matches!(a.body, AnnotBody::Check { .. }))
            .expect("coche");
        let cy = check.rect.y + check.rect.h / 2.0;
        assert!(
            (cy - (l5.y + l5.h / 2.0)).abs() < l5.h,
            "coche sur la ligne 5 : {:?} / {:?}",
            check.rect,
            l5
        );
        assert!(check.rect.x + check.rect.w <= l5.x + 4.0, "à gauche de la ligne 5");
    }
}
