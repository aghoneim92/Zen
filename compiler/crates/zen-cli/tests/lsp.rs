use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
struct Server {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Value>,
}
impl Server {
    fn new() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_zen"))
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let (tx, output) = mpsc::channel();
        std::thread::spawn(move || {
            loop {
                let mut len = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 {
                        return;
                    }
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(s) = line.strip_prefix("Content-Length:") {
                        len = s.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; len];
                reader.read_exact(&mut body).unwrap();
                if tx.send(serde_json::from_slice(&body).unwrap()).is_err() {
                    return;
                }
            }
        });
        Self {
            child,
            input,
            output,
        }
    }
    fn send(&mut self, v: Value) {
        let body = v.to_string();
        write!(self.input, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.input.flush().unwrap();
    }
    fn response(&self, id: u64) -> Value {
        loop {
            let v = self
                .output
                .recv_timeout(Duration::from_secs(20))
                .expect("LSP response timeout");
            if v["id"] == id {
                return v;
            }
        }
    }
    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let r = self.response(id);
        assert!(r.get("error").is_none(), "{r}");
        r["result"].clone()
    }
    fn diagnostics(&self, version: i32) -> Value {
        loop {
            let v = self
                .output
                .recv_timeout(Duration::from_secs(20))
                .expect("diagnostics timeout");
            if v["method"] == "textDocument/publishDiagnostics" && v["params"]["version"] == version
            {
                return v["params"]["diagnostics"].clone();
            }
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[test]
fn stdio_lifecycle_and_semantics() {
    let dir = std::env::temp_dir().join(format!("zen-lsp-protocol-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let uri = format!("file://{}/main.zen", dir.display());
    let mut server = Server::new();
    let caps = server.request(
        1,
        "initialize",
        json!({"capabilities":{},"rootUri":format!("file://{}",dir.display())}),
    );
    assert_eq!(caps["capabilities"]["positionEncoding"], "utf-16");
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let text = "struct User { name: String; }\nfn f(user: User) -> Unit { let label = \"😀\"; let name = user.name; }";
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"zen","version":1,"text":text}}}));
    assert_eq!(server.diagnostics(1), json!([]));
    let position = |line, character| json!({"textDocument":{"uri":uri},"position":{"line":line,"character":character}});
    let hover = server.request(2, "textDocument/hover", position(1, 50));
    assert!(hover.to_string().contains("String"), "{hover}");
    let definition = server.request(3, "textDocument/definition", position(1, 62));
    assert_eq!(definition["range"]["start"]["line"], 0, "{definition}");
    let tokens = server.request(
        4,
        "textDocument/semanticTokens/full",
        json!({"textDocument":{"uri":uri}}),
    );
    assert!(!tokens["data"].as_array().unwrap().is_empty());
    let symbols = server.request(
        5,
        "textDocument/documentSymbol",
        json!({"textDocument":{"uri":uri}}),
    );
    assert_eq!(symbols[0]["children"][0]["name"], "name");
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"fn f() -> Unit { let value: Float = 10; }"}]}}));
    let d = server.diagnostics(2);
    assert!(
        d.as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"].as_str().unwrap().starts_with("ZEN-TYPE")),
        "{d}"
    );
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":3},"contentChanges":[{"range":{"start":{"line":0,"character":28},"end":{"line":0,"character":33}},"text":"Int"}]}}));
    assert_eq!(server.diagnostics(3), json!([]));
    assert!(server.request(6, "shutdown", Value::Null).is_null());
    server.send(json!({"jsonrpc":"2.0","method":"exit"}));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn formatting_uses_unsaved_unicode_buffer() {
    let dir = std::env::temp_dir().join(format!("zen-format-lsp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("format.zen");
    std::fs::write(&path, "fn disk() -> Unit {}\n").unwrap();
    let uri = format!("file://{}", path.display());
    let mut server = Server::new();
    let caps = server.request(1, "initialize", json!({"capabilities":{}}));
    assert_eq!(caps["capabilities"]["documentFormattingProvider"], true);
    assert!(caps["capabilities"]["documentRangeFormattingProvider"].is_null());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let text = "fn unsaved()->Unit{let s=\"مرحبا 😀\";} // 😀";
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"zen","version":1,"text":text}}}));
    server.diagnostics(1);
    let params = json!({"textDocument":{"uri":uri},"options":{"tabSize":8,"insertSpaces":false}});
    let edits = server.request(2, "textDocument/formatting", params.clone());
    assert_eq!(edits.as_array().unwrap().len(), 1);
    assert_eq!(
        edits[0]["range"]["end"]["character"],
        text.encode_utf16().count()
    );
    let formatted = edits[0]["newText"].as_str().unwrap();
    assert_eq!(
        formatted,
        "fn unsaved() -> Unit {\n    let s = \"مرحبا 😀\";\n} // 😀\n"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "fn disk() -> Unit {}\n"
    );
    for (version, source) in [(2, formatted), (3, "fn broken {")] {
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":version},"contentChanges":[{"text":source}]}}));
        let diagnostics = server.diagnostics(version);
        let result = server.request(
            version as u64 + 2,
            "textDocument/formatting",
            params.clone(),
        );
        if version == 2 {
            assert_eq!(result, json!([]));
        } else {
            assert!(result.is_null());
            assert!(!diagnostics.as_array().unwrap().is_empty());
        }
    }
    server.request(9, "shutdown", json!(null));
    std::fs::remove_dir_all(dir).unwrap();
}
