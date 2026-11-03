using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace MiraMD.Windows;

internal sealed record OutlineItem(
    [property: JsonPropertyName("level")] int Level,
    [property: JsonPropertyName("text")] string Text,
    [property: JsonPropertyName("line")] int Line);

internal sealed record DocumentStats(
    [property: JsonPropertyName("words")] int Words,
    [property: JsonPropertyName("characters")] int Characters,
    [property: JsonPropertyName("lines")] int Lines,
    [property: JsonPropertyName("reading_minutes")] int ReadingMinutes)
{
    internal static DocumentStats Empty { get; } = new(0, 0, 0, 0);
}

internal static partial class NativeCore
{
    [LibraryImport("miramd_core", EntryPoint = "miramd_render_html", StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr RenderHtmlNative(string markdown);

    [LibraryImport("miramd_core", EntryPoint = "miramd_syntax_highlight_css")]
    private static partial IntPtr SyntaxHighlightCssNative();

    [LibraryImport("miramd_core", EntryPoint = "miramd_outline_json", StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr OutlineNative(string markdown);

    [LibraryImport("miramd_core", EntryPoint = "miramd_stats_json", StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr StatsNative(string markdown);

    [LibraryImport("miramd_core", EntryPoint = "miramd_export_ai", StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ExportAiNative(string markdown, string filename);

    [LibraryImport("miramd_core", EntryPoint = "miramd_clean_markdown", StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr CleanMarkdownNative(string markdown);

    [LibraryImport("miramd_core", EntryPoint = "miramd_export_docx", StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ExportDocxNative(string markdown, string title, string path);

    [LibraryImport("miramd_core", EntryPoint = "miramd_string_free")]
    private static partial void FreeNative(IntPtr value);

    internal static string Render(string markdown) => Take(RenderHtmlNative(markdown));
    internal static string SyntaxHighlightCss { get; } = Take(SyntaxHighlightCssNative());

    internal static IReadOnlyList<OutlineItem> Outline(string markdown)
    {
        var json = Take(OutlineNative(markdown));
        return JsonSerializer.Deserialize<OutlineItem[]>(json) ?? [];
    }

    internal static DocumentStats Stats(string markdown)
    {
        var json = Take(StatsNative(markdown));
        return JsonSerializer.Deserialize<DocumentStats>(json) ?? DocumentStats.Empty;
    }

    internal static string ExportForAi(string markdown, string filename) =>
        Take(ExportAiNative(markdown, filename));

    internal static string CleanMarkdown(string markdown) => Take(CleanMarkdownNative(markdown));

    internal static void ExportDocx(string markdown, string title, string path)
    {
        var error = Take(ExportDocxNative(markdown, title, path));
        if (!string.IsNullOrEmpty(error))
        {
            throw new InvalidOperationException(error);
        }
    }

    private static string Take(IntPtr pointer)
    {
        if (pointer == IntPtr.Zero)
        {
            throw new InvalidOperationException("The MiraMD engine could not process the document.");
        }

        try
        {
            return Marshal.PtrToStringUTF8(pointer) ?? string.Empty;
        }
        finally
        {
            FreeNative(pointer);
        }
    }
}
