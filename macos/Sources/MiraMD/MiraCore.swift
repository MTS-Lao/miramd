import Foundation

@_silgen_name("miramd_string_free")
private func miramd_string_free(_ value: UnsafeMutablePointer<CChar>?)

@_silgen_name("miramd_engine_version")
private func miramd_engine_version() -> UnsafeMutablePointer<CChar>?

@_silgen_name("miramd_syntax_highlight_css")
private func miramd_syntax_highlight_css() -> UnsafeMutablePointer<CChar>?

@_silgen_name("miramd_render_html")
private func miramd_render_html(_ markdown: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("miramd_outline_json")
private func miramd_outline_json(_ markdown: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("miramd_stats_json")
private func miramd_stats_json(_ markdown: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("miramd_export_ai")
private func miramd_export_ai(
    _ markdown: UnsafePointer<CChar>,
    _ filename: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar>?

@_silgen_name("miramd_clean_markdown")
private func miramd_clean_markdown(_ markdown: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("miramd_import_docx")
private func miramd_import_docx(_ path: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("miramd_export_docx")
private func miramd_export_docx(
    _ markdown: UnsafePointer<CChar>,
    _ title: UnsafePointer<CChar>,
    _ path: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar>?

struct OutlineItem: Codable, Hashable, Identifiable {
    let level: Int
    let text: String
    let line: Int

    var id: String { "\(line)-\(level)-\(text)" }
}

struct DocumentStats: Codable, Equatable {
    let words: Int
    let characters: Int
    let lines: Int
    let readingMinutes: Int

    enum CodingKeys: String, CodingKey {
        case words, characters, lines
        case readingMinutes = "reading_minutes"
    }

    static let empty = DocumentStats(words: 0, characters: 0, lines: 0, readingMinutes: 0)
}

struct WordImportResult: Codable {
    let markdown: String
    let title: String
    let warnings: [String]
    let error: String
}

enum MiraCore {
    static var version: String { decode(miramd_engine_version()) ?? "unknown" }
    static let syntaxHighlightCSS = decode(miramd_syntax_highlight_css()) ?? ""

    static func render(_ markdown: String) -> String {
        call(markdown, miramd_render_html) ?? ""
    }

    static func outline(_ markdown: String) -> [OutlineItem] {
        guard let json = call(markdown, miramd_outline_json),
              let data = json.data(using: .utf8)
        else { return [] }
        return (try? JSONDecoder().decode([OutlineItem].self, from: data)) ?? []
    }

    static func stats(_ markdown: String) -> DocumentStats {
        guard let json = call(markdown, miramd_stats_json),
              let data = json.data(using: .utf8)
        else { return .empty }
        return (try? JSONDecoder().decode(DocumentStats.self, from: data)) ?? .empty
    }

    static func exportForAI(_ markdown: String, filename: String) -> String {
        let pointer = markdown.withCString { markdownPointer in
            filename.withCString { filenamePointer in
                miramd_export_ai(markdownPointer, filenamePointer)
            }
        }
        return decode(pointer) ?? markdown
    }

    static func cleanMarkdown(_ markdown: String) -> String {
        call(markdown, miramd_clean_markdown) ?? markdown
    }

    static func importWord(from url: URL) throws -> WordImportResult {
        guard let json = call(url.path, miramd_import_docx),
              let data = json.data(using: .utf8),
              let result = try? JSONDecoder().decode(WordImportResult.self, from: data)
        else {
            throw NSError(
                domain: "MiraMD.WordImport",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "The Word importer did not return a readable result."]
            )
        }
        if !result.error.isEmpty {
            throw NSError(
                domain: "MiraMD.WordImport",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: result.error]
            )
        }
        return result
    }

    static func exportWord(_ markdown: String, title: String, to url: URL) throws {
        let pointer = markdown.withCString { markdownPointer in
            title.withCString { titlePointer in
                url.path.withCString { pathPointer in
                    miramd_export_docx(markdownPointer, titlePointer, pathPointer)
                }
            }
        }
        guard let message = decode(pointer) else {
            throw NSError(
                domain: "MiraMD.WordExport",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "The Rust document engine did not return a result."]
            )
        }
        if !message.isEmpty {
            throw NSError(
                domain: "MiraMD.WordExport",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: message]
            )
        }
    }

    private static func call(
        _ value: String,
        _ operation: (UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?
    ) -> String? {
        decode(value.withCString(operation))
    }

    private static func decode(_ pointer: UnsafeMutablePointer<CChar>?) -> String? {
        guard let pointer else { return nil }
        defer { miramd_string_free(pointer) }
        return String(validatingCString: pointer)
    }
}
