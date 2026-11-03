import AppKit
import SwiftUI
import WebKit

struct VisualMarkdownEditor: NSViewRepresentable {
    @Environment(\.colorScheme) private var colorScheme
    @Binding var text: String
    let fragment: String
    @ObservedObject var controller: EditorController

    func makeCoordinator() -> Coordinator { Coordinator(text: $text) }

    func makeNSView(context: Context) -> WKWebView {
        let contentController = WKUserContentController()
        contentController.add(context.coordinator, name: "miramd")
        let configuration = WKWebViewConfiguration()
        configuration.userContentController = contentController
        configuration.defaultWebpagePreferences.allowsContentJavaScript = true
        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = context.coordinator
        webView.setValue(false, forKey: "drawsBackground")
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        webView.appearance = NSAppearance(
            named: colorScheme == .dark ? .darkAqua : .aqua
        )
        let document = MarkdownPreview.htmlDocument(fragment: fragment, editable: true)
        if context.coordinator.pendingWebMarkdown == text {
            context.coordinator.pendingWebMarkdown = nil
            context.coordinator.lastDocument = document
        } else if document != context.coordinator.lastDocument {
            context.coordinator.lastDocument = document
            webView.loadHTMLString(document, baseURL: AppFonts.resourceBundle.resourceURL)
        }

        if let request = controller.request, request.id != context.coordinator.lastRequestID {
            context.coordinator.lastRequestID = request.id
            guard let command = Self.command(for: request.action) else { return }
            webView.evaluateJavaScript("window.miramdCommand?.('\(command.name)', \(command.value));")
        }
    }

    private static func command(for action: EditorAction) -> (name: String, value: String)? {
        switch action {
        case .bold: ("bold", "null")
        case .italic: ("italic", "null")
        case .highlight: ("highlight", "null")
        case let .heading(level): ("heading", String(level))
        case .bulletedList: ("bullet", "null")
        case .numberedList: ("number", "null")
        case .task: ("task", "null")
        case .link: ("link", "null")
        case .inlineCode: ("code", "null")
        case .codeBlock: ("codeblock", "null")
        case .quote: ("quote", "null")
        case .divider: ("divider", "null")
        case .goToLine: nil
        }
    }

    @MainActor
    final class Coordinator: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
        @Binding private var text: String
        var pendingWebMarkdown: String?
        var lastDocument = ""
        var lastRequestID: UUID?

        init(text: Binding<String>) {
            _text = text
        }

        func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
            guard message.name == "miramd", let markdown = message.body as? String else { return }
            pendingWebMarkdown = markdown
            if text != markdown { text = markdown }
        }

        func webView(
            _ webView: WKWebView,
            decidePolicyFor navigationAction: WKNavigationAction,
            decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void
        ) {
            if navigationAction.navigationType == .linkActivated {
                decisionHandler(.cancel)
            } else {
                decisionHandler(.allow)
            }
        }
    }
}
