use dashmap::DashMap;
use dream::driver::diagnostics::DiagnosticBag;
use dream::syntax::lexer::Lexer;
use dream::syntax::token::token_kind::TokenKind;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

mod analysis;
mod format;
mod index;
mod position;

fn map_position(pos: position::Position) -> Position {
    Position {
        line: pos.line,
        character: pos.character,
    }
}

fn map_range(range: position::Range) -> Range {
    Range {
        start: map_position(range.start),
        end: map_position(range.end),
    }
}

fn map_sym_kind_to_index(kind: index::SymKind) -> u32 {
    match kind {
        index::SymKind::Function => 3,
        index::SymKind::Struct => 5,
        index::SymKind::Enum => 6,
        index::SymKind::EnumMember => 7,
        index::SymKind::Field => 2,
        index::SymKind::Method => 4,
        index::SymKind::Variable => 1,
        index::SymKind::Param => 8,
        index::SymKind::Type => 9,
        index::SymKind::Keyword => 0,
    }
}

fn get_semantic_tokens(file_path: Option<&str>, text: &str) -> Vec<SemanticToken> {
    let mut scratch = DiagnosticBag::new(None);
    let mut lexer = Lexer::new(text.to_string());
    let tokens = lexer.lex_all(&mut scratch);
    let idx = index::Index::build(file_path, text);
    let line_index = position::LineIndex::new(text);

    let mut semantic_tokens = Vec::new();

    for token in tokens {
        if token.kind != TokenKind::EndOfFileToken && token.kind != TokenKind::BadToken {
            let token_type_index = match token.kind {
                TokenKind::IdentifierToken => {
                    let mut kind = 1; // Default to variable
                    if token.text == "this" {
                        kind = 0; // keyword
                    } else if let Some(decl) =
                        idx.decls.iter().find(|d| d.start == token.position.start)
                    {
                        kind = map_sym_kind_to_index(decl.kind);
                    } else if let Some(r) =
                        idx.refs.iter().find(|r| r.start == token.position.start)
                    {
                        kind = map_sym_kind_to_index(r.kind);
                    }
                    Some(kind)
                }
                TokenKind::NumberToken => Some(12),
                TokenKind::BooleanToken | TokenKind::NullToken => Some(0),
                TokenKind::IfToken
                | TokenKind::ElseToken
                | TokenKind::ForToken
                | TokenKind::WhileToken
                | TokenKind::DoToken
                | TokenKind::ReturnToken
                | TokenKind::BreakToken
                | TokenKind::ContinueToken
                | TokenKind::LetToken
                | TokenKind::ConstToken
                | TokenKind::FunToken
                | TokenKind::StaticToken
                | TokenKind::ImportToken
                | TokenKind::ExportToken
                | TokenKind::ExternToken
                | TokenKind::ClassToken
                | TokenKind::ExtendToken
                | TokenKind::IsToken
                | TokenKind::InToken
                | TokenKind::EnumToken
                | TokenKind::TypeToken
                | TokenKind::SwitchToken
                | TokenKind::CaseToken
                | TokenKind::DefaultToken
                | TokenKind::AsyncToken
                | TokenKind::AwaitToken => Some(0),
                TokenKind::DataTypeToken => Some(9),
                TokenKind::PlusToken
                | TokenKind::MinusToken
                | TokenKind::SlashToken
                | TokenKind::StarToken
                | TokenKind::BangToken
                | TokenKind::ModulusToken
                | TokenKind::PlusEqualToken
                | TokenKind::MinusEqualToken
                | TokenKind::StarEqualToken
                | TokenKind::SlashEqualToken
                | TokenKind::ModulusEqualToken
                | TokenKind::PlusPlusToken
                | TokenKind::MinusMinusToken
                | TokenKind::EqualEqualToken
                | TokenKind::NotEqualToken
                | TokenKind::AmpersandAmpersandToken
                | TokenKind::PipePipeToken
                | TokenKind::BitWisePipeToken
                | TokenKind::BitWiseAmpersandToken
                | TokenKind::BitWiseXorToken
                | TokenKind::ShiftLeftToken
                | TokenKind::ShiftRightToken
                | TokenKind::QuestionQuestionToken
                | TokenKind::EqualToken
                | TokenKind::GreaterThanEqualToken
                | TokenKind::GreaterThanToken
                | TokenKind::SmallerThanToken
                | TokenKind::SmallerThanEqualToken => Some(10),
                _ => None,
            };

            if let Some(type_idx) = token_type_index {
                if !token.text.contains('\n') {
                    let start_pos = line_index.position(token.position.start);
                    semantic_tokens.push((
                        start_pos.line,
                        start_pos.character,
                        token.text.chars().count() as u32,
                        type_idx,
                    ));
                }
            }
        }
    }

    // Stable sort by line, then char to delta encode
    semantic_tokens.sort_by_key(|t| (t.0, t.1));

    let mut result = Vec::new();
    let mut pre_line = 0;
    let mut pre_char = 0;

    for (line, char, len, type_idx) in semantic_tokens {
        let delta_line = line - pre_line;
        let delta_start = if delta_line == 0 {
            char - pre_char
        } else {
            char
        };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length: len,
            token_type: type_idx,
            token_modifiers_bitset: 0,
        });

        pre_line = line;
        pre_char = char;
    }

    result
}

#[derive(Debug)]
struct Backend {
    client: Client,
    document_map: DashMap<String, String>,
}

impl Backend {
    async fn publish_diagnostics(&self, uri: Url, text: String, version: Option<i32>) {
        let file_path = uri
            .to_file_path()
            .ok()
            .map(|p| p.to_string_lossy().to_string());
        let diagnostics_out = analysis::collect_diagnostics(file_path.as_deref(), &text);
        let diagnostics: Vec<Diagnostic> = diagnostics_out
            .into_iter()
            .map(|d| Diagnostic {
                range: map_range(d.range),
                severity: match d.severity {
                    "error" => Some(DiagnosticSeverity::ERROR),
                    "warning" => Some(DiagnosticSeverity::WARNING),
                    _ => Some(DiagnosticSeverity::INFORMATION),
                },
                message: d.message,
                ..Default::default()
            })
            .collect();
        self.client
            .publish_diagnostics(uri, diagnostics, version)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "\"".to_string(),
                        "/".to_string(),
                    ]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::KEYWORD,     // 0
                                    SemanticTokenType::VARIABLE,    // 1
                                    SemanticTokenType::PROPERTY,    // 2
                                    SemanticTokenType::FUNCTION,    // 3
                                    SemanticTokenType::METHOD,      // 4
                                    SemanticTokenType::CLASS,       // 5
                                    SemanticTokenType::ENUM,        // 6
                                    SemanticTokenType::ENUM_MEMBER, // 7
                                    SemanticTokenType::PARAMETER,   // 8
                                    SemanticTokenType::TYPE,        // 9
                                    SemanticTokenType::OPERATOR,    // 10
                                    SemanticTokenType::STRING,      // 11
                                    SemanticTokenType::NUMBER,      // 12
                                    SemanticTokenType::COMMENT,     // 13
                                ],
                                token_modifiers: vec![],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Dream LSP initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();
        let version = params.text_document.version;
        self.document_map.insert(uri.to_string(), text.clone());
        self.publish_diagnostics(uri, text, Some(version)).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let version = params.text_document.version;
        if let Some(change) = params.content_changes.into_iter().last() {
            let text = change.text;
            self.document_map.insert(uri.to_string(), text.clone());
            self.publish_diagnostics(uri, text, Some(version)).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.document_map.remove(&uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let text = self.document_map.get(&uri).map(|v| v.value().clone());
        if let Some(text) = text {
            let file_path = params
                .text_document_position_params
                .text_document
                .uri
                .to_file_path()
                .ok()
                .map(|p| p.to_string_lossy().to_string());
            let line_index = position::LineIndex::new(&text);
            let offset = line_index.offset(
                params.text_document_position_params.position.line,
                params.text_document_position_params.position.character,
            );
            let idx = index::Index::build(file_path.as_deref(), &text);
            if let Some(located) = idx.hover(offset) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: located.contents,
                    }),
                    range: Some(Range {
                        start: map_position(line_index.position(located.start)),
                        end: map_position(line_index.position(located.end)),
                    }),
                }));
            }
        }
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri_str = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let text = self.document_map.get(&uri_str).map(|v| v.value().clone());
        if let Some(text) = text {
            let file_path = params
                .text_document_position_params
                .text_document
                .uri
                .to_file_path()
                .ok()
                .map(|p| p.to_string_lossy().to_string());
            let line_index = position::LineIndex::new(&text);
            let offset = line_index.offset(
                params.text_document_position_params.position.line,
                params.text_document_position_params.position.character,
            );
            let idx = index::Index::build(file_path.as_deref(), &text);
            if let Some((start, end)) = idx.definition(offset) {
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: params.text_document_position_params.text_document.uri,
                    range: Range {
                        start: map_position(line_index.position(start)),
                        end: map_position(line_index.position(end)),
                    },
                })));
            }
        }
        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let text = self.document_map.get(&uri).map(|v| v.value().clone());
        if let Some(text) = text {
            let file_path = params
                .text_document_position
                .text_document
                .uri
                .to_file_path()
                .ok()
                .map(|p| p.to_string_lossy().to_string());
            let line_index = position::LineIndex::new(&text);
            let offset = line_index.offset(
                params.text_document_position.position.line,
                params.text_document_position.position.character,
            );
            let idx = index::Index::build(file_path.as_deref(), &text);
            let completions = idx.completions(file_path.as_deref(), &text, offset);

            let items: Vec<CompletionItem> = completions
                .into_iter()
                .map(|(label, kind, detail, doc_comment)| {
                    let lsp_kind = match kind {
                        index::SymKind::Function => CompletionItemKind::FUNCTION,
                        index::SymKind::Struct => CompletionItemKind::STRUCT,
                        index::SymKind::Enum => CompletionItemKind::ENUM,
                        index::SymKind::EnumMember => CompletionItemKind::ENUM_MEMBER,
                        index::SymKind::Field => CompletionItemKind::FIELD,
                        index::SymKind::Method => CompletionItemKind::METHOD,
                        index::SymKind::Variable | index::SymKind::Param => {
                            CompletionItemKind::VARIABLE
                        }
                        index::SymKind::Type => CompletionItemKind::CLASS,
                        index::SymKind::Keyword => CompletionItemKind::KEYWORD,
                    };
                    CompletionItem {
                        label,
                        kind: Some(lsp_kind),
                        detail: Some(detail),
                        documentation: doc_comment.map(|doc| {
                            Documentation::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: doc,
                            })
                        }),
                        ..Default::default()
                    }
                })
                .collect();
            return Ok(Some(CompletionResponse::Array(items)));
        }
        Ok(None)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let text = self.document_map.get(&uri).map(|v| v.value().clone());
        if let Some(text) = text {
            let file_path = params
                .text_document_position_params
                .text_document
                .uri
                .to_file_path()
                .ok()
                .map(|p| p.to_string_lossy().to_string());
            let line_index = position::LineIndex::new(&text);
            let offset = line_index.offset(
                params.text_document_position_params.position.line,
                params.text_document_position_params.position.character,
            );
            let idx = index::Index::build(file_path.as_deref(), &text);
            if let Some(decl) = idx.signature_help(&text, offset) {
                let mut parameters = vec![];
                let label = decl.detail.clone();

                if let Some(start_paren) = label.find('(') {
                    if let Some(end_paren) = label.rfind(')') {
                        if start_paren < end_paren {
                            let params_str = &label[start_paren + 1..end_paren];
                            if !params_str.trim().is_empty() {
                                for param in params_str.split(',') {
                                    parameters.push(ParameterInformation {
                                        label: ParameterLabel::Simple(param.trim().to_string()),
                                        documentation: None,
                                    });
                                }
                            }
                        }
                    }
                }

                let bytes = text.as_bytes();
                let mut active_parameter = 0;
                let mut i = offset;
                let mut paren_count = 0;
                while i > 0 {
                    i -= 1;
                    let b = bytes[i];
                    if b == b')' {
                        paren_count += 1;
                    } else if b == b'(' {
                        if paren_count > 0 {
                            paren_count -= 1;
                        } else {
                            break;
                        }
                    } else if b == b',' && paren_count == 0 {
                        active_parameter += 1;
                    } else if b == b';' || b == b'{' || b == b'}' {
                        break;
                    }
                }

                return Ok(Some(SignatureHelp {
                    signatures: vec![SignatureInformation {
                        label,
                        documentation: decl.doc_comment.map(|doc| {
                            Documentation::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: doc,
                            })
                        }),
                        parameters: Some(parameters),
                        active_parameter: Some(active_parameter),
                    }],
                    active_signature: Some(0),
                    active_parameter: Some(active_parameter),
                }));
            }
        }
        Ok(None)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri.to_string();
        let text = self.document_map.get(&uri).map(|v| v.value().clone());
        if let Some(text) = text {
            let formatted = format::format(&text);
            let line_index = position::LineIndex::new(&text);
            let end_pos = line_index.position(text.len());
            return Ok(Some(vec![TextEdit {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: map_position(end_pos),
                },
                new_text: formatted,
            }]));
        }
        Ok(None)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();
        let text = self.document_map.get(&uri).map(|v| v.value().clone());
        if let Some(text) = text {
            let file_path = params
                .text_document
                .uri
                .to_file_path()
                .ok()
                .map(|p| p.to_string_lossy().to_string());
            let tokens = get_semantic_tokens(file_path.as_deref(), &text);
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            })));
        }
        Ok(None)
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        document_map: DashMap::new(),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
