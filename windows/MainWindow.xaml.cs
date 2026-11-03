using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;
using Windows.Storage.Pickers;
using Windows.System;

namespace MiraMD.Windows;

public sealed partial class MainWindow : Window
{
    private static readonly Lazy<string> PhetsarathLaoCss = new(BuildPhetsarathLaoCss);
    private static readonly HashSet<string> MarkdownExtensions = new(StringComparer.OrdinalIgnoreCase)
        { ".md", ".markdown", ".mdown", ".mkd" };
    private static readonly HashSet<string> IgnoredDirectories = new(StringComparer.OrdinalIgnoreCase)
        { ".git", ".build", ".swiftpm", "target", "node_modules", "dist", "bin", "obj" };
    private static readonly string SettingsFilePath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "MiraMD",
        "settings.json"
    );
    private const string StarterDocument = """
        # Welcome to MiraMD

        MiraMD makes Markdown feel like a normal document while keeping the clean text that AI tools understand.

        ## Start writing

        Use the formatting bar to create **bold text**, *emphasis*, links, lists, quotes, and code without memorizing Markdown.

        - Read in a polished preview
        - Edit with familiar controls
        - Export a paste-safe copy for an AI conversation

        > Tip: Choose **Preview** when you only want to read.
        """;

    private StorageFile? _currentFile;
    private readonly AppSettings _settings = LoadSettings();
    private string? _workspacePath;
    private bool _isDirty;
    private bool _isLoading;
    private bool _isReceivingVisualEdit;
    private bool _isRestoringSettings = true;
    private string _viewMode = "visual";

    public MainWindow(string? initialPath = null)
    {
        InitializeComponent();
        RestoreSettings();
        RootLayout.ActualThemeChanged += (_, _) => RefreshDocument();
        Title = "MiraMD — Untitled.md";
        LoadDocument(StarterDocument);
        _isRestoringSettings = false;
        if (initialPath is not null)
        {
            _ = OpenInitialFileAsync(initialPath);
        }
    }

    private string DisplayName => _currentFile?.Name ?? "Untitled.md";

    private async Task OpenInitialFileAsync(string path)
    {
        try
        {
            _currentFile = await StorageFile.GetFileFromPathAsync(Path.GetFullPath(path));
            LoadDocument(await FileIO.ReadTextAsync(_currentFile));
            AdoptWorkspaceForFile(_currentFile.Path);
        }
        catch
        {
            StatusText.Text = $"Could not open {path}";
        }
    }

    private async void New_Click(object sender, RoutedEventArgs e)
    {
        if (!await ConfirmDiscardAsync()) return;
        _currentFile = null;
        LoadDocument(StarterDocument);
    }

    private async void Open_Click(object sender, RoutedEventArgs e)
    {
        if (!await ConfirmDiscardAsync()) return;
        var picker = new FileOpenPicker();
        picker.FileTypeFilter.Add(".md");
        picker.FileTypeFilter.Add(".markdown");
        InitializePicker(picker);
        var file = await picker.PickSingleFileAsync();
        if (file is null) return;
        _currentFile = file;
        LoadDocument(await FileIO.ReadTextAsync(file));
        AdoptWorkspaceForFile(file.Path);
    }

    private async void Save_Click(object sender, RoutedEventArgs e) => await SaveAsync();

    private async Task<bool> SaveAsync()
    {
        var file = _currentFile;
        if (file is null)
        {
            var picker = new FileSavePicker { SuggestedFileName = "Untitled" };
            picker.FileTypeChoices.Add("Markdown document", [".md"]);
            InitializePicker(picker);
            file = await picker.PickSaveFileAsync();
            if (file is null) return false;
            _currentFile = file;
        }

        await FileIO.WriteTextAsync(file, Editor.Text);
        _isDirty = false;
        AdoptWorkspaceForFile(file.Path);
        RefreshWorkspace();
        UpdateTitle();
        return true;
    }

    private void Editor_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (_isLoading) return;
        _isDirty = true;
        RefreshDocument();
        UpdateTitle();
    }

    private void RefreshDocument()
    {
        try
        {
            var stats = NativeCore.Stats(Editor.Text);
            DocumentStatsText.Text = $"{stats.Words:N0} words\n{stats.Characters:N0} characters\n{stats.ReadingMinutes} min read";
            StatusText.Text = $"{stats.Words:N0} words   {stats.Lines:N0} lines   {stats.ReadingMinutes} min read";
            OutlineList.ItemsSource = NativeCore.Outline(Editor.Text);
            if (!_isReceivingVisualEdit)
            {
                Preview.NavigateToString(PreviewDocument(
                    NativeCore.Render(Editor.Text),
                    RootLayout.ActualTheme == ElementTheme.Dark,
                    _viewMode == "visual"
                ));
            }
        }
        catch (DllNotFoundException)
        {
            StatusText.Text = "Rust engine not found — run scripts/build-windows.ps1";
        }
    }

    private void LoadDocument(string value)
    {
        _isLoading = true;
        Editor.Text = value;
        _isLoading = false;
        _isDirty = false;
        RefreshDocument();
        UpdateTitle();
    }

    private void CopyForAi_Click(object sender, RoutedEventArgs e)
    {
        var package = new DataPackage();
        package.SetText(NativeCore.ExportForAi(Editor.Text, DisplayName));
        Clipboard.SetContent(package);
        StatusText.Text = "Copied an AI-ready version to the clipboard";
    }

    private async void ExportForAi_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FileSavePicker { SuggestedFileName = Path.GetFileNameWithoutExtension(DisplayName) + "-for-ai" };
        picker.FileTypeChoices.Add("Text file", [".txt"]);
        InitializePicker(picker);
        var file = await picker.PickSaveFileAsync();
        if (file is not null)
        {
            await FileIO.WriteTextAsync(file, NativeCore.ExportForAi(Editor.Text, DisplayName));
        }
    }

    private async void ExportCleanMarkdown_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FileSavePicker { SuggestedFileName = Path.GetFileNameWithoutExtension(DisplayName) + "-clean" };
        picker.FileTypeChoices.Add("Markdown document", [".md"]);
        InitializePicker(picker);
        var file = await picker.PickSaveFileAsync();
        if (file is not null)
        {
            await FileIO.WriteTextAsync(file, NativeCore.CleanMarkdown(Editor.Text));
            StatusText.Text = $"Exported clean Markdown to {file.Name}";
        }
    }

    private async void ExportWord_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FileSavePicker { SuggestedFileName = Path.GetFileNameWithoutExtension(DisplayName) };
        picker.FileTypeChoices.Add("Microsoft Word document", [".docx"]);
        InitializePicker(picker);
        var file = await picker.PickSaveFileAsync();
        if (file is null) return;
        try
        {
            NativeCore.ExportDocx(Editor.Text, DisplayName, file.Path);
            StatusText.Text = $"Exported {file.Name}";
        }
        catch (Exception error)
        {
            StatusText.Text = error.Message;
        }
    }

    private void Heading_Click(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string value } && int.TryParse(value, out var level))
        {
            if (TryVisualCommand("heading", level)) return;
            PrefixSelectedLines(new string('#', level) + " ", stripHeading: true);
        }
    }

    private void Prefix_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: string action }) return;
        if (TryVisualCommand(action switch
            {
                "bullet" => "bullet",
                "number" => "number",
                "task" => "task",
                "quote" => "quote",
                _ => action,
            })) return;
        switch (action)
        {
            case "bullet": PrefixSelectedLines("- "); break;
            case "task": PrefixSelectedLines("- [ ] "); break;
            case "quote": PrefixSelectedLines("> "); break;
            case "number": PrefixSelectedLines("1. "); break;
        }
    }

    private void Wrap_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: string action }) return;
        if (TryVisualCommand(action switch
            {
                "codeblock" => "codeblock",
                _ => action,
            })) return;
        switch (action)
        {
            case "bold": WrapSelection("**", "**", "bold text"); break;
            case "italic": WrapSelection("_", "_", "emphasized text"); break;
            case "highlight": WrapSelection("==", "==", "highlighted text"); break;
            case "link": WrapSelection("[", "](https://example.com)", "link text"); break;
            case "code": WrapSelection("`", "`", "code"); break;
            case "codeblock": WrapSelection("```\n", "\n```", "code"); break;
        }
    }

    private void WrapSelection(string prefix, string suffix, string placeholder)
    {
        var start = Editor.SelectionStart;
        var length = Editor.SelectionLength;
        var selected = length > 0 ? Editor.Text.Substring(start, length) : placeholder;
        Editor.Text = Editor.Text.Remove(start, length).Insert(start, prefix + selected + suffix);
        Editor.Select(start + prefix.Length, selected.Length);
        Editor.Focus(FocusState.Programmatic);
    }

    private void PrefixSelectedLines(string prefix, bool stripHeading = false)
    {
        var start = Editor.SelectionStart == 0
            ? 0
            : Editor.Text.LastIndexOf('\n', Editor.SelectionStart - 1) + 1;
        var selectionEnd = Editor.SelectionStart + Editor.SelectionLength;
        var end = Editor.Text.IndexOf('\n', selectionEnd);
        if (end < 0) end = Editor.Text.Length;
        var lines = Editor.Text[start..end].Split('\n');
        for (var index = 0; index < lines.Length; index++)
        {
            var line = stripHeading ? lines[index].TrimStart('#', ' ') : lines[index];
            lines[index] = prefix + line;
        }
        var replacement = string.Join('\n', lines);
        Editor.Text = Editor.Text.Remove(start, end - start).Insert(start, replacement);
        Editor.Select(start, replacement.Length);
        Editor.Focus(FocusState.Programmatic);
    }

    private void OutlineList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (OutlineList.SelectedItem is not OutlineItem item) return;
        var line = 1;
        var position = 0;
        while (position < Editor.Text.Length && line < item.Line)
        {
            if (Editor.Text[position++] == '\n') line++;
        }
        SetViewMode("edit");
        Editor.Select(position, 0);
        Editor.Focus(FocusState.Programmatic);
    }

    private void VisualMode_Click(object sender, RoutedEventArgs e) => SetViewMode("visual");
    private void EditMode_Click(object sender, RoutedEventArgs e) => SetViewMode("edit");
    private void SplitMode_Click(object sender, RoutedEventArgs e) => SetViewMode("split");
    private void PreviewMode_Click(object sender, RoutedEventArgs e) => SetViewMode("preview");

    private void ThemePicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        var selection = ThemePicker.SelectedIndex switch
        {
            1 => ("Light", ElementTheme.Light),
            2 => ("Dark", ElementTheme.Dark),
            _ => ("System", ElementTheme.Default),
        };
        RootLayout.RequestedTheme = selection.Item2;
        if (!_isRestoringSettings)
        {
            _settings.Theme = selection.Item1;
            SaveSettings();
        }
        RefreshDocument();
    }

    private async void ChooseWorkspace_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FolderPicker();
        picker.FileTypeFilter.Add("*");
        InitializePicker(picker);
        var folder = await picker.PickSingleFolderAsync();
        if (folder is not null) SetWorkspace(folder.Path);
    }

    private void RefreshWorkspace_Click(object sender, RoutedEventArgs e) => RefreshWorkspace();

    private void WorkspaceFilter_TextChanged(object sender, TextChangedEventArgs e) => RefreshWorkspace();

    private async void WorkspaceTree_ItemInvoked(TreeView sender, TreeViewItemInvokedEventArgs args)
    {
        var entry = args.InvokedItem switch
        {
            TreeViewNode { Content: WorkspaceEntry value } => value,
            WorkspaceEntry value => value,
            _ => null,
        };
        if (entry is null || entry.IsDirectory || !await ConfirmDiscardAsync()) return;
        try
        {
            var file = await StorageFile.GetFileFromPathAsync(entry.Path);
            _currentFile = file;
            LoadDocument(await FileIO.ReadTextAsync(file));
        }
        catch (Exception error)
        {
            StatusText.Text = error.Message;
        }
    }

    private async void NewWorkspaceFile_Click(object sender, RoutedEventArgs e)
    {
        if (_workspacePath is null)
        {
            ChooseWorkspace_Click(sender, e);
            return;
        }
        if (!await ConfirmDiscardAsync()) return;
        var input = new TextBox { PlaceholderText = "Example.md", Text = "Untitled.md" };
        var dialog = new ContentDialog
        {
            XamlRoot = Content.XamlRoot,
            Title = "New Markdown file",
            Content = input,
            PrimaryButtonText = "Create",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        var name = input.Text.Trim();
        if (Path.GetExtension(name).Length == 0) name += ".md";
        if (!IsSimpleName(name) || !MarkdownExtensions.Contains(Path.GetExtension(name)))
        {
            StatusText.Text = "Use a simple filename ending in .md or .markdown";
            return;
        }
        var path = Path.Combine(_workspacePath, name);
        if (File.Exists(path))
        {
            StatusText.Text = $"{name} already exists";
            return;
        }
        await File.WriteAllTextAsync(path, $"# {Path.GetFileNameWithoutExtension(name)}\n\n");
        RefreshWorkspace();
        _currentFile = await StorageFile.GetFileFromPathAsync(path);
        LoadDocument(await FileIO.ReadTextAsync(_currentFile));
    }

    private async void NewWorkspaceFolder_Click(object sender, RoutedEventArgs e)
    {
        if (_workspacePath is null)
        {
            ChooseWorkspace_Click(sender, e);
            return;
        }
        var input = new TextBox { PlaceholderText = "Folder name" };
        var dialog = new ContentDialog
        {
            XamlRoot = Content.XamlRoot,
            Title = "New folder",
            Content = input,
            PrimaryButtonText = "Create",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        var name = input.Text.Trim();
        if (!IsSimpleName(name))
        {
            StatusText.Text = "Choose a simple folder name";
            return;
        }
        try
        {
            Directory.CreateDirectory(Path.Combine(_workspacePath, name));
            RefreshWorkspace();
        }
        catch (Exception error)
        {
            StatusText.Text = error.Message;
        }
    }

    private void SetViewMode(string mode)
    {
        _viewMode = mode;
        VisualModeButton.IsChecked = mode == "visual";
        EditModeButton.IsChecked = mode == "edit";
        SplitModeButton.IsChecked = mode == "split";
        PreviewModeButton.IsChecked = mode == "preview";
        EditorColumn.Width = mode is "preview" or "visual" ? new GridLength(0) : new GridLength(1, GridUnitType.Star);
        PreviewColumn.Width = mode == "edit" ? new GridLength(0) : new GridLength(1, GridUnitType.Star);
        FormattingBar.Visibility = mode == "preview" ? Visibility.Collapsed : Visibility.Visible;
        RefreshDocument();
    }

    private bool TryVisualCommand(string action, int? value = null)
    {
        if (_viewMode != "visual") return false;
        var name = JsonSerializer.Serialize(action);
        var argument = value?.ToString() ?? "null";
        _ = Preview.ExecuteScriptAsync($"window.miramdCommand?.({name}, {argument});");
        return true;
    }

    private void Preview_WebMessageReceived(Microsoft.UI.Xaml.Controls.WebView2 sender, Microsoft.Web.WebView2.Core.CoreWebView2WebMessageReceivedEventArgs args)
    {
        if (_viewMode != "visual") return;
        var markdown = args.TryGetWebMessageAsString();
        if (Editor.Text == markdown) return;
        _isReceivingVisualEdit = true;
        Editor.Text = markdown;
        _isReceivingVisualEdit = false;
    }

    private async void Preview_NavigationStarting(Microsoft.UI.Xaml.Controls.WebView2 sender, Microsoft.Web.WebView2.Core.CoreWebView2NavigationStartingEventArgs args)
    {
        if (Uri.TryCreate(args.Uri, UriKind.Absolute, out var uri) && uri.Scheme is "http" or "https" or "mailto")
        {
            args.Cancel = true;
            await Launcher.LaunchUriAsync(uri);
        }
    }

    private async Task<bool> ConfirmDiscardAsync()
    {
        if (!_isDirty) return true;
        var dialog = new ContentDialog
        {
            XamlRoot = Content.XamlRoot,
            Title = $"Save changes to {DisplayName}?",
            Content = "Your changes will be lost if you don’t save them.",
            PrimaryButtonText = "Save",
            SecondaryButtonText = "Don’t Save",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
        };
        var result = await dialog.ShowAsync();
        return result switch
        {
            ContentDialogResult.Primary => await SaveAsync(),
            ContentDialogResult.Secondary => true,
            _ => false,
        };
    }

    private void UpdateTitle() => Title = $"MiraMD — {DisplayName}{(_isDirty ? " — Edited" : string.Empty)}";

    private void InitializePicker(object picker)
    {
        var handle = WinRT.Interop.WindowNative.GetWindowHandle(this);
        WinRT.Interop.InitializeWithWindow.Initialize(picker, handle);
    }

    private void RestoreSettings()
    {
        ThemePicker.SelectedIndex = _settings.Theme switch { "Light" => 1, "Dark" => 2, _ => 0 };
        if (_settings.WorkspacePath is string path && Directory.Exists(path))
        {
            SetWorkspace(path);
        }
    }

    private void SetWorkspace(string path)
    {
        _workspacePath = Path.GetFullPath(path);
        _settings.WorkspacePath = _workspacePath;
        SaveSettings();
        WorkspacePathText.Text = new DirectoryInfo(_workspacePath).Name;
        WorkspacePathText.Tag = _workspacePath;
        ToolTipService.SetToolTip(WorkspacePathText, _workspacePath);
        RefreshWorkspace();
    }

    private void AdoptWorkspaceForFile(string path)
    {
        if (_workspacePath is null && Path.GetDirectoryName(path) is string directory)
        {
            SetWorkspace(directory);
        }
    }

    private void RefreshWorkspace()
    {
        WorkspaceTree.RootNodes.Clear();
        if (_workspacePath is null || !Directory.Exists(_workspacePath)) return;
        try
        {
            var filter = WorkspaceFilter.Text.Trim();
            foreach (var node in BuildWorkspaceNodes(_workspacePath, filter, 0))
            {
                WorkspaceTree.RootNodes.Add(node);
            }
        }
        catch (Exception error)
        {
            StatusText.Text = $"Could not read library: {error.Message}";
        }
    }

    private static IReadOnlyList<TreeViewNode> BuildWorkspaceNodes(string directory, string filter, int depth)
    {
        if (depth >= 12) return [];
        var results = new List<TreeViewNode>();
        IEnumerable<string> directories;
        IEnumerable<string> files;
        try
        {
            directories = Directory.EnumerateDirectories(directory)
                .Where(path => !IgnoredDirectories.Contains(Path.GetFileName(path)) && !IsHiddenOrLink(path))
                .OrderBy(path => Path.GetFileName(path), StringComparer.CurrentCultureIgnoreCase)
                .ToArray();
            files = Directory.EnumerateFiles(directory)
                .Where(path => MarkdownExtensions.Contains(Path.GetExtension(path)) && !IsHiddenOrLink(path))
                .OrderBy(path => Path.GetFileName(path), StringComparer.CurrentCultureIgnoreCase)
                .ToArray();
        }
        catch
        {
            return results;
        }

        foreach (var path in directories)
        {
            var children = BuildWorkspaceNodes(path, filter, depth + 1);
            var name = Path.GetFileName(path);
            if (filter.Length > 0 && !name.Contains(filter, StringComparison.CurrentCultureIgnoreCase) && children.Count == 0) continue;
            var node = new TreeViewNode { Content = new WorkspaceEntry(path, name, true) };
            foreach (var child in children) node.Children.Add(child);
            results.Add(node);
        }
        foreach (var path in files)
        {
            var name = Path.GetFileName(path);
            if (filter.Length > 0 && !name.Contains(filter, StringComparison.CurrentCultureIgnoreCase)) continue;
            results.Add(new TreeViewNode { Content = new WorkspaceEntry(path, name, false) });
        }
        return results;
    }

    private static bool IsHiddenOrLink(string path)
    {
        try
        {
            var attributes = File.GetAttributes(path);
            return attributes.HasFlag(FileAttributes.Hidden) || attributes.HasFlag(FileAttributes.ReparsePoint);
        }
        catch
        {
            return true;
        }
    }

    private static bool IsSimpleName(string name) =>
        !string.IsNullOrWhiteSpace(name)
        && name is not "." and not ".."
        && name == Path.GetFileName(name)
        && name.IndexOfAny(Path.GetInvalidFileNameChars()) < 0;

    private static AppSettings LoadSettings()
    {
        try
        {
            return File.Exists(SettingsFilePath)
                ? JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(SettingsFilePath)) ?? new AppSettings()
                : new AppSettings();
        }
        catch
        {
            return new AppSettings();
        }
    }

    private void SaveSettings()
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(SettingsFilePath)!);
            File.WriteAllText(SettingsFilePath, JsonSerializer.Serialize(_settings));
        }
        catch
        {
            // A read-only profile should not stop editing; preferences simply won't persist.
        }
    }

    private static string PreviewDocument(string fragment, bool dark, bool editable)
    {
        var hasLao = fragment.Any(character => character is >= '\u0E80' and <= '\u0EFF');
        var language = hasLao ? "lo" : "en";
        var lineHeight = hasLao ? "1.9" : "1.65";
        var editorAttributes = editable
            ? " contenteditable=\"true\" spellcheck=\"true\" role=\"textbox\" aria-label=\"Visual Markdown editor\""
            : string.Empty;
        var editorScript = editable ? VisualEditorScript : string.Empty;
        var theme = dark
            ? "body{color:#e7e9ed}h1,h2,h3,h4{color:#f6f7f9}h1,h2{border-color:#454850}a{color:#78a9ff}blockquote{color:#c3c6cd}mark{background:#806515;color:#fff5bf}th,td{border-color:#4b4e56}"
            : "body{color:#202124}h1,h2,h3,h4{color:#111}h1,h2{border-color:#d7d9dc}a{color:#1769e0}blockquote{color:#52565d}th,td{border-color:#cfd2d7}";
        return $$"""
        <!doctype html><html lang="{{language}}"><head><meta charset="utf-8"><meta name="color-scheme" content="{{(dark ? "dark" : "light")}}"><style>
        {{PhetsarathLaoCss.Value}}
        :root{color-scheme:{{(dark ? "dark" : "light")}};font:16px/{{lineHeight}} 'Phetsarath Lao','Segoe UI',sans-serif}html,body{background:transparent}body{max-width:760px;margin:auto;padding:30px 38px 80px;overflow-wrap:normal;word-break:normal}body[contenteditable=true]{min-height:calc(100vh - 110px);outline:none;caret-color:#1769e0}
        h1,h2,h3,h4{line-height:1.25;margin-top:1.6em}h1,h2{border-bottom:1px solid;padding-bottom:.25em}a{text-decoration:none}
        blockquote{margin:1.2em 0;padding:.2em 1em;color:#52565d;border-left:4px solid #8b5cf6;background:rgba(139,92,246,.07)}mark{color:inherit;background:#ffe082;padding:.06em .18em;border-radius:.22em;box-decoration-break:clone;-webkit-box-decoration-break:clone}code{font-family:"Cascadia Mono",Consolas,monospace;background:rgba(127,127,127,.14);border-radius:5px;padding:.15em .35em}
        pre{padding:16px;overflow:auto;background:rgba(127,127,127,.12);border-radius:9px}pre code{padding:0;background:none}table{border-collapse:collapse;width:100%}th,td{border:1px solid #cfd2d7;padding:8px 11px;text-align:left}img{max-width:100%}
        {{theme}}
        </style></head><body{{editorAttributes}}>{{fragment}}{{editorScript}}</body></html>
        """;
    }

    private const string VisualEditorScript = """
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
            changeTimer = setTimeout(() => window.chrome.webview.postMessage(markdown()), 100);
          }

          function restoreSelection() {
            if (!savedRange) return;
            const selection = window.getSelection();
            selection.removeAllRanges();
            selection.addRange(savedRange);
          }

          function insertHTML(value) { document.execCommand('insertHTML', false, value); }

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
        """;

    private static string BuildPhetsarathLaoCss()
    {
        static string Face(string file, int weight)
        {
            var path = Path.Combine(AppContext.BaseDirectory, "Assets", "Fonts", file);
            if (!File.Exists(path)) return string.Empty;
            var base64 = Convert.ToBase64String(File.ReadAllBytes(path));
            return $"@font-face{{font-family:'Phetsarath Lao';src:url(data:font/woff2;base64,{base64}) format('woff2');font-weight:{weight};font-style:normal;font-display:swap;unicode-range:U+0E80-0EFF;}}";
        }

        return Face("PhetsarathLao-Regular.woff2", 400)
            + Face("PhetsarathLao-SemiBold.woff2", 600)
            + Face("PhetsarathLao-Bold.woff2", 700);
    }

    private sealed record WorkspaceEntry(string Path, string Name, bool IsDirectory)
    {
        public override string ToString() => IsDirectory ? $"📁 {Name}" : Name;
    }

    private sealed class AppSettings
    {
        public string Theme { get; set; } = "System";
        public string? WorkspacePath { get; set; }
    }
}
