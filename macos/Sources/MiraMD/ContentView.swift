import SwiftUI

enum EditorMode: String, CaseIterable, Identifiable {
    case visual = "Visual"
    case edit = "Edit"
    case split = "Split"
    case preview = "Preview"

    var id: Self { self }
    var icon: String {
        switch self {
        case .visual: "doc.text.image"
        case .edit: "square.and.pencil"
        case .split: "rectangle.split.2x1"
        case .preview: "doc.richtext"
        }
    }
}

struct ContentView: View {
    @EnvironmentObject private var document: MarkdownDocument
    @EnvironmentObject private var editor: EditorController
    @EnvironmentObject private var appearance: AppearanceController
    @State private var mode: EditorMode = .visual

    var body: some View {
        NavigationSplitView {
            DocumentSidebar(mode: $mode)
                .navigationSplitViewColumnWidth(min: 190, ideal: 230, max: 300)
        } detail: {
            VStack(spacing: 0) {
                if mode != .preview {
                    FormattingBar()
                    Divider()
                }
                editorBody
                Divider()
                statusBar
            }
            .navigationTitle(document.windowTitle)
            .toolbar { toolbar }
        }
    }

    @ViewBuilder
    private var editorBody: some View {
        switch mode {
        case .visual:
            VisualMarkdownEditor(
                text: $document.content,
                fragment: document.renderedHTML,
                controller: editor
            )
        case .edit:
            SourceEditor(text: $document.content, controller: editor)
        case .split:
            HSplitView {
                SourceEditor(text: $document.content, controller: editor)
                    .frame(minWidth: 300)
                MarkdownPreview(fragment: document.renderedHTML)
                    .frame(minWidth: 300)
            }
        case .preview:
            MarkdownPreview(fragment: document.renderedHTML)
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            Button { document.showOpenPanel() } label: { Label("Open", systemImage: "folder") }
                .help("Open Markdown or import Microsoft Word")
            Button { document.save() } label: { Label("Save", systemImage: "square.and.arrow.down") }
                .help("Save")
            Button { document.copyForAI() } label: { Label("Copy for AI", systemImage: "sparkles") }
                .help("Copy a paste-safe AI context")
            Button { document.exportWord() } label: { Label("Export Word", systemImage: "doc.badge.arrow.up") }
                .help("Export to Microsoft Word with Lao and Unicode support")
            Menu {
                Picker("Appearance", selection: $appearance.selection) {
                    ForEach(AppAppearance.allCases) { option in
                        Label(option.title, systemImage: option.icon).tag(option)
                    }
                }
            } label: {
                Label("Appearance", systemImage: appearance.selection.icon)
            }
            .help("System, light, or dark appearance")
            Picker("View", selection: $mode) {
                ForEach(EditorMode.allCases) { mode in
                    Label(mode.rawValue, systemImage: mode.icon).tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .frame(width: 310)
        }
    }

    private var statusBar: some View {
        HStack(spacing: 12) {
            Text("\(document.stats.words) words")
            Text("\(document.stats.lines) lines")
            if document.stats.readingMinutes > 0 {
                Text("\(document.stats.readingMinutes) min read")
            }
            Spacer()
            Text(mode == .visual ? "Visual editing • Markdown" : "Markdown")
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .padding(.horizontal, 12)
        .frame(height: 28)
        .background(.bar)
    }
}

private struct DocumentSidebar: View {
    @EnvironmentObject private var document: MarkdownDocument
    @EnvironmentObject private var editor: EditorController
    @EnvironmentObject private var workspace: WorkspaceModel
    @Binding var mode: EditorMode

    var body: some View {
        List {
            Section("Library") {
                HStack(spacing: 10) {
                    Button { workspace.chooseFolder() } label: { Image(systemName: "folder.badge.plus") }
                        .help("Choose library folder")
                    Button { workspace.refresh() } label: { Image(systemName: "arrow.clockwise") }
                        .help("Refresh library")
                        .disabled(workspace.rootURL == nil)
                    Menu {
                        Button("New Markdown File…") {
                            if let url = workspace.createMarkdownFile() { document.open(url) }
                        }
                        Button("New Folder…") { workspace.createFolder() }
                    } label: {
                        Image(systemName: "plus")
                    }
                    .help("Create file or folder")
                    Spacer()
                    if workspace.rootURL != nil {
                        Button { workspace.clear() } label: { Image(systemName: "xmark.circle") }
                            .help("Close library")
                    }
                }
                if let root = workspace.rootURL {
                    Label(root.lastPathComponent, systemImage: "folder.fill")
                        .font(.subheadline.weight(.medium))
                        .lineLimit(1)
                        .help(root.path)
                    TextField("Filter Markdown", text: $workspace.query)
                        .textFieldStyle(.roundedBorder)
                    if workspace.visibleItems.isEmpty {
                        Text(workspace.query.isEmpty ? "No Markdown files yet" : "No matching files")
                            .foregroundStyle(.secondary)
                    } else {
                        OutlineGroup(workspace.visibleItems, children: \.children) { item in
                            WorkspaceItemRow(item: item) {
                                document.open(item.url)
                            }
                        }
                    }
                } else {
                    Button("Choose a folder…") { workspace.chooseFolder() }
                        .buttonStyle(.link)
                }
            }
            Section("Document") {
                Label("\(document.stats.words) words", systemImage: "text.word.spacing")
                Label("\(document.stats.characters) characters", systemImage: "character.cursor.ibeam")
                Label("\(document.stats.readingMinutes) min read", systemImage: "clock")
            }
            Section("AI Access") {
                Button {
                    document.copyMCPSetup()
                } label: {
                    Label("Set up MCP…", systemImage: "point.3.connected.trianglepath.dotted")
                }
                .buttonStyle(.plain)
                .help("Allow an AI client to read and safely edit a folder")
            }
            Section("Outline") {
                if document.outline.isEmpty {
                    Text("Add headings to build an outline")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(document.outline) { heading in
                        Button {
                            mode = .edit
                            editor.perform(.goToLine(heading.line))
                        } label: {
                            Text(heading.text)
                                .font(
                                    AppFonts.containsLao(heading.text)
                                        ? .custom(AppFonts.laoFamily, size: 16, relativeTo: .body)
                                        : .body
                                )
                                .lineLimit(1)
                                .padding(.leading, CGFloat(max(0, heading.level - 1)) * 10)
                        }
                        .buttonStyle(.plain)
                        .help("Go to line \(heading.line)")
                    }
                }
            }
        }
        .listStyle(.sidebar)
    }
}

private struct WorkspaceItemRow: View {
    @EnvironmentObject private var workspace: WorkspaceModel
    let item: WorkspaceItem
    let open: () -> Void

    var body: some View {
        Group {
            if item.isDirectory {
                Label(item.name, systemImage: "folder")
            } else {
                Button(action: open) {
                    Label(item.name, systemImage: item.name.lowercased() == "skill.md" ? "hammer" : "doc.text")
                        .lineLimit(1)
                }
                .buttonStyle(.plain)
            }
        }
        .contextMenu {
            Button("Reveal in Finder") { workspace.reveal(item) }
        }
    }
}

private struct FormattingBar: View {
    @EnvironmentObject private var editor: EditorController

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 4) {
                Menu {
                    ForEach(1...4, id: \.self) { level in
                        Button("Heading \(level)") { editor.perform(.heading(level)) }
                    }
                } label: {
                    Label("Heading", systemImage: "textformat.size")
                }
                Divider().frame(height: 22)
                formatButton("Bold", icon: "bold", .bold)
                formatButton("Italic", icon: "italic", .italic)
                formatButton("Highlight", icon: "highlighter", .highlight)
                formatButton("Link", icon: "link", .link)
                formatButton("Code", icon: "chevron.left.forwardslash.chevron.right", .inlineCode)
                Divider().frame(height: 22)
                formatButton("Bulleted list", icon: "list.bullet", .bulletedList)
                formatButton("Numbered list", icon: "list.number", .numberedList)
                formatButton("Task", icon: "checklist", .task)
                formatButton("Quote", icon: "text.quote", .quote)
                formatButton("Code block", icon: "curlybraces.square", .codeBlock)
                formatButton("Divider", icon: "minus", .divider)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
        }
        .background(.bar)
    }

    private func formatButton(_ label: String, icon: String, _ action: EditorAction) -> some View {
        Button { editor.perform(action) } label: { Image(systemName: icon) }
            .buttonStyle(.borderless)
            .frame(width: 28, height: 24)
            .help(label)
    }
}
