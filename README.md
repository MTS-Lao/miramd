# MiraMD

MiraMD is a small native Markdown reader and editor for people who should not need to learn Markdown before they can work with it. It keeps `.md` files portable for AI and developer tools while presenting them as readable documents.

This repository contains a working macOS MVP, a WinUI 3 Windows shell, a shared Rust Markdown engine, and a local MCP server that lets an AI read and safely edit user-approved Markdown folders.

## What works

- Native SwiftUI/AppKit interface on macOS and native WinUI 3 interface on Windows
- Persistent System, Light, and Dark appearance choices
- Folder-based Markdown library with nested browsing, filtering, refresh, and file/folder creation
- Visual WYSIWYG-style editing plus source, split, and formatted preview modes
- Formatting controls for headings, emphasis, links, lists, tasks, quotes, and code
- Portable `==highlight==` annotations, native Word highlighting, and clean Markdown/AI exports that remove only the markers
- Document outline, word/line counts, and reading time
- Safe HTML rendering with tables, task lists, footnotes, and strikethrough
- Open, create, save, Save As, and AI-ready export
- Microsoft Word `.docx` export with headings, lists, tables, code, Unicode, and Lao complex-script support
- Phetsarath Lao for in-app Lao display, with native SF Pro/Segoe UI and SF Mono/Cascadia fallback for English
- MCP tools for listing, reading, creating, editing, rendering, and opening Markdown files
- Folder allowlists and revision checks for AI writes

## Run the macOS app

Requirements: macOS 14+, Xcode/Swift 6, and Rust.

```sh
cargo build -p miramd-core
swift run --package-path macos MiraMD
```

Build a movable app bundle containing the Rust library and MCP server:

```sh
./scripts/build-macos.sh
open dist/MiraMD.app
```

The build uses `miramd-icon-transparent-logo.png` for the app icon. Regenerate macOS and Windows icon assets after changing the logo with `./scripts/generate-icons.sh`. macOS signing automatically prefers an installed **Developer ID Application** identity, falls back to **Apple Development**, and finally uses ad-hoc signing. Set `MIRAMD_CODESIGN_IDENTITY` to override the selected identity.

## Build for Windows

Requirements: Windows 10 1809+, Visual Studio Build Tools with the C++ workload, .NET 8 SDK, and Rust's MSVC toolchain.

```powershell
.\scripts\build-windows.ps1 -Architecture x64
```

The unpackaged, self-contained output is written to `dist\windows-x64`.

## Connect an AI with MCP

In the macOS app, choose **AI → Copy MCP Setup…**, select a folder, and paste the copied JSON into an MCP-capable AI client's settings.

The equivalent manual configuration is:

```json
{
  "mcpServers": {
    "miramd": {
      "command": "/Applications/MiraMD.app/Contents/MacOS/miramd-mcp",
      "args": [
        "--allow",
        "/Users/you/Documents/Markdown",
        "--app",
        "/Applications/MiraMD.app"
      ]
    }
  }
}
```

Repeat `--allow FOLDER` to expose another folder. The server will not access paths outside those roots and only accepts Markdown extensions. Replacing an existing file requires the revision token returned by `read_markdown`; this makes stale AI writes fail instead of silently overwriting human changes.

Available MCP tools:

- `list_markdown_files`
- `read_markdown`
- `write_markdown`
- `edit_markdown`
- `open_in_miramd`
- `render_markdown`
- `export_word_document`

Word exports mark Lao runs with `Phetsarath OT`, complex-script sizing and emphasis properties, and `lo-LA` language metadata so Microsoft Word can shape and wrap Lao correctly. Install Phetsarath OT 4.100 or newer for the intended appearance; Word will use an available Lao fallback otherwise.

The in-app display font is the separate Lao-only **Phetsarath Lao** web/app edition from the [LaoFonts catalog](https://fonts.mts.la/f/phetsarath-lao). Refresh the bundled font assets from the catalog API with `python3 scripts/fetch-fonts.py`. This does not alter the DOCX export font.

## Test

```sh
cargo test --workspace
cargo build -p miramd-core
swift build --package-path macos
```

Windows must be compiled on Windows because WinUI 3 and the MSVC Rust target are not available on macOS.

## Visual editing boundary

Visual mode lets people edit the formatted document directly while MiraMD continuously maintains Markdown as the source of truth. It round-trips common paragraphs, headings, emphasis, highlights, links, lists, tasks, quotes, code, tables, and rules. Advanced Markdown extensions or hand-written HTML should still be edited in Source or Split mode because a visual edit may normalize unsupported syntax.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for component boundaries, safety decisions, and the next milestones.

## License

MiraMD is open-source software available under the [MIT License](LICENSE). The bundled Phetsarath Lao font has its own license and attribution in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
