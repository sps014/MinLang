// Thin typed wrapper around the `dream-analyzer` WebAssembly module. Each underlying export
// returns a JSON string; we parse it into the typed shapes below so the Monaco glue stays clean.

import init, {
  diagnostics as wasmDiagnostics,
  semantic_tokens as wasmSemanticTokens,
  token_legend as wasmTokenLegend,
  hover as wasmHover,
  definition as wasmDefinition,
  references as wasmReferences,
  completions as wasmCompletions,
  format_document as wasmFormat,
} from "./pkg/dream_analyzer.js";
import wasmUrl from "./pkg/dream_analyzer_bg.wasm?url";

export interface Position {
  line: number;
  character: number;
}

export interface Range {
  start: Position;
  end: Position;
}

export interface Diagnostic {
  range: Range;
  severity: "error" | "warning";
  message: string;
}

export interface SemanticToken {
  range: Range;
  kind: string;
}

export interface Hover {
  contents: string;
  range: Range;
}

export interface Completion {
  label: string;
  kind: string;
  detail: string;
}

export interface Location {
  range: Range;
}

let ready: Promise<void> | null = null;

/** Loads and instantiates the WASM module exactly once. */
export function initAnalyzer(): Promise<void> {
  if (!ready) {
    ready = init({ module_or_path: wasmUrl }).then(() => undefined);
  }
  return ready;
}

export function getDiagnostics(text: string): Diagnostic[] {
  return JSON.parse(wasmDiagnostics(text));
}

export function getSemanticTokens(text: string): SemanticToken[] {
  return JSON.parse(wasmSemanticTokens(text));
}

export function getTokenLegend(): string[] {
  return JSON.parse(wasmTokenLegend());
}

export function getHover(text: string, line: number, character: number): Hover | null {
  return JSON.parse(wasmHover(text, line, character));
}

export function getDefinition(text: string, line: number, character: number): Location | null {
  return JSON.parse(wasmDefinition(text, line, character));
}

export function getReferences(text: string, line: number, character: number): Location[] {
  return JSON.parse(wasmReferences(text, line, character));
}

export function getCompletions(text: string, line: number, character: number): Completion[] {
  return JSON.parse(wasmCompletions(text, line, character));
}

export function formatDocument(text: string): string {
  return wasmFormat(text);
}
