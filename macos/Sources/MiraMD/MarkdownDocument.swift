import AppKit
import Foundation

@MainActor
final class MarkdownDocument: ObservableObject {
    @Published var content: String {
        didSet {
            guard !isLoading, content != oldValue else { return }
            isDirty = true
            refreshDerivedContent()
        }
    }
    @Published private(set) var fileURL: URL?
    @Published private(set) var isDirty = false
    @Published private(set) var renderedHTML = ""
    @Published private(set) var outline: [OutlineItem] = []
    @Published private(set) var stats: DocumentStats = .empty

    private var isLoading = false

    init() {
        content = Self.starterDocument
        refreshDerivedContent()
    }

    var displayName: String { fileURL?.lastPathComponent ?? "Untitled.md" }
    var windowTitle: String { isDirty ? "\(displayName) — Edited" : displayName }

    func newDocument() {
        guard confirmDiscardIfNeeded() else { return }
        load(content: Self.starterDocument, from: nil)
    }

    func showOpenPanel() {
        guard confirmDiscardIfNeeded() else { return }
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.init(filenameExtension: "md")!, .plainText]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.message = "Choose a Markdown document"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        loadFromDisk(url)
    }

    func open(_ url: URL) {
        guard confirmDiscardIfNeeded() else { return }
        loadFromDisk(url)
    }

    private func loadFromDisk(_ url: URL) {
        do {
            let value = try String(contentsOf: url, encoding: .utf8)
            load(content: value, from: url)
        } catch {
            showError(title: "Couldn’t open the document", error: error)
        }
    }

    @discardableResult
    func save() -> Bool {
        guard let fileURL else { return saveAs() }
        return write(to: fileURL)
    }

    @discardableResult
    func saveAs() -> Bool {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.init(filenameExtension: "md")!]
        panel.nameFieldStringValue = displayName
        panel.canCreateDirectories = true
        panel.message = "Save as a Markdown document"
        guard panel.runModal() == .OK, let url = panel.url else { return false }
        return write(to: url)
    }

    func copyForAI() {
        let value = MiraCore.exportForAI(content, filename: displayName)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(value, forType: .string)
    }

    func exportForAI() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.plainText]
        panel.nameFieldStringValue = displayName.replacingOccurrences(of: ".md", with: "-for-ai.txt")
        panel.canCreateDirectories = true
        panel.message = "Export a paste-safe AI context file"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            let value = MiraCore.exportForAI(content, filename: displayName)
            try value.write(to: url, atomically: true, encoding: .utf8)
        } catch {
            showError(title: "Couldn’t export the document", error: error)
        }
    }

    func exportCleanMarkdown() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.init(filenameExtension: "md")!]
        let source = URL(fileURLWithPath: displayName)
        panel.nameFieldStringValue = source.deletingPathExtension().lastPathComponent + "-clean.md"
        panel.canCreateDirectories = true
        panel.message = "Export standard Markdown without MiraMD highlight markers"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try MiraCore.cleanMarkdown(content).write(to: url, atomically: true, encoding: .utf8)
        } catch {
            showError(title: "Couldn’t export clean Markdown", error: error)
        }
    }

    func exportWord() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.init(filenameExtension: "docx")!]
        panel.nameFieldStringValue = URL(fileURLWithPath: displayName)
            .deletingPathExtension()
            .appendingPathExtension("docx")
            .lastPathComponent
        panel.canCreateDirectories = true
        panel.message = "Export as a Microsoft Word document with Unicode and Lao support"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try MiraCore.exportWord(content, title: displayName, to: url)
        } catch {
            showError(title: "Couldn’t export the Word document", error: error)
        }
    }

    func copyMCPSetup() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = true
        panel.message = "Choose the folder AI tools may read and write"
        panel.prompt = "Allow Folder"
        guard panel.runModal() == .OK, let folder = panel.url else { return }

        let bundledServer = Bundle.main.path(forAuxiliaryExecutable: "miramd-mcp")
            ?? ProcessInfo.processInfo.environment["MIRAMD_MCP_BINARY"]
            ?? "miramd-mcp"
        let configuration: [String: Any] = [
            "mcpServers": [
                "miramd": [
                    "command": bundledServer,
                    "args": ["--allow", folder.path, "--app", Bundle.main.bundlePath],
                ],
            ],
        ]

        do {
            let data = try JSONSerialization.data(withJSONObject: configuration, options: [.prettyPrinted, .sortedKeys])
            guard let value = String(data: data, encoding: .utf8) else { return }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(value, forType: .string)
            let alert = NSAlert()
            alert.messageText = "MCP configuration copied"
            alert.informativeText = "Paste it into the MCP settings for your AI client. Only \(folder.path) will be accessible."
            alert.runModal()
        } catch {
            showError(title: "Couldn’t create MCP configuration", error: error)
        }
    }

    private func load(content: String, from url: URL?) {
        isLoading = true
        self.content = content
        fileURL = url
        isDirty = false
        isLoading = false
        refreshDerivedContent()
        if let url {
            NotificationCenter.default.post(name: .miramdWorkspaceChanged, object: url)
        }
    }

    private func write(to url: URL) -> Bool {
        do {
            try content.write(to: url, atomically: true, encoding: .utf8)
            fileURL = url
            isDirty = false
            NotificationCenter.default.post(name: .miramdWorkspaceChanged, object: url)
            return true
        } catch {
            showError(title: "Couldn’t save the document", error: error)
            return false
        }
    }

    private func refreshDerivedContent() {
        renderedHTML = MiraCore.render(content)
        outline = MiraCore.outline(content)
        stats = MiraCore.stats(content)
    }

    private func confirmDiscardIfNeeded() -> Bool {
        guard isDirty else { return true }
        let alert = NSAlert()
        alert.messageText = "Save changes to \(displayName)?"
        alert.informativeText = "Your changes will be lost if you don’t save them."
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: "Don’t Save")
        switch alert.runModal() {
        case .alertFirstButtonReturn: return save()
        case .alertThirdButtonReturn: return true
        default: return false
        }
    }

    private func showError(title: String, error: Error) {
        let alert = NSAlert(error: error)
        alert.messageText = title
        alert.runModal()
    }

    private static let starterDocument = """
    # Welcome to MiraMD

    MiraMD makes Markdown feel like a normal document while keeping the clean text that AI tools understand.

    ## Start writing

    Use the formatting bar to create **bold text**, *emphasis*, links, lists, quotes, and code without memorizing Markdown.

    - Read in a polished preview
    - Edit with familiar controls
    - Export a paste-safe copy for an AI conversation

    > Tip: Choose **View → Preview** when you only want to read.

    ```swift
    print("Markdown stays portable.")
    ```
    """
}
