"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const node_1 = require("vscode-languageclient/node");
let client;
function activate(context) {
    // Use the absolute path because the extension runs from ~/.vscode/extensions/...
    // but we want it to run the language server from the project directory.
    const manifestPath = '/Users/shivendra/Desktop/Projects/Dream/tooling/dream-lsp/Cargo.toml';
    // Server options point to cargo run for development convenience
    const serverOptions = {
        command: 'cargo',
        args: ['run', '-q', '--manifest-path', manifestPath]
    };
    // Client options
    const clientOptions = {
        documentSelector: [{ scheme: 'file', language: 'dream' }]
    };
    // Create the language client and start the client.
    client = new node_1.LanguageClient('dreamLanguageServer', 'Dream Language Server', serverOptions, clientOptions);
    // Start the client. This will also launch the server
    context.subscriptions.push(client);
    client.start();
}
function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
//# sourceMappingURL=extension.js.map