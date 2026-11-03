import SwiftUI

struct MiraMDCommands: Commands {
    let document: MarkdownDocument
    let editor: EditorController
    let appearance: AppearanceController

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New") { document.newDocument() }
                .keyboardShortcut("n")
            Button("Open…") { document.showOpenPanel() }
                .keyboardShortcut("o")
        }
        CommandGroup(replacing: .saveItem) {
            Button("Save") { document.save() }
                .keyboardShortcut("s")
            Button("Save As…") { document.saveAs() }
                .keyboardShortcut("s", modifiers: [.command, .shift])
        }
        CommandMenu("Format") {
            Button("Bold") { editor.perform(.bold) }.keyboardShortcut("b")
            Button("Italic") { editor.perform(.italic) }.keyboardShortcut("i")
            Button("Highlight") { editor.perform(.highlight) }
            Divider()
            Button("Heading 1") { editor.perform(.heading(1)) }
            Button("Heading 2") { editor.perform(.heading(2)) }
            Button("Heading 3") { editor.perform(.heading(3)) }
            Divider()
            Button("Bulleted List") { editor.perform(.bulletedList) }
            Button("Numbered List") { editor.perform(.numberedList) }
            Button("Task") { editor.perform(.task) }
            Button("Quote") { editor.perform(.quote) }
            Button("Link") { editor.perform(.link) }.keyboardShortcut("k")
            Button("Inline Code") { editor.perform(.inlineCode) }
            Button("Code Block") { editor.perform(.codeBlock) }
        }
        CommandMenu("Export") {
            Button("Copy for AI") { document.copyForAI() }
                .keyboardShortcut("c", modifiers: [.command, .option])
            Button("Export for AI…") { document.exportForAI() }
            Button("Export Clean Markdown…") { document.exportCleanMarkdown() }
            Divider()
            Button("Export to Microsoft Word…") { document.exportWord() }
        }
        CommandMenu("AI") {
            Button("Copy MCP Setup…") { document.copyMCPSetup() }
            Divider()
            Button("Copy Document for AI") { document.copyForAI() }
        }
        CommandMenu("Appearance") {
            ForEach(AppAppearance.allCases) { option in
                Button {
                    appearance.selection = option
                } label: {
                    if appearance.selection == option {
                        Label(option.title, systemImage: "checkmark")
                    } else {
                        Text(option.title)
                    }
                }
            }
        }
    }
}
