# MiraMD architecture

## Goals and assumptions

- A solo developer or small team should be able to ship the MVP.
- Files remain ordinary UTF-8 Markdown; there is no proprietary document database.
- macOS and Windows should feel native, including menus, shortcuts, file dialogs, accessibility, and system themes.
- AI access must be local-first, least-privilege, and resistant to stale overwrites.
- The initial audience edits normal documents; collaboration and cloud sync are outside the MVP.

## Components

```text
                    local stdio
 AI host  ─────────────────────────►  miramd-mcp (Rust)
                                            │
                       allowed folders only │ read / revision-checked write
                                            ▼
                                      Markdown files
                                            ▲
                                            │ open / save
          ┌─────────────────────────────────┴─────────────────────────┐
          │                                                           │
 macOS: SwiftUI + AppKit + WebKit                         Windows: WinUI 3 + WebView2
          │                                                           │
          └───────────────── C ABI: UTF-8 strings ────────────────────┘
                                      │
                                      ▼
                           miramd-core (Rust)
             parse • sanitize • outline • stats • AI export • DOCX
```

## Key decisions

### Native shells with a Rust engine

SwiftUI/AppKit and WinUI 3 give each platform native windows, controls, keyboard behavior, file pickers, and accessibility. Rust owns deterministic document behavior. The tradeoff is two UI codebases, but the hard-to-keep-consistent parsing and export logic remains shared.

### Markdown remains the canonical format

The editor never converts the file into a proprietary rich-text model. That makes files durable and ideal for AI tools. The tradeoff is that the MVP exposes Markdown source in edit mode; a later block editor will need careful round-trip rules.

### Embedded system web views are preview-only

WebKit and WebView2 display sanitized HTML from the Rust engine. Editing stays in native controls. Raw HTML is sanitized and preview JavaScript is disabled on macOS. External links are handed to the default browser.

### MCP is a separate stdio process

AI hosts launch `miramd-mcp` only when configured. It has no network listener and receives explicit allowed roots at startup. This avoids adding a background daemon or an unauthenticated localhost port.

For writes, the server uses this flow:

```text
read_markdown → content + SHA-256 revision
                         │
AI proposes edit         │
                         ▼
write/edit + expected revision → compare → temp file → sync → replace
                                      └─ mismatch: fail and re-read
```

### DOCX and Lao text

The Rust engine writes OOXML directly, so Microsoft Word export has no runtime dependency on Python, Node, Pandoc, Word, or LibreOffice. Markdown headings become built-in Word heading styles, lists use OOXML numbering, and tables become fixed-width Word tables.

All Lao runs use `Phetsarath OT` and include Word complex-script flags (`w:cs`, `w:szCs`, `w:bCs`/`w:iCs`, and `w:lang w:bidi="lo-LA"`). `word/settings.xml` also declares Lao as the bidirectional theme language. These details enable Word’s Lao dictionary segmentation and prevent line breaks inside words.

The desktop display path deliberately uses the separate Lao-only `Phetsarath Lao` web/app family. Only Lao Unicode ranges receive that font; English remains in SF Pro/SF Mono on macOS and Segoe UI/Cascadia on Windows. Phetsarath Lao is never written into DOCX files.

## Failure behavior

- Invalid UTF-8 files produce a readable error and are not modified.
- Files outside allowlisted roots, symlink escapes, parent traversal, and non-Markdown extensions are rejected.
- Existing files cannot be replaced without a current revision.
- Non-unique text edits fail unless the caller explicitly requests replacement of every match.
- Preview HTML is sanitized before it reaches a web view.
- Saves use a same-folder temporary file, flush it, then replace the destination.

## What to revisit as the product grows

1. Add file watching and a native conflict sheet when MCP or another editor changes the open document.
2. Add a block-based visual editor with lossless fallback for tables, directives, and embedded HTML.
3. Add an opt-in local IPC channel so MCP can target an unsaved open buffer, guarded by in-app approval.
4. Add autosave, crash recovery, tabs, recent files, and workspace search.
5. Add signed/notarized macOS packaging and MSIX packaging for Windows.
6. If remote MCP is ever offered, introduce authenticated Streamable HTTP, audit logs, and per-tool consent; do not expose the local stdio design over a network.
