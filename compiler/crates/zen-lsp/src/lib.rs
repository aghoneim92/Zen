//! Editor-independent LSP transport. All semantic decisions belong to zen-semantics.
mod features;
mod position;
mod workspace;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use tower_lsp_server::{Client, LanguageServer, LspService, Server, jsonrpc::Result, ls_types::*};
use workspace::{Document, Snapshot};
#[derive(Default)]
struct State {
    open: BTreeMap<String, Document>,
    snapshots: BTreeMap<PathBuf, Arc<Snapshot>>,
    generation: BTreeMap<PathBuf, u64>,
    roots: Vec<PathBuf>,
    watch_files: bool,
}
struct Backend {
    client: Client,
    state: Mutex<State>,
}
impl Backend {
    async fn snapshot(&self, u: &Uri) -> Option<Arc<Snapshot>> {
        let root = workspace::root(&workspace::path(u)?);
        {
            let state = self.state.lock().await;
            let snapshot = state.snapshots.get(&root)?;
            if state.open.iter().any(|(u, d)| {
                workspace::path(&u.parse().unwrap()).is_some_and(|p| workspace::root(&p) == root)
                    && snapshot.versions.get(u) != Some(&d.version)
            }) {
                return None;
            }
            Some(snapshot.clone())
        }
    }
    async fn refresh(&self, u: &Uri) {
        let Some(path) = workspace::path(u) else {
            return;
        };
        let root = workspace::root(&path);
        let (generation, open, previous) = {
            let mut s = self.state.lock().await;
            let generation = s.generation.entry(root.clone()).or_default();
            *generation += 1;
            (*generation, s.open.clone(), s.snapshots.get(&root).cloned())
        };
        let work_root = root.clone();
        let result =
            tokio::task::spawn_blocking(move || workspace::load(work_root, open, previous)).await;
        let Ok(snapshot) = result else {
            self.client
                .log_message(MessageType::ERROR, "Zen analysis worker failed")
                .await;
            return;
        };
        let mut state = self.state.lock().await;
        if state.generation.get(&root) != Some(&generation) {
            return;
        }
        let snapshot = Arc::new(snapshot);
        let old = state.snapshots.insert(root, snapshot.clone());
        if let Some(old) = old {
            for f in old.database.files.iter().flatten() {
                if !snapshot
                    .database
                    .files
                    .iter()
                    .flatten()
                    .any(|new| new.path == f.path)
                    && let Ok(uri) = f.path.parse()
                {
                    self.client.publish_diagnostics(uri, vec![], None).await;
                }
            }
        }
        // Keep the generation guard through publication; analysis itself never holds it.
        for (i, f) in snapshot.database.files.iter().enumerate() {
            if let Some(f) = f
                && let Ok(uri) = f.path.parse()
            {
                self.client
                    .publish_diagnostics(
                        uri,
                        snapshot.diagnostics(zen_diagnostics::FileId(i)),
                        snapshot.versions.get(&f.path).copied(),
                    )
                    .await;
            }
        }
    }
}
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.state.lock().await.watch_files = params
            .capabilities
            .workspace
            .as_ref()
            .and_then(|w| w.did_change_watched_files.as_ref())
            .and_then(|w| w.dynamic_registration)
            .unwrap_or(false);
        let mut roots: Vec<_> = params
            .workspace_folders
            .unwrap_or_default()
            .into_iter()
            .filter_map(|w| workspace::path(&w.uri))
            .collect();
        #[allow(deprecated)]
        if roots.is_empty()
            && let Some(uri) = params.root_uri
            && let Some(path) = workspace::path(&uri)
        {
            roots.push(path);
        }
        self.state.lock().await.roots = roots;
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(PositionEncodingKind::UTF16),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                document_formatting_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".into(), ":".into()]),
                    ..Default::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into(), ":".into()]),
                    ..Default::default()
                }),
                inlay_hint_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: features::legend(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "zen".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            ..Default::default()
        })
    }
    async fn initialized(&self, _: InitializedParams) {
        if self.state.lock().await.watch_files {
            let options = DidChangeWatchedFilesRegistrationOptions {
                watchers: vec![FileSystemWatcher {
                    glob_pattern: GlobPattern::String("**/*.zen".into()),
                    kind: None,
                }],
            };
            if let Err(error) = self
                .client
                .register_capability(vec![Registration {
                    id: "zen-source-files".into(),
                    method: "workspace/didChangeWatchedFiles".into(),
                    register_options: serde_json::to_value(options).ok(),
                }])
                .await
            {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("Could not register Zen file watcher: {error}"),
                    )
                    .await;
            }
        }
        let roots = self.state.lock().await.roots.clone();
        for root in roots {
            let source = if root.join("src").is_dir() {
                root.join("src")
            } else {
                root
            };
            // A root with no direct Zen source is a repository container, not
            // a package. Open documents discover their own compiler source roots.
            if std::fs::read_dir(&source).is_ok_and(|mut es| {
                es.any(|e| e.is_ok_and(|e| e.path().extension().is_some_and(|x| x == "zen")))
            }) && let Some(uri) = workspace::uri(&source.join("__workspace__.zen"))
            {
                self.refresh(&uri).await;
            }
        }

        self.client
            .log_message(MessageType::INFO, "Zen compiler language server ready")
            .await;
    }
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
    async fn did_open(&self, p: DidOpenTextDocumentParams) {
        let d = p.text_document;
        self.state.lock().await.open.insert(
            d.uri.as_str().into(),
            Document {
                text: d.text,
                version: d.version,
            },
        );
        self.refresh(&d.uri).await;
    }
    async fn did_change(&self, p: DidChangeTextDocumentParams) {
        let u = p.text_document.uri;
        {
            let mut s = self.state.lock().await;
            let Some(d) = s.open.get_mut(u.as_str()) else {
                return;
            };
            if p.text_document.version <= d.version {
                return;
            }
            let mut text = d.text.clone();
            for c in p.content_changes {
                if let Some(r) = c.range {
                    let (Some(a), Some(b)) = (
                        position::offset(&text, r.start),
                        position::offset(&text, r.end),
                    ) else {
                        return;
                    };
                    if a > b {
                        return;
                    }
                    text.replace_range(a..b, &c.text);
                } else {
                    text = c.text;
                }
            }
            d.text = text;
            d.version = p.text_document.version;
        }
        self.refresh(&u).await;
    }
    async fn did_close(&self, p: DidCloseTextDocumentParams) {
        self.state
            .lock()
            .await
            .open
            .remove(p.text_document.uri.as_str());
        self.refresh(&p.text_document.uri).await;
        self.client
            .publish_diagnostics(p.text_document.uri, vec![], None)
            .await;
    }
    async fn did_change_watched_files(&self, p: DidChangeWatchedFilesParams) {
        let mut roots = std::collections::BTreeSet::new();
        for change in p.changes {
            if let Some(path) = workspace::path(&change.uri) {
                roots.insert(workspace::root(&path));
            }
        }
        for root in roots {
            if let Some(uri) = workspace::uri(&root.join("__workspace__.zen")) {
                self.refresh(&uri).await;
            }
        }
    }
    async fn did_save(&self, p: DidSaveTextDocumentParams) {
        self.refresh(&p.text_document.uri).await;
    }
    async fn hover(&self, p: HoverParams) -> Result<Option<Hover>> {
        Ok(self
            .snapshot(&p.text_document_position_params.text_document.uri)
            .await
            .and_then(|s| features::hover(&s, &p.text_document_position_params)))
    }
    async fn goto_definition(
        &self,
        p: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        Ok(self
            .snapshot(&p.text_document_position_params.text_document.uri)
            .await
            .and_then(|s| features::definition(&s, &p.text_document_position_params, false))
            .map(GotoDefinitionResponse::Scalar))
    }
    async fn goto_type_definition(
        &self,
        p: request::GotoTypeDefinitionParams,
    ) -> Result<Option<request::GotoTypeDefinitionResponse>> {
        Ok(self
            .snapshot(&p.text_document_position_params.text_document.uri)
            .await
            .and_then(|s| features::definition(&s, &p.text_document_position_params, true))
            .map(GotoDefinitionResponse::Scalar))
    }
    async fn references(&self, p: ReferenceParams) -> Result<Option<Vec<Location>>> {
        Ok(self
            .snapshot(&p.text_document_position.text_document.uri)
            .await
            .map(|s| {
                features::references(&s, &p.text_document_position, p.context.include_declaration)
            }))
    }
    async fn document_highlight(
        &self,
        p: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let u = &p.text_document_position_params.text_document.uri;
        Ok(self.snapshot(u).await.map(|s| {
            features::references(&s, &p.text_document_position_params, true)
                .into_iter()
                .filter(|l| &l.uri == u)
                .map(|l| DocumentHighlight {
                    range: l.range,
                    kind: Some(
                        if s.program
                            .ide
                            .writes
                            .iter()
                            .any(|sp| s.location(*sp).is_some_and(|x| x == l))
                        {
                            DocumentHighlightKind::WRITE
                        } else {
                            DocumentHighlightKind::READ
                        },
                    ),
                })
                .collect()
        }))
    }
    async fn document_symbol(
        &self,
        p: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        Ok(self
            .snapshot(&p.text_document.uri)
            .await
            .map(|s| DocumentSymbolResponse::Nested(features::symbols(&s, &p.text_document.uri))))
    }
    async fn symbol(&self, p: WorkspaceSymbolParams) -> Result<Option<WorkspaceSymbolResponse>> {
        let s = self.state.lock().await;
        Ok(Some(WorkspaceSymbolResponse::Flat(
            s.snapshots
                .values()
                .flat_map(|s| features::workspace_symbols(s, &p.query))
                .collect(),
        )))
    }
    async fn goto_implementation(
        &self,
        p: request::GotoImplementationParams,
    ) -> Result<Option<request::GotoImplementationResponse>> {
        Ok(self
            .snapshot(&p.text_document_position_params.text_document.uri)
            .await
            .map(|s| {
                let locations = s
                    .at(&p.text_document_position_params)
                    .and_then(|(f, o)| s.program.symbol_at(f, o))
                    .map(|e| {
                        s.program
                            .ide
                            .implementations
                            .iter()
                            .filter(|(x, _)| x == &e)
                            .filter_map(|(_, sp)| s.location(*sp))
                            .collect()
                    })
                    .unwrap_or_default();
                GotoDefinitionResponse::Array(locations)
            }))
    }
    async fn prepare_rename(
        &self,
        p: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        Ok(self.snapshot(&p.text_document.uri).await.and_then(|s| {
            let (f, o) = s.at(&p)?;
            let e = s.program.symbol_at(f, o)?;
            let d = s.program.declaration(&e)?;
            if d.name == "self" {
                return None;
            }
            let span = s
                .program
                .references(&e, true)
                .into_iter()
                .find(|sp| zen_semantics::analysis::contains(*sp, f, o))?;
            Some(PrepareRenameResponse::RangeWithPlaceholder {
                range: s.location(span)?.range,
                placeholder: d.name.clone(),
            })
        }))
    }
    async fn rename(&self, p: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let Some(s) = self
            .snapshot(&p.text_document_position.text_document.uri)
            .await
        else {
            return Ok(None);
        };
        let Some((f, o)) = s.at(&p.text_document_position) else {
            return Ok(None);
        };
        let Some(e) = s.program.symbol_at(f, o) else {
            return Ok(None);
        };
        let db = s.database.clone();
        let name = p.new_name.clone();
        let snapshot = s.clone();
        let spans = tokio::task::spawn_blocking(move || db.rename(&snapshot.program, &e, &name))
            .await
            .map_err(|_| tower_lsp_server::jsonrpc::Error::internal_error())?
            .map_err(tower_lsp_server::jsonrpc::Error::invalid_params)?;
        let state = self.state.lock().await;
        if s.versions
            .iter()
            .any(|(u, v)| state.open.get(u).is_some_and(|d| d.version != *v))
        {
            return Err(tower_lsp_server::jsonrpc::Error::invalid_params(
                "Document changed during rename",
            ));
        }
        Ok(Some(features::edit(
            &s,
            spans
                .into_iter()
                .map(|sp| (sp, p.new_name.clone()))
                .collect(),
        )))
    }
    async fn completion(&self, p: CompletionParams) -> Result<Option<CompletionResponse>> {
        let Some(s) = self
            .snapshot(&p.text_document_position.text_document.uri)
            .await
        else {
            return Ok(None);
        };
        Ok(
            tokio::task::spawn_blocking(move || {
                features::completion(&s, &p.text_document_position)
            })
            .await
            .ok()
            .map(CompletionResponse::Array),
        )
    }
    async fn signature_help(&self, p: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        Ok(self
            .snapshot(&p.text_document_position_params.text_document.uri)
            .await
            .and_then(|s| features::signature(&s, &p.text_document_position_params)))
    }
    async fn semantic_tokens_full(
        &self,
        p: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        Ok(self
            .snapshot(&p.text_document.uri)
            .await
            .map(|s| SemanticTokensResult::Tokens(features::tokens(&s, &p.text_document.uri))))
    }
    async fn inlay_hint(&self, p: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        Ok(self
            .snapshot(&p.text_document.uri)
            .await
            .map(|s| features::hints(&s, &p)))
    }
    async fn code_action(&self, p: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        Ok(self
            .snapshot(&p.text_document.uri)
            .await
            .map(|s| features::actions(&s, &p)))
    }
    async fn formatting(&self, p: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = p.text_document.uri;
        let document = self.state.lock().await.open.get(uri.as_str()).cloned();
        let Some(document) = document else {
            return Ok(None);
        };
        let source = document.text.clone();
        let formatted = tokio::task::spawn_blocking(move || zen_format::format_source(&source))
            .await
            .map_err(|_| tower_lsp_server::jsonrpc::Error::internal_error())?;
        let state = self.state.lock().await;
        if !state
            .open
            .get(uri.as_str())
            .is_some_and(|d| d.version == document.version && d.text == document.text)
        {
            return Ok(None);
        }
        match formatted {
            Ok(text) if text != document.text => Ok(Some(vec![TextEdit {
                range: position::range(&document.text, 0, document.text.len()),
                new_text: text,
            }])),
            Ok(_) => Ok(Some(vec![])),
            Err(zen_format::FormatError::Syntax(_)) => Ok(None),
            Err(zen_format::FormatError::Internal(_)) => {
                Err(tower_lsp_server::jsonrpc::Error::internal_error())
            }
        }
    }
}
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
    tokio::runtime::Runtime::new()
        .expect("Tokio runtime")
        .block_on(async {
            let (service, socket) = LspService::new(|client| Backend {
                client,
                state: Mutex::new(State::default()),
            });
            Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
                .serve(service)
                .await;
        });
}
