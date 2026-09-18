use crate::{position, workspace::Snapshot};
use std::collections::BTreeMap;
use tower_lsp_server::ls_types::*;
use zen_diagnostics::Span;
use zen_semantics::{
    analysis::{Declaration, Entity, Kind, contains},
    types::Type,
};
pub fn hover(s: &Snapshot, p: &TextDocumentPositionParams) -> Option<Hover> {
    let (f, o) = s.at(p)?;
    let d = s
        .program
        .symbol_at(f, o)
        .and_then(|e| s.program.declaration(&e));
    let text = if let Some(d) = d {
        if !d.detail.is_empty() {
            d.detail.clone()
        } else {
            let ty = if contains(d.selection, f, o) {
                &d.ty
            } else {
                s.program.type_at(f, o).unwrap_or(&d.ty)
            };
            let name = d
                .parent
                .as_ref()
                .and_then(|e| s.program.declaration(e))
                .map_or_else(
                    || d.name.clone(),
                    |parent| format!("{}.{}", parent.name, d.name),
                );
            format!("{}: {}", name, s.program.display_type(ty))
        }
    } else {
        s.program.display_type(s.program.type_at(f, o)?)
    };
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```zen\n{text}\n```"),
        }),
        range: d
            .and_then(|d| s.location(d.selection))
            .filter(|l| l.uri == p.text_document.uri)
            .map(|l| l.range),
    })
}
pub fn definition(s: &Snapshot, p: &TextDocumentPositionParams, ty: bool) -> Option<Location> {
    let (f, o) = s.at(p)?;
    let entity = if ty {
        let t = s
            .program
            .symbol_at(f, o)
            .and_then(|e| s.program.declaration(&e))
            .map(|d| {
                if contains(d.selection, f, o) {
                    &d.ty
                } else {
                    s.program.type_at(f, o).unwrap_or(&d.ty)
                }
            })
            .or_else(|| s.program.type_at(f, o))?;
        if let Type::Nominal(id, _) = t {
            Entity::Nominal(*id)
        } else {
            return None;
        }
    } else {
        s.program.symbol_at(f, o)?
    };
    s.location(s.program.declaration(&entity)?.selection)
}
pub fn references(s: &Snapshot, p: &TextDocumentPositionParams, decl: bool) -> Vec<Location> {
    let Some((f, o)) = s.at(p) else { return vec![] };
    s.program
        .symbol_at(f, o)
        .map(|e| {
            s.program
                .references(&e, decl)
                .into_iter()
                .filter_map(|sp| s.location(sp))
                .collect()
        })
        .unwrap_or_default()
}
fn kind(k: Kind) -> SymbolKind {
    match k {
        Kind::Variable => SymbolKind::VARIABLE,
        Kind::Parameter => SymbolKind::VARIABLE,
        Kind::Function => SymbolKind::FUNCTION,
        Kind::Method => SymbolKind::METHOD,
        Kind::Constant => SymbolKind::CONSTANT,
        Kind::Struct => SymbolKind::STRUCT,
        Kind::Enum => SymbolKind::ENUM,
        Kind::Interface => SymbolKind::INTERFACE,
        Kind::Field => SymbolKind::FIELD,
        Kind::Variant => SymbolKind::ENUM_MEMBER,
        Kind::TypeParameter => SymbolKind::TYPE_PARAMETER,
    }
}
#[allow(deprecated)]
fn document_symbol(s: &Snapshot, d: &Declaration) -> DocumentSymbol {
    DocumentSymbol {
        name: d.name.clone(),
        detail: Some(if d.detail.is_empty() {
            s.program.display_type(&d.ty)
        } else {
            d.detail.clone()
        }),
        kind: kind(d.kind),
        tags: None,
        deprecated: None,
        range: position::range(s.text(d.range.file), d.range.start, d.range.end),
        selection_range: position::range(
            s.text(d.selection.file),
            d.selection.start,
            d.selection.end,
        ),
        children: Some(
            s.program
                .ide
                .declarations
                .iter()
                .filter(|c| c.parent.as_ref() == Some(&d.entity))
                .map(|c| document_symbol(s, c))
                .collect(),
        ),
    }
}
pub fn symbols(s: &Snapshot, u: &Uri) -> Vec<DocumentSymbol> {
    let Some(f) = s.file(u) else { return vec![] };
    s.program
        .ide
        .declarations
        .iter()
        .filter(|d| {
            d.selection.file == f
                && d.parent.is_none()
                && !matches!(
                    d.kind,
                    Kind::Variable | Kind::Parameter | Kind::TypeParameter
                )
        })
        .map(|d| document_symbol(s, d))
        .collect()
}
#[allow(deprecated)]
pub fn workspace_symbols(s: &Snapshot, q: &str) -> Vec<SymbolInformation> {
    s.program
        .ide
        .declarations
        .iter()
        .filter(|d| {
            !matches!(
                d.kind,
                Kind::Variable
                    | Kind::Parameter
                    | Kind::TypeParameter
                    | Kind::Field
                    | Kind::Variant
            ) && d.name.to_lowercase().contains(&q.to_lowercase())
        })
        .filter_map(|d| {
            Some(SymbolInformation {
                name: d.name.clone(),
                kind: kind(d.kind),
                tags: None,
                deprecated: None,
                location: s.location(d.selection)?,
                container_name: None,
            })
        })
        .collect()
}
pub fn edit(s: &Snapshot, edits: Vec<(Span, String)>) -> WorkspaceEdit {
    let mut groups: BTreeMap<String, Vec<TextEdit>> = BTreeMap::new();
    for (sp, new_text) in edits {
        if let Some(l) = s.location(sp) {
            groups
                .entry(l.uri.as_str().into())
                .or_default()
                .push(TextEdit {
                    range: l.range,
                    new_text,
                });
        }
    }
    WorkspaceEdit {
        document_changes: Some(DocumentChanges::Edits(
            groups
                .into_iter()
                .filter_map(|(u, edits)| {
                    Some(TextDocumentEdit {
                        text_document: OptionalVersionedTextDocumentIdentifier {
                            uri: u.parse().ok()?,
                            version: s.versions.get(&u).copied(),
                        },
                        edits: edits.into_iter().map(OneOf::Left).collect(),
                    })
                })
                .collect(),
        )),
        ..Default::default()
    }
}
pub fn completion(s: &Snapshot, p: &TextDocumentPositionParams) -> Vec<CompletionItem> {
    let Some((f, o)) = s.at(p) else { return vec![] };
    s.database
        .complete(f, o)
        .into_iter()
        .enumerate()
        .map(|(i, c)| CompletionItem {
            label: c.name.clone(),
            kind: Some(match c.entity {
                Entity::Nominal(_) | Entity::Generic(_) | Entity::Builtin(_) => {
                    CompletionItemKind::CLASS
                }
                Entity::Member(_, _) => CompletionItemKind::FIELD,
                Entity::Value(_) => {
                    if matches!(c.ty, Type::Function(..)) {
                        CompletionItemKind::FUNCTION
                    } else {
                        CompletionItemKind::VARIABLE
                    }
                }
            }),
            detail: Some(s.program.display_type(&c.ty)),
            sort_text: Some(format!("{i:05}")),
            filter_text: Some(c.name.clone()),
            insert_text: Some(c.name),
            ..Default::default()
        })
        .collect()
}
pub fn signature(s: &Snapshot, p: &TextDocumentPositionParams) -> Option<SignatureHelp> {
    let (f, o) = s.at(p)?;
    let call = s
        .program
        .ide
        .calls
        .iter()
        .filter(|c| contains(c.span, f, o) && o >= c.callee.end)
        .min_by_key(|c| c.span.end - c.span.start)?;
    let mut active = call
        .arguments
        .iter()
        .position(|(sp, _)| o <= sp.end)
        .unwrap_or(call.arguments.len());
    if let Some((_, Some(label))) = call.arguments.get(active) {
        active = call
            .parameters
            .iter()
            .position(|(n, _)| n == label)
            .unwrap_or(active);
    }
    let d = s.program.declaration(&Entity::Value(call.symbol))?;
    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label: d.detail.clone(),
            documentation: None,
            parameters: Some(
                call.parameters
                    .iter()
                    .map(|(n, t)| ParameterInformation {
                        label: ParameterLabel::Simple(format!(
                            "{}: {}",
                            n,
                            s.program.display_type(t)
                        )),
                        documentation: None,
                    })
                    .collect(),
            ),
            active_parameter: Some(active.min(call.parameters.len().saturating_sub(1)) as u32),
        }],
        active_signature: Some(0),
        active_parameter: Some(active.min(call.parameters.len().saturating_sub(1)) as u32),
    })
}
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::METHOD,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::STRUCT,
            SemanticTokenType::ENUM,
            SemanticTokenType::INTERFACE,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::ENUM_MEMBER,
            SemanticTokenType::TYPE_PARAMETER,
            SemanticTokenType::NAMESPACE,
            SemanticTokenType::TYPE,
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::READONLY,
            SemanticTokenModifier::DEFAULT_LIBRARY,
        ],
    }
}
fn token_kind(k: Kind) -> u32 {
    match k {
        Kind::Variable => 0,
        Kind::Parameter => 1,
        Kind::Function => 2,
        Kind::Method => 3,
        Kind::Constant => 4,
        Kind::Struct => 5,
        Kind::Enum => 6,
        Kind::Interface => 7,
        Kind::Field => 8,
        Kind::Variant => 9,
        Kind::TypeParameter => 10,
    }
}
pub fn tokens(s: &Snapshot, u: &Uri) -> SemanticTokens {
    let Some(f) = s.file(u) else {
        return SemanticTokens::default();
    };
    let mut entries: Vec<_> = s
        .program
        .ide
        .declarations
        .iter()
        .filter(|d| d.selection.file == f)
        .map(|d| {
            (
                d.selection,
                token_kind(d.kind),
                if matches!(d.kind, Kind::Constant | Kind::Field) {
                    3u32
                } else {
                    1u32
                },
            )
        })
        .chain(
            s.program
                .ide
                .references
                .iter()
                .filter(|(sp, _)| sp.file == f)
                .filter_map(|(sp, e)| {
                    s.program
                        .declaration(e)
                        .map(|d| (*sp, token_kind(d.kind), 0))
                }),
        )
        .collect();
    entries.extend(
        s.program
            .ide
            .namespaces
            .iter()
            .filter(|sp| sp.file == f)
            .map(|sp| (*sp, 11, 0)),
    );
    entries.extend(
        s.program
            .ide
            .builtin_types
            .iter()
            .filter(|sp| sp.file == f)
            .map(|sp| (*sp, 12, 4)),
    );
    for (sp, entity) in &s.program.ide.references {
        if sp.file == f && s.program.declaration(entity).is_none() {
            match entity {
                Entity::Nominal(_) => entries.push((*sp, 12, 4)),
                Entity::Member(_, _) => entries.push((*sp, 9, 4)),
                _ => {}
            }
        }
    }
    entries.sort_by_key(|(s, _, _)| (s.start, s.end));
    entries.dedup_by_key(|(s, _, _)| (s.start, s.end));
    let mut previous = Position::default();
    let mut data = vec![];
    for (sp, token_type, token_modifiers_bitset) in entries {
        let r = position::range(s.text(f), sp.start, sp.end);
        if r.start.line != r.end.line || r.start == r.end {
            continue;
        }
        data.push(SemanticToken {
            delta_line: r.start.line - previous.line,
            delta_start: if r.start.line == previous.line {
                r.start.character - previous.character
            } else {
                r.start.character
            },
            length: r.end.character - r.start.character,
            token_type,
            token_modifiers_bitset,
        });
        previous = r.start;
    }
    SemanticTokens {
        result_id: None,
        data,
    }
}
pub fn hints(s: &Snapshot, p: &InlayHintParams) -> Vec<InlayHint> {
    let Some(f) = s.file(&p.text_document.uri) else {
        return vec![];
    };
    s.program
        .ide
        .inferred
        .iter()
        .filter(|(sp, t)| sp.file == f && *t != Type::Error)
        .filter_map(|(sp, t)| {
            let pos = position::position(s.text(f), sp.end);
            if pos < p.range.start || pos > p.range.end {
                return None;
            }
            Some(InlayHint {
                position: pos,
                label: InlayHintLabel::String(format!(": {}", s.program.display_type(t))),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: Some(false),
                padding_right: Some(false),
                data: None,
            })
        })
        .collect()
}
pub fn actions(s: &Snapshot, p: &CodeActionParams) -> CodeActionResponse {
    let Some(f) = s.file(&p.text_document.uri) else {
        return vec![];
    };
    s.program
        .diagnostics
        .iter()
        .filter(|d| {
            d.primary.file == f
                && s.location(d.primary)
                    .is_some_and(|l| l.range.end >= p.range.start && l.range.start <= p.range.end)
        })
        .flat_map(|d| d.fixes.iter())
        .filter_map(|fix| {
            let (sp, text) = fix.replacement.as_ref()?;

            Some(CodeActionOrCommand::CodeAction(CodeAction {
                title: fix.message.clone(),
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(edit(s, vec![(*sp, text.clone())])),
                ..Default::default()
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zen_semantics::analysis::Database;
    fn snapshot(text: &str) -> Snapshot {
        let mut database = Database::default();
        database.set_file("file:///test.zen".into(), "test".into(), text.into());
        let program = database.analyze();
        Snapshot {
            database,
            program,
            versions: BTreeMap::new(),
        }
    }
    fn at(text: &str, needle: &str) -> TextDocumentPositionParams {
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///test.zen".parse().unwrap(),
            },
            position: position::position(text, text.find(needle).unwrap()),
        }
    }
    #[test]
    fn signatures_use_named_parameter_order() {
        let text = "native fn request(url: String, timeout: Int) -> Unit; fn f() -> Unit { request(timeout: 10, url: \"x\"); }";
        let s = snapshot(text);
        let help = signature(&s, &at(text, "10,")).unwrap();
        assert_eq!(help.active_parameter, Some(1));
        assert!(help.signatures[0].label.contains("timeout: Int"));
        let help = signature(&s, &at(text, "\"x\"")).unwrap();
        assert_eq!(help.active_parameter, Some(0));
    }
    #[test]
    fn semantic_tokens_follow_resolution() {
        let text =
            "struct item { field: Int; } fn act(value: item) -> Unit { let item = value.field; }";
        let s = snapshot(text);
        let uri = "file:///test.zen".parse().unwrap();
        let t = tokens(&s, &uri);
        let types = t.data.iter().map(|t| t.token_type).collect::<Vec<_>>();
        assert!(types.contains(&5));
        assert!(types.contains(&8));
        assert!(types.contains(&2));
        assert!(types.contains(&1));
        assert!(types.contains(&0));
    }
    #[test]
    fn code_action_at_diagnostic_start() {
        let text = "enum E { a; b; } fn f(e: E) -> Unit { match e { .a => unit; } }";
        let s = snapshot(text);
        let position = at(text, "match");
        let actions = actions(
            &s,
            &CodeActionParams {
                text_document: position.text_document,
                range: Range::new(position.position, position.position),
                context: CodeActionContext {
                    diagnostics: vec![],
                    only: None,
                    trigger_kind: None,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        );
        assert_eq!(actions.len(), 1);
    }
}
