# File I/O

`File` and `FileStream` are the filesystem API for reading and writing files. Operations are
[`async`](../language/async.md) and return a `Future<T>` you `await`. The same `.dream` runs
unchanged on every host.

## Runtime support

| Runtime | Filesystem |
| --- | --- |
| Wasmtime (native CLI) | Real on-disk filesystem (`cargo run -- run file.dream`) |
| Node.js | Real on-disk filesystem via `node:fs` |
| Browser | In-memory virtual filesystem, like a C/C++ → WASM build's Emscripten MEMFS; files persist for the page session only |

The API is identical across all three; only the browser differs in that writes live in memory rather than on disk.

## Reading and writing text

`File.read` returns the whole file as a UTF-8 string; `File.write` replaces its contents and `File.append` adds to the end. Each resolves a `Future`, so `await` them inside an `async fun`:

```dream
async fun main(): void {
    await File.write("notes.txt", "hello\n");
    await File.append("notes.txt", "world\n");

    let text = await File.read("notes.txt");
    System.print(text);                 // "hello\nworld\n"
}
```

`write` and `append` resolve with the number of bytes written (`-1` on error).

## Metadata

`exists`, `size`, and `is_dir` are cheap and synchronous — no `await`:

```dream
async fun main(): void {
    if (File.exists("notes.txt")) {
        System.print("size = ");
        System.println(File.size("notes.txt"));   // bytes; -1 if missing
    }
}
```

## Listing a directory

`File.list` resolves to a `string[]` of entry names (empty for an empty or non-directory path):

```dream
async fun main(): void {
    let entries = await File.list(".");
    System.println(entries.len());
}
```

## Binary I/O

For non-text data, `read_bytes`/`write_bytes` move raw bytes directly between the file and a `char[]` with a single bulk copy — no string round-trip, so they are binary-safe (bytes such as `0x00` are preserved):

```dream
async fun main(): void {
    let bytes = await File.read_bytes("image.png");   // char[]
    await File.write_bytes("copy.png", bytes);
}
```

## Streaming with FileStream

`File.open` reads a file's bytes once into a buffered, seekable cursor. Reads slice the buffer in place; text views are materialized only when you ask for them, and random access via `seek` is allocation-free:

```dream
async fun main(): void {
    let stream = await File.open("notes.txt");

    System.println(stream.read(5));        // first 5 bytes as text
    System.println(stream.position());     // 5

    stream.seek(0);                        // rewind
    let head = stream.read_bytes(4);       // first 4 bytes as char[]

    while (stream.has_more()) {
        System.print(stream.read(16));     // 16-byte text chunks
    }
    stream.close();
}
```

## API reference

### File

| Member | Description |
| --- | --- |
| `File.read(path): Future<string>` | read the whole file as UTF-8 text |
| `File.write(path, content): Future<int>` | overwrite `path`; resolves with bytes written (`-1` on error) |
| `File.append(path, content): Future<int>` | append to `path`; resolves with bytes written |
| `File.read_bytes(path): Future<char[]>` | read the whole file as raw bytes (binary-safe) |
| `File.write_bytes(path, data): Future<int>` | write raw bytes; resolves with bytes written |
| `File.delete(path): Future<bool>` | delete `path`; resolves `true` on success |
| `File.list(path): Future<string[]>` | directory entry names (empty if not a directory) |
| `File.exists(path): bool` | true if `path` exists (synchronous) |
| `File.size(path): int` | size in bytes, or `-1` if missing (synchronous) |
| `File.is_dir(path): bool` | true if `path` is a directory (synchronous) |
| `File.open(path): Future<FileStream>` | open a buffered read stream |

### FileStream

A buffered cursor over a file's bytes. `read`/`read_all` produce text; `read_bytes` produces raw `char[]`. The cursor advances on each read.

| Member | Description |
| --- | --- |
| `read(n): string` | up to `n` bytes from the cursor as text |
| `read_bytes(n): char[]` | up to `n` raw bytes from the cursor |
| `read_all(): string` | everything remaining as text |
| `has_more(): bool` | true while unread bytes remain |
| `position(): int` | current cursor offset in bytes |
| `length(): int` | total buffered length in bytes |
| `seek(offset): void` | move the cursor to an absolute offset (clamped to `[0, length]`) |
| `reset(): void` | rewind the cursor to the start |
| `close(): void` | release the buffer (advances the cursor to the end) |

A runnable example lives in [`sample/interop/file_io.dream`](https://github.com/sps014/Dream/blob/main/sample/interop/file_io.dream).
