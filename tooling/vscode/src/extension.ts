import * as path from 'path';
import * as vscode from 'vscode';
import * as fs from 'fs';
import { exec } from 'child_process';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions
} from 'vscode-languageclient/node';

let client: LanguageClient;

export async function activate(context: vscode.ExtensionContext) {
    const outputChannel = vscode.window.createOutputChannel('Dream Language Server');
    outputChannel.appendLine('Activating Dream extension...');

    const platform = process.platform;
    const arch = process.arch;
    const ext = platform === 'win32' ? '.exe' : '';
    
    // Check for platform-specific binary (e.g. dream-lsp-darwin-arm64)
    const specificBinName = `dream-lsp-${platform}-${arch}${ext}`;
    const genericBinName = `dream-lsp${ext}`;
    
    const specificBinPath = path.join(__dirname, '..', 'bin', specificBinName);
    const genericBinPath = path.join(__dirname, '..', 'bin', genericBinName);
    
    let binPath = '';
    if (fs.existsSync(specificBinPath)) {
        binPath = specificBinPath;
    } else if (fs.existsSync(genericBinPath)) {
        binPath = genericBinPath;
    }

    let serverOptions: ServerOptions;

    if (binPath !== '') {
        outputChannel.appendLine(`Found bundled binary at ${binPath}`);
        try {
            fs.chmodSync(binPath, '755');
        } catch (e) {
            outputChannel.appendLine(`Failed to make binary executable: ${e}`);
        }
        serverOptions = {
            command: binPath,
            args: [],
            options: { env: process.env }
        };
    } else {
        outputChannel.appendLine('Bundled binary not found. Falling back to cargo...');
        
        const isCargoAvailable = await new Promise<boolean>((resolve) => {
            exec('cargo --version', (error) => resolve(!error));
        });

        if (!isCargoAvailable) {
            const msg = 'Dream LSP failed to start: "cargo" is not available in your PATH, and no bundled binary was found.';
            vscode.window.showErrorMessage(msg);
            outputChannel.appendLine(msg);
            outputChannel.show();
            return;
        }

        const manifestPath = path.join(__dirname, '..', '..', 'dream-lsp', 'Cargo.toml');
        serverOptions = {
            command: 'cargo',
            args: ['run', '-q', '--manifest-path', manifestPath],
            options: { env: process.env }
        };
    }

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'dream' }],
        outputChannel: outputChannel
    };

    client = new LanguageClient(
        'dreamLanguageServer',
        'Dream Language Server',
        serverOptions,
        clientOptions
    );

    context.subscriptions.push(client);

    try {
        outputChannel.appendLine('Starting client...');
        await client.start();
        outputChannel.appendLine('Client started successfully.');
    } catch (err) {
        outputChannel.appendLine(`Failed to start client: ${err}`);
        vscode.window.showErrorMessage(`Dream LSP failed to start. Check the 'Dream Language Server' output channel for details.`);
        outputChannel.show();
    }
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
