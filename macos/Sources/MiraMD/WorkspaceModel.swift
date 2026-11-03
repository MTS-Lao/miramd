import AppKit
import Combine
import UniformTypeIdentifiers

extension Notification.Name {
    static let miramdWorkspaceChanged = Notification.Name("MiraMD.workspaceChanged")
}

struct WorkspaceItem: Identifiable, Hashable {
    let url: URL
    let name: String
    let isDirectory: Bool
    let children: [WorkspaceItem]?

    var id: String { url.path }
}

@MainActor
final class WorkspaceModel: ObservableObject {
    @Published private(set) var rootURL: URL?
    @Published private(set) var items: [WorkspaceItem] = []
    @Published var query = ""

    private static let preferenceKey = "MiraMD.workspacePath"
    private static let markdownExtensions = ["md", "markdown", "mdown", "mkd"]
    private static let ignoredDirectories = [".git", ".build", ".swiftpm", "target", "node_modules", "dist", "bin", "obj"]
    private var cancellables = Set<AnyCancellable>()

    init() {
        if let path = UserDefaults.standard.string(forKey: Self.preferenceKey) {
            let url = URL(fileURLWithPath: path, isDirectory: true)
            if FileManager.default.fileExists(atPath: url.path) {
                rootURL = url
                refresh()
            }
        }
        NotificationCenter.default.publisher(for: .miramdWorkspaceChanged)
            .receive(on: RunLoop.main)
            .sink { [weak self] notification in
                guard let self else { return }
                if self.rootURL == nil, let fileURL = notification.object as? URL {
                    self.setRoot(fileURL.deletingLastPathComponent())
                } else {
                    self.refresh()
                }
            }
            .store(in: &cancellables)
    }

    var visibleItems: [WorkspaceItem] {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !term.isEmpty else { return items }
        return items.compactMap { filtered($0, matching: term) }
    }

    var rootName: String { rootURL?.lastPathComponent ?? "No folder selected" }

    func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Use Folder"
        panel.message = "Choose a folder containing Markdown documents or skills"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        setRoot(url)
    }

    func setRoot(_ url: URL) {
        rootURL = url
        UserDefaults.standard.set(url.path, forKey: Self.preferenceKey)
        refresh()
    }

    func clear() {
        rootURL = nil
        items = []
        query = ""
        UserDefaults.standard.removeObject(forKey: Self.preferenceKey)
    }

    func refresh() {
        guard let rootURL else {
            items = []
            return
        }
        items = loadChildren(of: rootURL, depth: 0)
    }

    func createMarkdownFile() -> URL? {
        guard let rootURL else {
            chooseFolder()
            return nil
        }
        let panel = NSSavePanel()
        panel.directoryURL = rootURL
        panel.allowedContentTypes = [UTType(filenameExtension: "md")!]
        panel.nameFieldStringValue = "Untitled.md"
        panel.canCreateDirectories = true
        panel.prompt = "Create"
        panel.message = "Create a Markdown or SKILL.md file in the library"
        guard panel.runModal() == .OK, let url = panel.url else { return nil }
        do {
            if !FileManager.default.fileExists(atPath: url.path) {
                let title = url.deletingPathExtension().lastPathComponent
                try "# \(title)\n\n".write(to: url, atomically: true, encoding: .utf8)
            }
            refresh()
            return url
        } catch {
            showError("Couldn’t create the Markdown file", error)
            return nil
        }
    }

    func createFolder() {
        guard let rootURL else {
            chooseFolder()
            return
        }
        let alert = NSAlert()
        alert.messageText = "New Folder"
        alert.informativeText = "Create a folder inside \(rootURL.lastPathComponent)."
        alert.addButton(withTitle: "Create")
        alert.addButton(withTitle: "Cancel")
        let field = NSTextField(string: "")
        field.placeholderString = "Folder name"
        field.frame = NSRect(x: 0, y: 0, width: 280, height: 24)
        alert.accessoryView = field
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        let name = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, name != ".", name != "..", !name.contains("/") else {
            showMessage("Invalid folder name", "Choose a simple folder name without slashes.")
            return
        }
        let destination = rootURL.appendingPathComponent(name, isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: false)
            refresh()
        } catch {
            showError("Couldn’t create the folder", error)
        }
    }

    func reveal(_ item: WorkspaceItem) {
        NSWorkspace.shared.activateFileViewerSelecting([item.url])
    }

    private func loadChildren(of directory: URL, depth: Int) -> [WorkspaceItem] {
        guard depth < 12 else { return [] }
        let keys: [URLResourceKey] = [.isDirectoryKey, .isRegularFileKey, .isHiddenKey, .isSymbolicLinkKey]
        guard let urls = try? FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: keys,
            options: [.skipsHiddenFiles]
        ) else { return [] }

        return urls.compactMap { url in
            guard let values = try? url.resourceValues(forKeys: Set(keys)),
                  values.isHidden != true,
                  values.isSymbolicLink != true
            else { return nil }
            if values.isDirectory == true {
                guard !Self.ignoredDirectories.contains(url.lastPathComponent) else { return nil }
                return WorkspaceItem(
                    url: url,
                    name: url.lastPathComponent,
                    isDirectory: true,
                    children: loadChildren(of: url, depth: depth + 1)
                )
            }
            guard values.isRegularFile == true,
                  Self.markdownExtensions.contains(url.pathExtension.lowercased())
            else { return nil }
            return WorkspaceItem(url: url, name: url.lastPathComponent, isDirectory: false, children: nil)
        }
        .sorted {
            if $0.isDirectory != $1.isDirectory { return $0.isDirectory }
            return $0.name.localizedStandardCompare($1.name) == .orderedAscending
        }
    }

    private func filtered(_ item: WorkspaceItem, matching term: String) -> WorkspaceItem? {
        if !item.isDirectory {
            return item.name.lowercased().contains(term) ? item : nil
        }
        if item.name.lowercased().contains(term) { return item }
        let children = item.children?.compactMap { filtered($0, matching: term) } ?? []
        guard !children.isEmpty else { return nil }
        return WorkspaceItem(url: item.url, name: item.name, isDirectory: true, children: children)
    }

    private func showError(_ title: String, _ error: Error) {
        let alert = NSAlert(error: error)
        alert.messageText = title
        alert.runModal()
    }

    private func showMessage(_ title: String, _ message: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.runModal()
    }
}
