// `edcore.main` bundles the full editor including all contributions (suggest widget, hover,
// definition peek, find references, formatting actions, context menu) but excludes the built-in
// languages we don't need. Importing the bare `editor.api` would omit these contributions, so the
// suggestion/hover popups would never appear. Types come from the ambient declaration in
// `monaco-edcore.d.ts`, which re-exports the full `editor.api` namespace.
import * as monaco from "monaco-editor/esm/vs/editor/edcore.main";
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";

import { LANGUAGE_ID, registerDreamLanguage } from "./dream-language";
import {
  initAnalyzer,
  getDiagnostics,
  getSemanticTokens,
  getTokenLegend,
  getHover,
  getDefinition,
  getReferences,
  getCompletions,
  formatDocument,
  type Range as WasmRange,
} from "./analyzer";

// Monaco only needs its base editor worker for a custom language.
(globalThis as unknown as { MonacoEnvironment: monaco.Environment }).MonacoEnvironment = {
  getWorker: () => new editorWorker(),
};

const SAMPLE = `// Welcome to the Dream playground.
// Edit the code below to see live diagnostics, hover, completion, and more.

class Point {
    x: int;
    y: int;

    fun magnitude_squared(): int {
        return this.x * this.x + this.y * this.y;
    }
}

enum Color {
    Red,
    Green,
    Blue
}

fun add(a: int, b: int): int {
    return a + b;
}

fun main(): void {
    let origin: Point = Point { x: 0, y: 0 };
    let total: int = add(3, 4);
    let favorite: Color = Color.Green;
    println(total);
}
`;

/** Converts a 0-based WASM range into Monaco's 1-based range. */
function toMonacoRange(range: WasmRange): monaco.IRange {
  return {
    startLineNumber: range.start.line + 1,
    startColumn: range.start.character + 1,
    endLineNumber: range.end.line + 1,
    endColumn: range.end.character + 1,
  };
}

function completionKind(kind: string): monaco.languages.CompletionItemKind {
  const k = monaco.languages.CompletionItemKind;
  switch (kind) {
    case "function":
      return k.Function;
    case "method":
      return k.Method;
    case "struct":
      return k.Struct;
    case "enum":
      return k.Enum;
    case "enumMember":
      return k.EnumMember;
    case "field":
      return k.Field;
    case "variable":
      return k.Variable;
    case "type":
      return k.Class;
    case "keyword":
      return k.Keyword;
    default:
      return k.Text;
  }
}

function registerProviders(): void {
  const legend = getTokenLegend();
  const legendIndex = new Map(legend.map((name, i) => [name, i]));

  monaco.languages.registerDocumentSemanticTokensProvider(LANGUAGE_ID, {
    getLegend: () => ({ tokenTypes: legend, tokenModifiers: [] }),
    provideDocumentSemanticTokens: (model) => {
      const tokens = getSemanticTokens(model.getValue());
      const data: number[] = [];
      let prevLine = 0;
      let prevChar = 0;
      for (const token of tokens) {
        const line = token.range.start.line;
        const char = token.range.start.character;
        const length = token.range.end.character - token.range.start.character;
        const type = legendIndex.get(token.kind);
        if (type === undefined || length <= 0) {
          continue;
        }
        const deltaLine = line - prevLine;
        const deltaChar = deltaLine === 0 ? char - prevChar : char;
        data.push(deltaLine, deltaChar, length, type, 0);
        prevLine = line;
        prevChar = char;
      }
      return { data: new Uint32Array(data) };
    },
    releaseDocumentSemanticTokens: () => undefined,
  });

  monaco.languages.registerHoverProvider(LANGUAGE_ID, {
    provideHover: (model, position) => {
      const hover = getHover(model.getValue(), position.lineNumber - 1, position.column - 1);
      if (!hover) {
        return null;
      }
      return {
        range: toMonacoRange(hover.range),
        contents: [{ value: "```dream\n" + hover.contents + "\n```" }],
      };
    },
  });

  monaco.languages.registerDefinitionProvider(LANGUAGE_ID, {
    provideDefinition: (model, position) => {
      const def = getDefinition(model.getValue(), position.lineNumber - 1, position.column - 1);
      if (!def) {
        return null;
      }
      return { uri: model.uri, range: toMonacoRange(def.range) };
    },
  });

  monaco.languages.registerReferenceProvider(LANGUAGE_ID, {
    provideReferences: (model, position) => {
      const refs = getReferences(model.getValue(), position.lineNumber - 1, position.column - 1);
      return refs.map((location) => ({ uri: model.uri, range: toMonacoRange(location.range) }));
    },
  });

  monaco.languages.registerCompletionItemProvider(LANGUAGE_ID, {
    triggerCharacters: ["."],
    provideCompletionItems: (model, position) => {
      try {
        const word = model.getWordUntilPosition(position);
        const range: monaco.IRange = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };
        const items = getCompletions(model.getValue(), position.lineNumber - 1, position.column - 1);
        const suggestions = items.map((item) => ({
          label: item.label,
          kind: completionKind(item.kind),
          detail: item.detail,
          insertText: item.label,
          range,
        }));
        return { suggestions };
      } catch (err) {
        console.error("completion provider failed", err);
        return { suggestions: [] };
      }
    },
  });

  monaco.languages.registerDocumentFormattingEditProvider(LANGUAGE_ID, {
    provideDocumentFormattingEdits: (model) => {
      const formatted = formatDocument(model.getValue());
      return [{ range: model.getFullModelRange(), text: formatted }];
    },
  });
}

function refreshDiagnostics(model: monaco.editor.ITextModel): void {
  const diags = getDiagnostics(model.getValue());
  const markers: monaco.editor.IMarkerData[] = diags.map((diag) => ({
    severity:
      diag.severity === "error" ? monaco.MarkerSeverity.Error : monaco.MarkerSeverity.Warning,
    message: diag.message,
    ...toMonacoRange(diag.range),
  }));
  monaco.editor.setModelMarkers(model, LANGUAGE_ID, markers);
}

async function bootstrap(): Promise<void> {
  const status = document.getElementById("status")!;
  await initAnalyzer();

  registerDreamLanguage(monaco);
  registerProviders();

  const model = monaco.editor.createModel(SAMPLE, LANGUAGE_ID);
  const editor = monaco.editor.create(document.getElementById("editor")!, {
    model,
    theme: "vs-dark",
    automaticLayout: true,
    fontSize: 14,
    minimap: { enabled: false },
    "semanticHighlighting.enabled": true,
    formatOnType: false,
    // Pop the suggestion list automatically while typing, after `.`, and don't fall back to
    // generic word-based suggestions (we provide our own).
    quickSuggestions: { other: true, comments: false, strings: false },
    quickSuggestionsDelay: 0,
    suggestOnTriggerCharacters: true,
    wordBasedSuggestions: "off",
    tabCompletion: "on",
    acceptSuggestionOnEnter: "on",
  });

  // macOS reserves Ctrl+Space (input-source switching), so Monaco's default trigger-suggest
  // shortcut never reaches the editor. Bind Cmd/Ctrl+I as a reliable manual trigger too.
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyI, () => {
    editor.trigger("keyboard", "editor.action.triggerSuggest", {});
  });

  let timer: number | undefined;
  const schedule = () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => refreshDiagnostics(model), 250);
  };
  model.onDidChangeContent(schedule);
  refreshDiagnostics(model);

  status.textContent =
    "Analyzer ready. Autocomplete pops as you type (or press Cmd/Ctrl+I). Hover, Ctrl/Cmd+Click for definition, right-click \u2192 Format Document.";
  void editor;
}

bootstrap().catch((err) => {
  const status = document.getElementById("status");
  if (status) {
    status.textContent = "Failed to load analyzer: " + String(err);
  }
  console.error(err);
});
