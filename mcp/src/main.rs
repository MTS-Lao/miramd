use anyhow::{bail, Context};
use clap::Parser;
use rmcp::{
    handler::server::wrapper::Parameters, schemars, tool, tool_router, ErrorData as McpError,
    ServiceExt,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;

#[derive(Debug, Parser)]
#[command(
    name = "miramd-mcp",
    version,
    about = "Allow AI clients to work with Markdown files through MiraMD"
)]
struct Cli {
    /// Folder an AI client may read and write. Repeat to allow more than one.
    #[arg(long = "allow", required = true, value_name = "FOLDER")]
    allowed_folders: Vec<PathBuf>,

    /// MiraMD.app on macOS or MiraMD.exe on Windows, used by open_in_miramd.
    #[arg(long, value_name = "APP")]
    app: Option<PathBuf>,
}

#[derive(Clone)]
struct MiraMdMcp {
    roots: Arc<Vec<PathBuf>>,
    app: Option<PathBuf>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListRequest {
    /// Optional case-insensitive substring to match against a relative path.
    query: Option<String>,
    /// Maximum number of results. Defaults to 100 and is capped at 500.
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PathRequest {
    /// Absolute path inside an allowed folder, or a path relative to the first allowed folder.
    path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RenderRequest {
    /// Markdown source to render. This does not read or write a file.
    markdown: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct WriteRequest {
    /// Absolute path inside an allowed folder, or a path relative to the first allowed folder.
    path: String,
    /// Complete Markdown content to save.
    content: String,
    /// Revision returned by read_markdown. Required when replacing an existing file.
    expected_revision: Option<String>,
    /// Open the saved file in MiraMD after writing it.
    open_in_miramd: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct EditRequest {
    /// Absolute path inside an allowed folder, or a path relative to the first allowed folder.
    path: String,
    /// Exact text that currently exists in the file.
    old_text: String,
    /// Replacement text.
    new_text: String,
    /// Replace every match. Defaults to false; otherwise a non-unique match is rejected.
    replace_all: Option<bool>,
    /// Revision returned by read_markdown. Prevents overwriting newer changes.
    expected_revision: String,
    /// Open the saved file in MiraMD after writing it.
    open_in_miramd: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ExportWordRequest {
    /// Existing Markdown source path inside an allowed folder.
    source_path: String,
    /// Output .docx path inside an allowed folder. Defaults beside the source file.
    output_path: Option<String>,
    /// Explicitly allow replacing an existing .docx file. Defaults to false.
    overwrite: Option<bool>,
}

#[derive(Debug, Serialize)]
struct FileRecord {
    path: String,
    relative_path: String,
    root: String,
    bytes: u64,
}

#[derive(Debug, Serialize)]
struct ReadResult {
    path: String,
    revision: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct WriteResult {
    path: String,
    revision: String,
    bytes: usize,
    opened_in_miramd: bool,
}

#[derive(Debug, Serialize)]
struct WordExportResult {
    source_path: String,
    output_path: String,
    bytes: usize,
    lao_complex_script_enabled: bool,
}

#[tool_router(server_handler)]
impl MiraMdMcp {
    #[tool(
        description = "List Markdown files in the folders the user explicitly allowed. Returns paths and sizes; use read_markdown to inspect one."
    )]
    fn list_markdown_files(
        &self,
        Parameters(request): Parameters<ListRequest>,
    ) -> Result<String, McpError> {
        self.list_files(request).map(to_json).map_err(tool_error)
    }

    #[tool(
        description = "Read a Markdown file and return its complete content plus a revision token. Pass that token to write_markdown or edit_markdown so newer user changes are not overwritten."
    )]
    fn read_markdown(
        &self,
        Parameters(request): Parameters<PathRequest>,
    ) -> Result<String, McpError> {
        self.read_file(&request.path).map(to_json).map_err(tool_error)
    }

    #[tool(
        description = "Create or replace a Markdown file in an allowed folder. Replacing requires the latest expected_revision from read_markdown. Saves are crash-resistant and may optionally open the result in MiraMD."
    )]
    fn write_markdown(
        &self,
        Parameters(request): Parameters<WriteRequest>,
    ) -> Result<String, McpError> {
        self.write_file(request).map(to_json).map_err(tool_error)
    }

    #[tool(
        description = "Make a precise text replacement in a Markdown file. The exact old text and latest revision are required, preventing accidental or stale edits."
    )]
    fn edit_markdown(
        &self,
        Parameters(request): Parameters<EditRequest>,
    ) -> Result<String, McpError> {
        self.edit_file(request).map(to_json).map_err(tool_error)
    }

    #[tool(
        description = "Open an existing allowed Markdown file in the native MiraMD application. This does not modify the file."
    )]
    fn open_in_miramd(
        &self,
        Parameters(request): Parameters<PathRequest>,
    ) -> Result<String, McpError> {
        let path = self
            .resolve_markdown_path(&request.path, true)
            .map_err(tool_error)?;
        self.launch_app(&path).map_err(tool_error)?;
        Ok(format!("Opened {} in MiraMD.", path.display()))
    }

    #[tool(
        description = "Render Markdown with the same safe Rust engine used by MiraMD. Returns a sanitized HTML fragment for previews or export workflows."
    )]
    fn render_markdown(&self, Parameters(request): Parameters<RenderRequest>) -> String {
        miramd_core::render_html(&request.markdown)
    }

    #[tool(
        description = "Export an allowed Markdown file to Microsoft Word .docx. Preserves headings, formatting, lists, tables, code, Unicode, and Word complex-script metadata for Lao text."
    )]
    fn export_word_document(
        &self,
        Parameters(request): Parameters<ExportWordRequest>,
    ) -> Result<String, McpError> {
        self.export_word(request).map(to_json).map_err(tool_error)
    }
}

impl MiraMdMcp {
    fn new(roots: Vec<PathBuf>, app: Option<PathBuf>) -> anyhow::Result<Self> {
        let roots = roots
            .into_iter()
            .map(|root| {
                let canonical = root
                    .canonicalize()
                    .with_context(|| format!("allowed folder does not exist: {}", root.display()))?;
                if !canonical.is_dir() {
                    bail!("allowed path is not a folder: {}", canonical.display());
                }
                Ok(canonical)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self {
            roots: Arc::new(roots),
            app,
        })
    }

    fn list_files(&self, request: ListRequest) -> anyhow::Result<Vec<FileRecord>> {
        let query = request.query.unwrap_or_default().to_lowercase();
        let limit = request.limit.unwrap_or(100).clamp(1, 500);
        let mut result = Vec::new();

        for root in self.roots.iter() {
            for entry in WalkDir::new(root).follow_links(false).into_iter() {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(_) => continue,
                };
                if !entry.file_type().is_file() || !is_markdown(entry.path()) {
                    continue;
                }
                let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
                let relative_text = relative.to_string_lossy().to_string();
                if !query.is_empty() && !relative_text.to_lowercase().contains(&query) {
                    continue;
                }
                result.push(FileRecord {
                    path: entry.path().to_string_lossy().to_string(),
                    relative_path: relative_text,
                    root: root.to_string_lossy().to_string(),
                    bytes: entry.metadata().map(|value| value.len()).unwrap_or(0),
                });
            }
        }
        result.sort_by(|left, right| left.path.cmp(&right.path));
        result.truncate(limit);
        Ok(result)
    }

    fn read_file(&self, requested: &str) -> anyhow::Result<ReadResult> {
        let path = self.resolve_markdown_path(requested, true)?;
        let content = fs::read_to_string(&path)
            .with_context(|| format!("could not read UTF-8 Markdown: {}", path.display()))?;
        Ok(ReadResult {
            path: path.to_string_lossy().to_string(),
            revision: revision(&content),
            content,
        })
    }

    fn write_file(&self, request: WriteRequest) -> anyhow::Result<WriteResult> {
        let path = self.resolve_markdown_path(&request.path, false)?;
        if path.exists() {
            let existing = fs::read_to_string(&path)
                .with_context(|| format!("could not read existing file: {}", path.display()))?;
            let expected = request.expected_revision.as_deref().with_context(|| {
                "expected_revision is required when replacing a file; call read_markdown first"
            })?;
            if revision(&existing) != expected {
                bail!("the file changed since it was read; read it again before writing");
            }
        } else if request.expected_revision.is_some() {
            bail!("the file no longer exists; omit expected_revision only if it should be recreated");
        }

        atomic_write(&path, request.content.as_bytes())?;
        let opened = request.open_in_miramd.unwrap_or(false);
        if opened {
            self.launch_app(&path)?;
        }
        Ok(WriteResult {
            path: path.to_string_lossy().to_string(),
            revision: revision(&request.content),
            bytes: request.content.len(),
            opened_in_miramd: opened,
        })
    }

    fn edit_file(&self, request: EditRequest) -> anyhow::Result<WriteResult> {
        if request.old_text.is_empty() {
            bail!("old_text cannot be empty");
        }
        let path = self.resolve_markdown_path(&request.path, true)?;
        let existing = fs::read_to_string(&path)
            .with_context(|| format!("could not read existing file: {}", path.display()))?;
        if revision(&existing) != request.expected_revision {
            bail!("the file changed since it was read; read it again before editing");
        }
        let matches = existing.matches(&request.old_text).count();
        if matches == 0 {
            bail!("old_text was not found; read the file again and use an exact match");
        }
        if matches > 1 && !request.replace_all.unwrap_or(false) {
            bail!("old_text matched {matches} places; provide more context or set replace_all");
        }
        let updated = if request.replace_all.unwrap_or(false) {
            existing.replace(&request.old_text, &request.new_text)
        } else {
            existing.replacen(&request.old_text, &request.new_text, 1)
        };
        atomic_write(&path, updated.as_bytes())?;
        let opened = request.open_in_miramd.unwrap_or(false);
        if opened {
            self.launch_app(&path)?;
        }
        Ok(WriteResult {
            path: path.to_string_lossy().to_string(),
            revision: revision(&updated),
            bytes: updated.len(),
            opened_in_miramd: opened,
        })
    }

    fn export_word(&self, request: ExportWordRequest) -> anyhow::Result<WordExportResult> {
        let source = self.resolve_markdown_path(&request.source_path, true)?;
        let markdown = fs::read_to_string(&source)
            .with_context(|| format!("could not read UTF-8 Markdown: {}", source.display()))?;
        let default_output = source.with_extension("docx");
        let output = match request.output_path {
            Some(path) => self.resolve_docx_path(&path, false)?,
            None => self.resolve_docx_path(&default_output.to_string_lossy(), false)?,
        };
        if output.exists() && !request.overwrite.unwrap_or(false) {
            bail!("the Word output already exists; choose another path or set overwrite to true");
        }
        let title = source
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("MiraMD document");
        let bytes = miramd_core::export_docx(&markdown, title).map_err(anyhow::Error::msg)?;
        atomic_write(&output, &bytes)?;
        Ok(WordExportResult {
            source_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            bytes: bytes.len(),
            lao_complex_script_enabled: markdown
                .chars()
                .any(|character| matches!(character, '\u{0E80}'..='\u{0EFF}')),
        })
    }

    fn resolve_markdown_path(&self, requested: &str, must_exist: bool) -> anyhow::Result<PathBuf> {
        self.resolve_allowed_path(requested, must_exist, &["md", "markdown", "mdown", "mkd"])
    }

    fn resolve_docx_path(&self, requested: &str, must_exist: bool) -> anyhow::Result<PathBuf> {
        self.resolve_allowed_path(requested, must_exist, &["docx"])
    }

    fn resolve_allowed_path(
        &self,
        requested: &str,
        must_exist: bool,
        extensions: &[&str],
    ) -> anyhow::Result<PathBuf> {
        let requested = Path::new(requested);
        if requested
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            bail!("parent-directory components are not allowed in paths");
        }
        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            self.roots[0].join(requested)
        };
        if !has_extension(&candidate, extensions) {
            bail!("the requested file type is not allowed for this operation");
        }

        let checked = if candidate.exists() {
            candidate
                .canonicalize()
                .with_context(|| format!("could not resolve path: {}", candidate.display()))?
        } else {
            if must_exist {
                bail!("file does not exist: {}", candidate.display());
            }
            self.validate_new_path(&candidate)?
        };
        if !self.roots.iter().any(|root| checked.starts_with(root)) {
            bail!("path is outside the folders allowed by the user");
        }
        Ok(checked)
    }

    fn validate_new_path(&self, candidate: &Path) -> anyhow::Result<PathBuf> {
        let mut ancestor = candidate.parent().context("file has no parent folder")?;
        let mut missing = Vec::new();
        while !ancestor.exists() {
            let name = ancestor.file_name().context("could not resolve parent folder")?;
            missing.push(name.to_os_string());
            ancestor = ancestor.parent().context("could not resolve parent folder")?;
        }
        let mut resolved = ancestor
            .canonicalize()
            .with_context(|| format!("could not resolve parent: {}", ancestor.display()))?;
        for component in missing.iter().rev() {
            resolved.push(component);
        }
        resolved.push(candidate.file_name().context("file has no name")?);
        Ok(resolved)
    }

    fn launch_app(&self, path: &Path) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        {
            let app = self
                .app
                .as_ref()
                .map(|path| path.as_os_str())
                .unwrap_or_else(|| std::ffi::OsStr::new("MiraMD"));
            Command::new("open")
                .arg("-a")
                .arg(app)
                .arg(path)
                .spawn()
                .context("could not launch MiraMD")?;
        }
        #[cfg(target_os = "windows")]
        {
            let app = self.app.as_deref().unwrap_or_else(|| Path::new("MiraMD.exe"));
            Command::new(app)
                .arg(path)
                .spawn()
                .context("could not launch MiraMD")?;
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let app = self.app.as_deref().context("--app is required on this platform")?;
            Command::new(app)
                .arg(path)
                .spawn()
                .context("could not launch MiraMD")?;
        }
        Ok(())
    }
}

fn is_markdown(path: &Path) -> bool {
    has_extension(path, &["md", "markdown", "mdown", "mkd"])
}

fn has_extension(path: &Path, allowed: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| allowed.contains(&extension.to_lowercase().as_str()))
}

fn revision(content: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(content.as_bytes()))
}

fn atomic_write(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    let parent = path.parent().context("file has no parent folder")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("could not create folder: {}", parent.display()))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".miramd-save-{}-{nonce}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("could not create temporary file in {}", parent.display()))?;
    if let Err(error) = (|| -> std::io::Result<()> {
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temporary, path)
    })() {
        let _ = fs::remove_file(&temporary);
        return Err(error).with_context(|| format!("could not save {}", path.display()));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(target_os = "windows")]
fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    if !destination.exists() {
        return fs::rename(temporary, destination);
    }
    let backup = destination.with_extension("miramd-backup");
    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    fs::rename(destination, &backup)?;
    match fs::rename(temporary, destination) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(backup, destination);
            Err(error)
        }
    }
}

fn to_json<T: Serialize>(value: T) -> String {
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "null".to_owned())
}

fn tool_error(error: anyhow::Error) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let server = MiraMdMcp::new(cli.allowed_folders, cli.app)?;
    eprintln!("MiraMD MCP is running; allowed roots: {}", server.roots.len());
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(root: &Path) -> MiraMdMcp {
        MiraMdMcp::new(vec![root.to_path_buf()], None).unwrap()
    }

    #[test]
    fn rejects_paths_outside_allowed_root() {
        let root = tempfile::tempdir().unwrap();
        let service = server(root.path());
        assert!(service.resolve_markdown_path("../secret.md", false).is_err());
    }

    #[test]
    fn requires_revision_to_replace_and_rejects_stale_writes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.md");
        fs::write(&path, "one").unwrap();
        let service = server(root.path());
        assert!(service
            .write_file(WriteRequest {
                path: path.to_string_lossy().into_owned(),
                content: "two".into(),
                expected_revision: None,
                open_in_miramd: None,
            })
            .is_err());
        assert!(service
            .write_file(WriteRequest {
                path: path.to_string_lossy().into_owned(),
                content: "two".into(),
                expected_revision: Some("sha256:stale".into()),
                open_in_miramd: None,
            })
            .is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "one");
    }

    #[test]
    fn edits_only_an_exact_unique_match() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.md");
        fs::write(&path, "alpha beta").unwrap();
        let service = server(root.path());
        let result = service
            .edit_file(EditRequest {
                path: path.to_string_lossy().into_owned(),
                old_text: "beta".into(),
                new_text: "gamma".into(),
                replace_all: None,
                expected_revision: revision("alpha beta"),
                open_in_miramd: None,
            })
            .unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "alpha gamma");
        assert_eq!(result.revision, revision("alpha gamma"));
    }

    #[test]
    fn exports_allowed_lao_markdown_to_word() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("ບົດທົດສອບ.md");
        let output = root.path().join("ບົດທົດສອບ.docx");
        fs::write(&source, "# ສະບາຍດີ\n\nນີ້ແມ່ນເອກະສານພາສາລາວ").unwrap();
        let service = server(root.path());
        let result = service
            .export_word(ExportWordRequest {
                source_path: source.to_string_lossy().into_owned(),
                output_path: Some(output.to_string_lossy().into_owned()),
                overwrite: None,
            })
            .unwrap();
        assert!(fs::read(output).unwrap().starts_with(b"PK"));
        assert!(result.lao_complex_script_enabled);
    }
}
