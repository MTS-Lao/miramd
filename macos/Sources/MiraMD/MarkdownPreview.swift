import AppKit
import SwiftUI
import WebKit

struct MarkdownPreview: NSViewRepresentable {
    @Environment(\.colorScheme) private var colorScheme
    let fragment: String

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = context.coordinator
        webView.setValue(false, forKey: "drawsBackground")
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        webView.appearance = NSAppearance(
            named: colorScheme == .dark ? .darkAqua : .aqua
        )
        let document = Self.htmlDocument(fragment: fragment, dark: colorScheme == .dark)
        guard document != context.coordinator.lastDocument else { return }
        context.coordinator.lastDocument = document
        webView.loadHTMLString(document, baseURL: AppFonts.resourceBundle.resourceURL)
    }

    static func htmlDocument(fragment: String, editable: Bool = false, dark: Bool) -> String {
        let hasLao = fragment.unicodeScalars.contains { (0x0E80...0x0EFF).contains(Int($0.value)) }
        let language = hasLao ? "lo" : "en"
        let editorAttributes = editable
            ? #" contenteditable="true" spellcheck="true" role="textbox" aria-label="Visual Markdown editor""#
            : ""
        let editorScript = editable ? visualEditorScript : ""
        return """
        <!doctype html>
        <html lang="\(language)"><head><meta charset="utf-8"><meta name="color-scheme" content="light dark">
        <style>
        @font-face { font-family: 'Phetsarath Lao'; src: url('Fonts/PhetsarathLao-Regular.woff2') format('woff2'); font-weight: 400; font-style: normal; font-display: swap; unicode-range: U+0E80-0EFF; }
        @font-face { font-family: 'Phetsarath Lao'; src: url('Fonts/PhetsarathLao-SemiBold.woff2') format('woff2'); font-weight: 600; font-style: normal; font-display: swap; unicode-range: U+0E80-0EFF; }
        @font-face { font-family: 'Phetsarath Lao'; src: url('Fonts/PhetsarathLao-Bold.woff2') format('woff2'); font-weight: 700 900; font-style: normal; font-display: swap; unicode-range: U+0E80-0EFF; }
        \(MiraCore.syntaxHighlightCSS)
        :root { color-scheme: light dark; font: -apple-system-body; }
        body { max-width: 760px; margin: 0 auto; padding: 32px 38px 80px; font-family: 'Phetsarath Lao', -apple-system, BlinkMacSystemFont, sans-serif; line-height: \(hasLao ? "1.9" : "1.65"); color: #202124; overflow-wrap: normal; word-break: normal; }
        h1, h2, h3, h4 { line-height: 1.25; margin-top: 1.6em; margin-bottom: .55em; color: #111; }
        h1 { font-size: 2em; border-bottom: 1px solid #d7d9dc; padding-bottom: .3em; }
        h2 { font-size: 1.5em; border-bottom: 1px solid #e3e5e8; padding-bottom: .25em; }
        a { color: #1769e0; text-decoration: none; } a:hover { text-decoration: underline; }
        mark { color: inherit; background: #ffe082; padding: .06em .18em; border-radius: .22em; box-decoration-break: clone; -webkit-box-decoration-break: clone; }
        blockquote { margin: 1.2em 0; padding: .2em 1em; color: #52565d; border-left: 4px solid #8b5cf6; background: rgba(139,92,246,.07); }
        code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .9em; background: rgba(127,127,127,.14); border-radius: 5px; padding: .15em .35em; }
        pre { padding: 16px; overflow: auto; background: rgba(127,127,127,.12); border: 1px solid rgba(127,127,127,.18); border-radius: 9px; }
        pre.syntax-code { position: relative; padding-top: 2.35em; }
        pre.syntax-code[data-language]::before { content: attr(data-language); position: absolute; top: .45em; right: .75em; color: rgba(127,127,127,.82); font: 600 11px/1.4 -apple-system, BlinkMacSystemFont, sans-serif; letter-spacing: .04em; text-transform: uppercase; }
        pre code { padding: 0; background: none; }
        table { border-collapse: collapse; width: 100%; margin: 1em 0; }
        th, td { border: 1px solid #cfd2d7; padding: 8px 11px; text-align: left; } th { background: rgba(127,127,127,.1); }
        img { max-width: 100%; height: auto; border-radius: 8px; }
        li { margin: .25em 0; } input[type=checkbox] { margin-right: .5em; }
        hr { border: 0; border-top: 1px solid #d7d9dc; margin: 2em 0; }
        body[contenteditable="true"] { min-height: calc(100vh - 112px); outline: none; caret-color: #1769e0; }
        body[contenteditable="true"]:focus { outline: none; }
        @media (prefers-color-scheme: dark) {
          body { color: #e7e9ed; } h1, h2, h3, h4 { color: #f6f7f9; }
          h1, h2, hr { border-color: #454850; } a { color: #78a9ff; }
          blockquote { color: #c3c6cd; }
          mark { background: #806515; color: #fff5bf; }
          th, td { border-color: #4b4e56; }
        }
        </style></head><body class="syntax-\(dark ? "dark" : "light")"\(editorAttributes)>\(fragment)\(editorScript)</body></html>
        """
    }

    private static let visualEditorScript = #"""
    <script>
    (() => {
      const root = document.body;
      let changeTimer = null;
      let savedRange = null;

      const escapeText = value => value
        .replace(/\\/g, '\\\\')
        .replace(/([*_`\[\]])/g, '\\$1')
        .replace(/==(?=\S)/g, '\\==');

      const inlineChildren = node => Array.from(node.childNodes).map(inlineMarkdown).join('');

      function inlineMarkdown(node) {
        if (node.nodeType === Node.TEXT_NODE) return escapeText(node.nodeValue || '');
        if (node.nodeType !== Node.ELEMENT_NODE) return '';
        const tag = node.tagName.toLowerCase();
        if (tag === 'script' || tag === 'style') return '';
        if (tag === 'br') return '  \n';
        if (tag === 'input' && node.type === 'checkbox') return node.checked ? '[x] ' : '[ ] ';
        if (tag === 'img') return `![${node.alt || ''}](${node.getAttribute('src') || ''})`;
        const value = inlineChildren(node);
        if (tag === 'strong' || tag === 'b') return `**${value}**`;
        if (tag === 'em' || tag === 'i') return `_${value}_`;
        if (tag === 'del' || tag === 's') return `~~${value}~~`;
        if (tag === 'mark' || (tag === 'span' && node.style.backgroundColor)) return `==${value}==`;
        if (tag === 'code' && node.parentElement?.tagName.toLowerCase() !== 'pre') {
          const raw = node.textContent || '';
          const ticks = '`'.repeat(Math.max(1, ...Array.from(raw.matchAll(/`+/g), match => match[0].length + 1)));
          return `${ticks}${raw}${ticks}`;
        }
        if (tag === 'a') return `[${value}](${node.getAttribute('href') || ''})`;
        return value;
      }

      function listMarkdown(list, depth) {
        const ordered = list.tagName.toLowerCase() === 'ol';
        let number = Number(list.getAttribute('start') || 1);
        let result = '';
        for (const item of Array.from(list.children).filter(child => child.tagName.toLowerCase() === 'li')) {
          const nested = Array.from(item.children).filter(child => ['ul', 'ol'].includes(child.tagName.toLowerCase()));
          const content = Array.from(item.childNodes)
            .filter(child => !nested.includes(child))
            .map(inlineMarkdown)
            .join('')
            .trim();
          result += `${'  '.repeat(depth)}${ordered ? `${number++}.` : '-'} ${content}\n`;
          for (const child of nested) result += listMarkdown(child, depth + 1);
        }
        return result;
      }

      function tableMarkdown(table) {
        const rows = Array.from(table.rows).map(row =>
          Array.from(row.cells).map(cell => inlineChildren(cell).replace(/\|/g, '\\|').replace(/\n/g, ' ').trim())
        );
        if (!rows.length) return '';
        const columns = Math.max(...rows.map(row => row.length));
        const normalized = rows.map(row => Array.from({length: columns}, (_, index) => row[index] || ''));
        const lines = [`| ${normalized[0].join(' | ')} |`, `| ${Array(columns).fill('---').join(' | ')} |`];
        for (const row of normalized.slice(1)) lines.push(`| ${row.join(' | ')} |`);
        return lines.join('\n') + '\n\n';
      }

      function blockMarkdown(node, depth = 0) {
        if (node.nodeType === Node.TEXT_NODE) return escapeText(node.nodeValue || '');
        if (node.nodeType !== Node.ELEMENT_NODE) return '';
        const tag = node.tagName.toLowerCase();
        if (tag === 'script' || tag === 'style') return '';
        if (/^h[1-6]$/.test(tag)) return `${'#'.repeat(Number(tag[1]))} ${inlineChildren(node).trim()}\n\n`;
        if (tag === 'p' || tag === 'div') return `${inlineChildren(node).trimEnd()}\n\n`;
        if (tag === 'ul' || tag === 'ol') return `${listMarkdown(node, depth)}\n`;
        if (tag === 'blockquote') {
          const content = Array.from(node.childNodes).map(child => blockMarkdown(child, depth)).join('').trim();
          return content.split('\n').map(line => `> ${line}`).join('\n') + '\n\n';
        }
        if (tag === 'pre') {
          const value = node.textContent || '';
          const code = node.querySelector(':scope > code');
          const language = Array.from(code?.classList || [])
            .find(name => name.startsWith('language-'))?.slice('language-'.length) || '';
          const ticks = '`'.repeat(Math.max(3, ...Array.from(value.matchAll(/`+/g), match => match[0].length + 1)));
          return `${ticks}${language}\n${value.replace(/\n$/, '')}\n${ticks}\n\n`;
        }
        if (tag === 'table') return tableMarkdown(node);
        if (tag === 'hr') return '---\n\n';
        return inlineChildren(node);
      }

      function markdown() {
        return Array.from(root.childNodes)
          .map(node => blockMarkdown(node))
          .join('')
          .replace(/\n{3,}/g, '\n\n')
          .trimEnd() + '\n';
      }

      function emitChange() {
        clearTimeout(changeTimer);
        changeTimer = setTimeout(() => window.webkit.messageHandlers.miramd.postMessage(markdown()), 100);
      }

      function restoreSelection() {
        if (!savedRange) return;
        const selection = window.getSelection();
        selection.removeAllRanges();
        selection.addRange(savedRange);
      }

      function insertHTML(value) {
        document.execCommand('insertHTML', false, value);
      }

      function wrapSelection(tag, placeholder) {
        const selection = window.getSelection();
        const range = selection.rangeCount ? selection.getRangeAt(0) : null;
        if (!range || range.collapsed) {
          insertHTML(`<${tag}>${placeholder}</${tag}>`);
          return;
        }
        const wrapper = document.createElement(tag);
        try {
          range.surroundContents(wrapper);
          selection.removeAllRanges();
          const next = document.createRange();
          next.selectNodeContents(wrapper);
          selection.addRange(next);
        } catch (_) {
          const text = selection.toString().replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
          insertHTML(`<${tag}>${text}</${tag}>`);
        }
      }

      document.addEventListener('selectionchange', () => {
        const selection = window.getSelection();
        if (selection.rangeCount && root.contains(selection.anchorNode)) savedRange = selection.getRangeAt(0).cloneRange();
      });
      root.addEventListener('input', emitChange);
      root.addEventListener('click', event => { if (event.target.closest('a')) event.preventDefault(); });

      window.miramdCommand = (action, value) => {
        root.focus();
        restoreSelection();
        switch (action) {
          case 'bold': document.execCommand('bold'); break;
          case 'italic': document.execCommand('italic'); break;
          case 'highlight': wrapSelection('mark', 'highlighted text'); break;
          case 'heading': document.execCommand('formatBlock', false, `h${value}`); break;
          case 'bullet': document.execCommand('insertUnorderedList'); break;
          case 'number': document.execCommand('insertOrderedList'); break;
          case 'task': insertHTML('<ul><li><input type="checkbox">Task</li></ul>'); break;
          case 'link':
            if (window.getSelection()?.isCollapsed) insertHTML('<a href="https://example.com">link text</a>');
            else document.execCommand('createLink', false, 'https://example.com');
            break;
          case 'code': wrapSelection('code', 'code'); break;
          case 'codeblock': document.execCommand('formatBlock', false, 'pre'); break;
          case 'quote': document.execCommand('formatBlock', false, 'blockquote'); break;
          case 'divider': document.execCommand('insertHorizontalRule'); break;
        }
        emitChange();
      };
    })();
    </script>
    """#

    @MainActor
    final class Coordinator: NSObject, WKNavigationDelegate {
        var lastDocument = ""

        func webView(
            _ webView: WKWebView,
            decidePolicyFor navigationAction: WKNavigationAction,
            decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void
        ) {
            if navigationAction.navigationType == .linkActivated,
               let url = navigationAction.request.url,
               ["http", "https", "mailto"].contains(url.scheme?.lowercased() ?? "") {
                NSWorkspace.shared.open(url)
                decisionHandler(.cancel)
            } else {
                decisionHandler(.allow)
            }
        }
    }
}
