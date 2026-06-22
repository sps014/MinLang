// The `edcore.main` entry point ships JavaScript without type declarations. It exposes the same
// public surface as `editor.api`, so re-export that namespace to type our runtime import.
declare module "monaco-editor/esm/vs/editor/edcore.main" {
  export * from "monaco-editor/esm/vs/editor/editor.api";
}
