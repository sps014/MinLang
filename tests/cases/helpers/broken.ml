// This imported file contains a semantic error (return type mismatch). The compiler
// must report the error against THIS file's name and line, not the importing file.
fun broken_helper(): int {
    return "not an int";
}
