// Registers the "dream" language with Monaco: a Monarch grammar for immediate syntax
// highlighting plus the editor configuration (comments, brackets, auto-closing pairs).

import type * as monaco from "monaco-editor/esm/vs/editor/editor.api";

export const LANGUAGE_ID = "dream";

const keywords = [
  "if", "else", "for", "while", "do", "return", "break", "continue", "let", "const",
  "fun", "static", "import", "export", "extern", "class", "extend", "enum", "type",
  "switch", "case", "default", "is", "in", "this", "constructor", "del",
];

const typeKeywords = ["int", "float", "double", "string", "bool", "char", "void", "object"];

const literals = ["true", "false", "null"];

export function registerDreamLanguage(m: typeof monaco): void {
  if (m.languages.getLanguages().some((l) => l.id === LANGUAGE_ID)) {
    return;
  }

  m.languages.register({ id: LANGUAGE_ID, extensions: [".dream"], aliases: ["Dream", "dream"] });

  m.languages.setLanguageConfiguration(LANGUAGE_ID, {
    comments: { lineComment: "//", blockComment: ["/*", "*/"] },
    brackets: [
      ["{", "}"],
      ["[", "]"],
      ["(", ")"],
    ],
    autoClosingPairs: [
      { open: "{", close: "}" },
      { open: "[", close: "]" },
      { open: "(", close: ")" },
      { open: '"', close: '"', notIn: ["string"] },
      { open: "'", close: "'", notIn: ["string"] },
    ],
    surroundingPairs: [
      { open: "{", close: "}" },
      { open: "[", close: "]" },
      { open: "(", close: ")" },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
    ],
  });

  m.languages.setMonarchTokensProvider(LANGUAGE_ID, {
    defaultToken: "",
    keywords,
    typeKeywords,
    literals,
    operators: [
      "=", ">", "<", "!", "?", "??", ":", "==", "!=", "<=", ">=", "&&", "||", "++", "--",
      "+", "-", "*", "/", "%", "&", "|", "^", "<<", ">>", "+=", "-=", "*=", "/=", "%=",
    ],
    symbols: /[=><!~?:&|+\-*/^%]+/,
    escapes: /\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4})/,
    tokenizer: {
      root: [
        [
          /[a-zA-Z_]\w*/,
          {
            cases: {
              "@keywords": "keyword",
              "@typeKeywords": "type",
              "@literals": "keyword",
              "@default": "identifier",
            },
          },
        ],
        { include: "@whitespace" },
        [/[{}()[\]]/, "@brackets"],
        [/@symbols/, { cases: { "@operators": "operator", "@default": "" } }],
        [/\d+(\.\d+)?([dDfF])?/, "number"],
        [/"([^"\\]|\\.)*$/, "string.invalid"],
        [/"/, { token: "string.quote", bracket: "@open", next: "@string" }],
        [/'[^\\']'/, "string"],
        [/'(\\.)'/, "string"],
        [/[;,.]/, "delimiter"],
      ],
      whitespace: [
        [/[ \t\r\n]+/, ""],
        [/\/\*/, "comment", "@comment"],
        [/\/\/.*$/, "comment"],
      ],
      comment: [
        [/[^/*]+/, "comment"],
        [/\*\//, "comment", "@pop"],
        [/[/*]/, "comment"],
      ],
      string: [
        [/[^\\"]+/, "string"],
        [/@escapes/, "string.escape"],
        [/\\./, "string.escape.invalid"],
        [/"/, { token: "string.quote", bracket: "@close", next: "@pop" }],
      ],
    },
  });
}
