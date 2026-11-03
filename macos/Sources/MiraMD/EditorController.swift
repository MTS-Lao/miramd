import Foundation

enum EditorAction: Equatable {
    case bold
    case italic
    case highlight
    case heading(Int)
    case bulletedList
    case numberedList
    case task
    case link
    case inlineCode
    case codeBlock
    case quote
    case divider
    case goToLine(Int)
}

struct EditorRequest: Equatable {
    let id = UUID()
    let action: EditorAction
}

@MainActor
final class EditorController: ObservableObject {
    @Published private(set) var request: EditorRequest?

    func perform(_ action: EditorAction) {
        request = EditorRequest(action: action)
    }
}
