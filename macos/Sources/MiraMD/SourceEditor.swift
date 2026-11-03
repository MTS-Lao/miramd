import AppKit
import SwiftUI

struct SourceEditor: NSViewRepresentable {
    @Binding var text: String
    @ObservedObject var controller: EditorController

    func makeCoordinator() -> Coordinator { Coordinator(text: $text) }

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false

        let textView = NSTextView()
        textView.delegate = context.coordinator
        textView.isRichText = false
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isAutomaticSpellingCorrectionEnabled = false
        textView.allowsUndo = true
        textView.usesFindPanel = true
        textView.isIncrementalSearchingEnabled = true
        textView.font = .monospacedSystemFont(ofSize: 14, weight: .regular)
        textView.textContainerInset = NSSize(width: 22, height: 22)
        textView.textContainer?.widthTracksTextView = true
        textView.string = text
        textView.backgroundColor = .textBackgroundColor
        scrollView.documentView = textView
        context.coordinator.applyDisplayFonts(to: textView)
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let textView = scrollView.documentView as? NSTextView else { return }
        if textView.string != text {
            let selection = textView.selectedRange()
            textView.string = text
            textView.setSelectedRange(NSRange(location: min(selection.location, text.utf16.count), length: 0))
            context.coordinator.applyDisplayFonts(to: textView)
        }

        if let request = controller.request, request.id != context.coordinator.lastRequestID {
            context.coordinator.lastRequestID = request.id
            context.coordinator.apply(request.action, to: textView)
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        @Binding private var text: String
        var lastRequestID: UUID?

        init(text: Binding<String>) {
            _text = text
        }

        func textDidChange(_ notification: Notification) {
            guard let textView = notification.object as? NSTextView else { return }
            text = textView.string
            applyDisplayFonts(to: textView)
        }

        func applyDisplayFonts(to textView: NSTextView) {
            guard let storage = textView.textStorage else { return }
            let source = textView.string as NSString
            let fullRange = NSRange(location: 0, length: source.length)
            let englishFont = NSFont.monospacedSystemFont(ofSize: 14, weight: .regular)
            let laoFont = AppFonts.lao(size: 16)
            let paragraph = NSMutableParagraphStyle()
            paragraph.lineSpacing = 3
            paragraph.minimumLineHeight = 22

            storage.beginEditing()
            storage.addAttribute(.font, value: englishFont, range: fullRange)
            storage.addAttribute(.paragraphStyle, value: paragraph, range: fullRange)

            var laoStart: Int?
            for index in 0...source.length {
                let isLao = index < source.length && (0x0E80...0x0EFF).contains(Int(source.character(at: index)))
                if isLao, laoStart == nil {
                    laoStart = index
                } else if !isLao, let start = laoStart {
                    storage.addAttribute(.font, value: laoFont, range: NSRange(location: start, length: index - start))
                    laoStart = nil
                }
            }
            storage.endEditing()
            textView.typingAttributes[.font] = englishFont
            textView.typingAttributes[.paragraphStyle] = paragraph
        }

        func apply(_ action: EditorAction, to textView: NSTextView) {
            switch action {
            case .bold: wrap("**", "**", placeholder: "bold text", in: textView)
            case .italic: wrap("_", "_", placeholder: "emphasized text", in: textView)
            case .highlight: wrap("==", "==", placeholder: "highlighted text", in: textView)
            case .link: wrap("[", "](https://example.com)", placeholder: "link text", in: textView)
            case .inlineCode: wrap("`", "`", placeholder: "code", in: textView)
            case .codeBlock: wrap("```\n", "\n```", placeholder: "code", in: textView)
            case let .heading(level): prefixLines(String(repeating: "#", count: level) + " ", stripHeadings: true, in: textView)
            case .bulletedList: prefixLines("- ", in: textView)
            case .numberedList: prefixNumberedLines(in: textView)
            case .task: prefixLines("- [ ] ", in: textView)
            case .quote: prefixLines("> ", in: textView)
            case .divider: insert("\n\n---\n\n", in: textView)
            case let .goToLine(line): goToLine(line, in: textView)
            }
        }

        private func wrap(_ prefix: String, _ suffix: String, placeholder: String, in textView: NSTextView) {
            let selection = textView.selectedRange()
            let source = textView.string as NSString
            let selected = selection.length > 0 ? source.substring(with: selection) : placeholder
            replace(selection, with: prefix + selected + suffix, in: textView)
            let start = selection.location + prefix.utf16.count
            textView.setSelectedRange(NSRange(location: start, length: selected.utf16.count))
        }

        private func insert(_ value: String, in textView: NSTextView) {
            let selection = textView.selectedRange()
            replace(selection, with: value, in: textView)
            textView.setSelectedRange(NSRange(location: selection.location + value.utf16.count, length: 0))
        }

        private func prefixLines(_ prefix: String, stripHeadings: Bool = false, in textView: NSTextView) {
            let source = textView.string as NSString
            let selectedLineRange = source.lineRange(for: textView.selectedRange())
            var lines = source.substring(with: selectedLineRange).split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
            if lines.last == "" { lines.removeLast() }
            lines = lines.map { line in
                let cleaned: String
                if stripHeadings {
                    cleaned = line.replacingOccurrences(of: #"^#{1,6}\s+"#, with: "", options: .regularExpression)
                } else {
                    cleaned = line
                }
                return prefix + cleaned
            }
            let trailingNewline = selectedLineRange.length > 0 && source.substring(with: selectedLineRange).hasSuffix("\n")
            let replacement = lines.joined(separator: "\n") + (trailingNewline ? "\n" : "")
            replace(selectedLineRange, with: replacement, in: textView)
            textView.setSelectedRange(NSRange(location: selectedLineRange.location, length: replacement.utf16.count))
        }

        private func prefixNumberedLines(in textView: NSTextView) {
            let source = textView.string as NSString
            let range = source.lineRange(for: textView.selectedRange())
            var lines = source.substring(with: range).split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
            if lines.last == "" { lines.removeLast() }
            let replacement = lines.enumerated().map { "\($0.offset + 1). \($0.element)" }.joined(separator: "\n")
                + (source.substring(with: range).hasSuffix("\n") ? "\n" : "")
            replace(range, with: replacement, in: textView)
            textView.setSelectedRange(NSRange(location: range.location, length: replacement.utf16.count))
        }

        private func replace(_ range: NSRange, with replacement: String, in textView: NSTextView) {
            guard textView.shouldChangeText(in: range, replacementString: replacement) else { return }
            textView.textStorage?.replaceCharacters(in: range, with: replacement)
            textView.didChangeText()
        }

        private func goToLine(_ line: Int, in textView: NSTextView) {
            guard line > 0 else { return }
            var currentLine = 1
            var location = 0
            for character in textView.string.utf16 {
                if currentLine == line { break }
                location += 1
                if character == 10 { currentLine += 1 }
            }
            textView.setSelectedRange(NSRange(location: location, length: 0))
            textView.scrollRangeToVisible(NSRange(location: location, length: 0))
            textView.window?.makeFirstResponder(textView)
        }
    }
}
