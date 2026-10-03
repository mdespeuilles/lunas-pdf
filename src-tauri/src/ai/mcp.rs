//! Serveur MCP local (transport HTTP « streamable », réponses JSON simples) qui
//! expose à l'agent les outils de Lunas PDF, le temps d'une demande. Repris de Lunas
//! Mail ; un outil peut renvoyer des images (rendu d'une page) en plus du texte.
//!
//! Écoute sur 127.0.0.1, port aléatoire, et n'accepte que les requêtes portant la
//! clé aléatoire de la session (`Authorization: Bearer …`).
//!
//! Vérifié avec Claude Code 2.1.282 : il envoie d'abord `server/discover` (hors
//! spécification MCP), puis `initialize`, `notifications/initialized`, un GET
//! (flux SSE facultatif, refusé par 405), `tools/list` et `tools/call`.

use std::{future::Future, pin::Pin, sync::Arc};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

pub type ToolFuture = Pin<Box<dyn Future<Output = Result<Vec<Value>, String>> + Send>>;
/// Exécution d'un outil : nom, arguments → blocs de contenu MCP (texte, image) ou
/// message d'erreur.
pub type ToolFn = Arc<dyn Fn(String, Value) -> ToolFuture + Send + Sync>;

/// Bloc de contenu texte.
pub fn text(s: impl Into<String>) -> Value {
    json!({ "type": "text", "text": s.into() })
}

/// Bloc de contenu image (PNG encodé en base64).
pub fn png(base64: String) -> Value {
    json!({ "type": "image", "data": base64, "mimeType": "image/png" })
}

const MAX_BODY: usize = 1024 * 1024;

pub struct Server {
    pub url: String,
    stop: CancellationToken,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

/// Démarre le serveur ; il s'arrête quand `Server` est abandonné.
pub async fn start(token: String, tools: Vec<Value>, call: ToolFn) -> std::io::Result<Server> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/mcp", listener.local_addr()?);
    let stop = CancellationToken::new();
    let (tools, stop2) = (Arc::new(tools), stop.clone());
    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                a = listener.accept() => a,
                _ = stop2.cancelled() => return,
            };
            let Ok((stream, _)) = accepted else { continue };
            let (token, tools, call) = (token.clone(), tools.clone(), call.clone());
            tokio::spawn(async move {
                let _ = serve(stream, &token, &tools, &call).await;
            });
        }
    });
    Ok(Server { url, stop })
}

#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub body: Vec<u8>,
}

/// En-têtes complets ? Renvoie la requête une fois le corps entier reçu.
pub fn parse(buf: &[u8]) -> Option<Request> {
    let end = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&buf[..end]).ok()?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let (method, path) = (first.next()?.to_owned(), first.next()?.to_owned());
    let mut length = 0usize;
    let mut authorization = None;
    for l in lines {
        let Some((k, v)) = l.split_once(':') else { continue };
        match k.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = v.trim().parse().ok()?,
            "authorization" => authorization = Some(v.trim().to_owned()),
            _ => {}
        }
    }
    let body = buf.get(end + 4..end + 4 + length)?.to_vec();
    Some(Request {
        method,
        path,
        authorization,
        body,
    })
}

async fn serve(mut stream: TcpStream, token: &str, tools: &[Value], call: &ToolFn) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let req = loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(r) = parse(&buf) {
            break r;
        }
        if buf.len() > MAX_BODY {
            return respond(&mut stream, "413 Payload Too Large", None).await;
        }
    };
    if req.authorization.as_deref() != Some(&format!("Bearer {token}")) {
        return respond(&mut stream, "401 Unauthorized", None).await;
    }
    match req.method.as_str() {
        "POST" => {
            let Ok(msg) = serde_json::from_slice::<Value>(&req.body) else {
                return respond(&mut stream, "400 Bad Request", None).await;
            };
            match dispatch(&msg, tools, call).await {
                Some(reply) => respond(&mut stream, "200 OK", Some(&reply)).await,
                None => respond(&mut stream, "202 Accepted", None).await,
            }
        }
        "DELETE" => respond(&mut stream, "200 OK", None).await,
        // Pas de flux SSE côté serveur : permis par la spécification.
        _ => respond(&mut stream, "405 Method Not Allowed", None).await,
    }
}

async fn respond(stream: &mut TcpStream, status: &str, body: Option<&Value>) -> std::io::Result<()> {
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let ctype = if body.is_empty() {
        ""
    } else {
        "Content-Type: application/json\r\n"
    };
    let resp = format!(
        "HTTP/1.1 {status}\r\n{ctype}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.shutdown().await
}

/// Réponse JSON-RPC ; `None` pour une notification (pas d'identifiant).
pub async fn dispatch(msg: &Value, tools: &[Value], call: &ToolFn) -> Option<Value> {
    let id = msg.get("id").cloned().filter(|i| !i.is_null())?;
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let ok = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result });
    let err = |code: i64, message: &str| json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } });
    Some(match method {
        "initialize" => {
            let version = msg
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2025-06-18");
            ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "lunas-pdf", "version": env!("CARGO_PKG_VERSION") },
            }))
        }
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({ "tools": tools })),
        "tools/call" => {
            let name = msg.pointer("/params/name").and_then(Value::as_str).unwrap_or("").to_owned();
            let args = msg.pointer("/params/arguments").cloned().unwrap_or_else(|| json!({}));
            // Une erreur d'outil est un résultat (isError), pour que l'agent la lise.
            match call(name, args).await {
                Ok(content) => ok(json!({ "content": content })),
                Err(e) => ok(json!({ "content": [text(e)], "isError": true })),
            }
        }
        _ => err(-32601, "method not found"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo() -> ToolFn {
        Arc::new(|name, args| {
            Box::pin(async move {
                if name == "fail" {
                    Err("boom".into())
                } else {
                    Ok(vec![text(format!("{name}:{args}"))])
                }
            })
        })
    }

    #[test]
    fn requete_http() {
        let raw = b"POST /mcp HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer k\r\nContent-Length: 2\r\n\r\n{}";
        let r = parse(raw).unwrap();
        assert_eq!(
            (
                r.method.as_str(),
                r.path.as_str(),
                r.authorization.as_deref(),
                r.body.as_slice()
            ),
            ("POST", "/mcp", Some("Bearer k"), &b"{}"[..])
        );
        assert!(
            parse(b"POST /mcp HTTP/1.1\r\nContent-Length: 5\r\n\r\n{}").is_none(),
            "corps incomplet"
        );
        assert!(parse(b"POST /mcp HTTP/1.1\r\n").is_none(), "en-têtes incomplets");
    }

    #[tokio::test]
    async fn json_rpc() {
        let tools = vec![json!({ "name": "t" })];
        let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-11-25" } });
        let r = dispatch(&init, &tools, &echo()).await.unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
        assert_eq!(
            dispatch(
                &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
                &tools,
                &echo()
            )
            .await,
            None
        );
        let list = dispatch(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }), &tools, &echo())
            .await
            .unwrap();
        assert_eq!(list["result"]["tools"][0]["name"], "t");
        let call =
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "t", "arguments": { "q": 1 } } });
        assert_eq!(
            dispatch(&call, &tools, &echo()).await.unwrap()["result"]["content"][0]["text"],
            "t:{\"q\":1}"
        );
        let fail = json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "fail" } });
        assert_eq!(dispatch(&fail, &tools, &echo()).await.unwrap()["result"]["isError"], true);
        // Sonde propre à Claude Code : erreur JSON-RPC, pas de plantage.
        let probe = json!({ "jsonrpc": "2.0", "id": "server-discover-probe-1", "method": "server/discover" });
        assert_eq!(dispatch(&probe, &tools, &echo()).await.unwrap()["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn cle_exigee() {
        let server = start("secret".into(), vec![], echo()).await.unwrap();
        let addr = server.url.trim_start_matches("http://").trim_end_matches("/mcp").to_owned();
        let send = |auth: &'static str| {
            let addr = addr.clone();
            async move {
                let mut s = TcpStream::connect(addr).await.unwrap();
                let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
                let req = format!(
                    "POST /mcp HTTP/1.1\r\nAuthorization: {auth}\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                s.write_all(req.as_bytes()).await.unwrap();
                let mut out = String::new();
                s.read_to_string(&mut out).await.unwrap();
                out
            }
        };
        assert!(send("Bearer faux").await.starts_with("HTTP/1.1 401"));
        let ok = send("Bearer secret").await;
        assert!(ok.starts_with("HTTP/1.1 200") && ok.contains(r#""result":{}"#), "{ok}");
    }

    /// Essai réel avec Claude Code (réseau et abonnement requis) :
    /// `LUNAS_CLAUDE=~/.local/bin/claude cargo test -p lunas-pdf mcp_avec_claude_code -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn mcp_avec_claude_code() {
        let Ok(claude) = std::env::var("LUNAS_CLAUDE") else { return };
        let tools = vec![json!({
            "name": "count_pages",
            "description": "Number of pages of the open document.",
            "inputSchema": { "type": "object", "properties": {} },
        })];
        let call: ToolFn = Arc::new(|_, _| Box::pin(async move { Ok(vec![text("42")]) }));
        let server = start("cle-test".into(), tools, call).await.unwrap();
        let mut args = crate::ai::agent::args(crate::ai::agent::Agent::Claude, Some("sonnet"), "You are a PDF assistant.");
        args.extend(crate::ai::agent::chat_args(&server.url, "cle-test"));
        let mut child = tokio::process::Command::new(claude)
            .args(&args)
            .current_dir(std::env::temp_dir())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(b"How many pages does the document have? Use the tool.")
            .await
            .unwrap();
        drop(stdin);
        let out = child.wait_with_output().await.unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let chunks: Vec<_> = text
            .lines()
            .filter_map(|l| crate::ai::agent::parse_line(crate::ai::agent::Agent::Claude, l))
            .collect();
        println!("{chunks:#?}");
        assert!(
            chunks
                .iter()
                .any(|c| matches!(c, crate::ai::agent::Chunk::Tool(n, _) if n == "mcp__lunas__count_pages"))
        );
        assert!(
            chunks
                .iter()
                .any(|c| matches!(c, crate::ai::agent::Chunk::Done(t) if t.contains("42")))
        );
    }
}
