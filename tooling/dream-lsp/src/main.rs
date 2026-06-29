//! Entry point for the native Dream language server. All protocol handling lives in
//! [`dream_lsp::backend::Backend`]; this binary only wires it to stdio and the tokio runtime.

use tower_lsp::{LspService, Server};

use dream_lsp::backend::Backend;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);

    Server::new(stdin, stdout, socket).serve(service).await;
}
