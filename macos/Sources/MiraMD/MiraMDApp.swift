import SwiftUI

@main
struct MiraMDApp: App {
    @StateObject private var document = MarkdownDocument()
    @StateObject private var editor = EditorController()
    @StateObject private var workspace = WorkspaceModel()
    @StateObject private var appearance = AppearanceController()

    init() {
        AppFonts.register()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(document)
                .environmentObject(editor)
                .environmentObject(workspace)
                .environmentObject(appearance)
                .preferredColorScheme(appearance.selection.colorScheme)
                .frame(minWidth: 840, minHeight: 560)
                .onOpenURL { url in
                    document.open(url)
                }
                .task {
                    guard let path = CommandLine.arguments.dropFirst().first,
                          ["md", "markdown", "mdown", "mkd"].contains(URL(fileURLWithPath: path).pathExtension.lowercased())
                    else { return }
                    document.open(URL(fileURLWithPath: path))
                }
        }
        .defaultSize(width: 1180, height: 760)
        .commands {
            MiraMDCommands(document: document, editor: editor, appearance: appearance)
        }
    }
}
