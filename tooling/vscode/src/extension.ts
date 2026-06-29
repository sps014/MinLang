import * as path from 'path';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: vscode.ExtensionContext) {
    // Use the absolute path because the extension runs from ~/.vscode/extensions/...
    // but we want it to run the language server from the project directory.
    const manifestPath = '/Users/shivendra/Desktop/Projects/Dream/tooling/dream-lsp/Cargo.toml';

    // Server options point to cargo run for development convenience
    const serverOptions: ServerOptions = {
        command: 'cargo',
        args: ['run', '-q', '--manifest-path', manifestPath]
    };

    // Client options
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'dream' }]
    };

    // Create the language client and start the client.
    client = new LanguageClient(
        'dreamLanguageServer',
        'Dream Language Server',
        serverOptions,
        clientOptions
    );

    // Start the client. This will also launch the server
    context.subscriptions.push(client);
    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
