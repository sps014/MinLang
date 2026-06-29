# Dream Tooling

This directory contains developer tooling for the Dream language, focused around the native Language Server Protocol (LSP) and editor extensions.

## Layout

- [`dream-lsp/`](dream-lsp) — A native Rust Language Server Protocol (LSP) server binary powered by `tower-lsp`. It reuses the compiler's frontend (lexer, parser, semantic analyzer) to provide rich IntelliSense features.
- [`vscode/`](vscode) — A Visual Studio Code extension client written in TypeScript that connects to the `dream-lsp` server.

## Features Supported

The `dream-lsp` server provides the following capabilities:
- **Real-time Diagnostics**: Reports syntax and semantic errors/warnings directly in the editor.
- **Semantic Tokens**: AST-driven, perfectly accurate syntax highlighting (functions, classes, fields, parameters, etc.).
- **Autocomplete (IntelliSense)**: Intelligent completions for keywords, data types, and scoped symbols (including cross-file imports).
- **Hover**: Rich Markdown hover tooltips displaying symbol signatures and documentation comments.
- **Signature Help**: Pop-up parameter hints and active parameter tracking when writing function or constructor calls.
- **Go to Definition / Find References**: Jump to or find all usages of a symbol across the project.
- **Formatting**: Brace-depth indentation.

## Building and Running the Extension

To test or develop the VS Code extension:

1. Ensure you have Node.js, `npm`, and `cargo` installed.
2. Navigate to the `vscode/` folder:
   ```bash
   cd vscode
   npm install
   npm run compile
   ```
3. You can either open the workspace in VS Code and press **F5** to launch the Extension Development Host, or build a `.vsix` package to install it globally:
   ```bash
   npx @vscode/vsce package
   code --install-extension dream-lang-0.1.0.vsix
   ```

*(Note: The VS Code extension automatically attempts to invoke `cargo run` from the `dream-lsp` crate when starting, so you must have the Rust toolchain installed locally).*

## Testing the LSP Server

The LSP server contains standalone tests to verify the compiler and analysis pipeline works without needing an editor:

```bash
cargo test -p dream-lsp
```