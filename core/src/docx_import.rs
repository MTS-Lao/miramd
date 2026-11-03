use roxmltree::{Document, Node};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use zip::ZipArchive;

const MAX_DOCX_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4_096;
const MAX_ARCHIVE_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_XML_PART_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocxImport {
    pub markdown: String,
    pub title: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct StyleInfo {
    name: String,
    outline_level: Option<u8>,
}

#[derive(Debug, Clone, Default)]
struct NumberingInfo {
    formats: HashMap<(String, usize), String>,
}

impl NumberingInfo {
    fn ordered(&self, number_id: &str, level: usize) -> bool {
        !matches!(
            self.formats.get(&(number_id.to_owned(), level)).map(String::as_str),
            Some("bullet") | Some("none")
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RunStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    highlight: bool,
    code: bool,
    link: Option<String>,
}

#[derive(Debug, Clone)]
struct Inline {
    text: String,
    style: RunStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Normal,
    Heading(u8),
    Quote,
    Code,
    List { ordered: bool, level: usize },
    Rule,
}

#[derive(Debug, Clone)]
struct MarkdownBlock {
    text: String,
    is_list: bool,
}

struct ImportContext {
    styles: HashMap<String, StyleInfo>,
    numbering: NumberingInfo,
    relationships: HashMap<String, String>,
    image_count: usize,
    equation_count: usize,
}

pub fn import_docx_from_path(path: &str) -> Result<DocxImport, String> {
    let path = Path::new(path);
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not read the Word document: {error}"))?;
    if metadata.len() > MAX_DOCX_BYTES {
        return Err("The Word document is too large to import safely (maximum 128 MB).".to_owned());
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("Could not read the Word document: {error}"))?;
    let source_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Imported document.docx");
    import_docx(&bytes, source_name)
}

pub fn import_docx(bytes: &[u8], source_name: &str) -> Result<DocxImport, String> {
    if bytes.len() as u64 > MAX_DOCX_BYTES {
        return Err("The Word document is too large to import safely (maximum 128 MB).".to_owned());
    }

    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|error| format!("This file is not a readable DOCX package: {error}"))?;
    validate_archive(&mut archive)?;

    let document_xml = read_xml_part(&mut archive, "word/document.xml", true)?
        .ok_or_else(|| "The DOCX package does not contain word/document.xml.".to_owned())?;
    let styles_xml = read_xml_part(&mut archive, "word/styles.xml", false)?;
    let numbering_xml = read_xml_part(&mut archive, "word/numbering.xml", false)?;
    let relationships_xml =
        read_xml_part(&mut archive, "word/_rels/document.xml.rels", false)?;
    let core_xml = read_xml_part(&mut archive, "docProps/core.xml", false)?;

    let styles = styles_xml
        .as_deref()
        .map(parse_styles)
        .transpose()?
        .unwrap_or_default();
    let numbering = numbering_xml
        .as_deref()
        .map(parse_numbering)
        .transpose()?
        .unwrap_or_default();
    let relationships = relationships_xml
        .as_deref()
        .map(parse_relationships)
        .transpose()?
        .unwrap_or_default();
    let title = core_xml
        .as_deref()
        .and_then(parse_title)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| source_title(source_name));

    let parsed = Document::parse(&document_xml)
        .map_err(|error| format!("The main Word document XML is invalid: {error}"))?;
    let body = parsed
        .descendants()
        .find(|node| is_element(*node, "body"))
        .ok_or_else(|| "The Word document does not contain a readable body.".to_owned())?;
    let mut context = ImportContext {
        styles,
        numbering,
        relationships,
        image_count: 0,
        equation_count: 0,
    };
    let mut blocks = Vec::new();
    collect_body_blocks(body, &mut context, &mut blocks);
    let markdown = join_blocks(&blocks);

    if markdown.trim().is_empty() {
        return Err("The Word document did not contain readable text.".to_owned());
    }

    let mut warnings = Vec::new();
    if context.image_count > 0 {
        warnings.push(format!(
            "{} embedded image{} converted to text placeholders. Save the Markdown before adding permanent image files.",
            context.image_count,
            if context.image_count == 1 { " was" } else { "s were" }
        ));
    }
    if context.equation_count > 0 {
        warnings.push(format!(
            "{} Word equation{} converted to [Equation] placeholders.",
            context.equation_count,
            if context.equation_count == 1 { " was" } else { "s were" }
        ));
    }

    Ok(DocxImport {
        markdown,
        title,
        warnings,
    })
}

fn validate_archive(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<(), String> {
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("The DOCX package contains too many files to import safely.".to_owned());
    }
    let mut total_size = 0_u64;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| format!("Could not inspect the DOCX package: {error}"))?;
        if file.enclosed_name().is_none() || file.is_symlink() {
            return Err("The DOCX package contains an unsafe file entry.".to_owned());
        }
        total_size = total_size.saturating_add(file.size());
        if total_size > MAX_ARCHIVE_UNCOMPRESSED_BYTES {
            return Err(
                "The expanded DOCX package is too large to import safely (maximum 256 MB)."
                    .to_owned(),
            );
        }
    }
    Ok(())
}

fn read_xml_part(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
    required: bool,
) -> Result<Option<String>, String> {
    let mut file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) if !required => return Ok(None),
        Err(error) => return Err(format!("Could not read {name} from the DOCX package: {error}")),
    };
    if file.size() > MAX_XML_PART_BYTES {
        return Err(format!("The DOCX XML part {name} is too large to import safely."));
    }
    let mut bytes = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read {name} from the DOCX package: {error}"))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("The DOCX XML part {name} is not valid UTF-8."))?;
    Ok(Some(text))
}

fn parse_styles(xml: &str) -> Result<HashMap<String, StyleInfo>, String> {
    let document = Document::parse(xml)
        .map_err(|error| format!("The Word styles XML is invalid: {error}"))?;
    let mut styles = HashMap::new();
    for style in document.descendants().filter(|node| is_element(*node, "style")) {
        let Some(style_id) = attribute(style, "styleId") else {
            continue;
        };
        let name = child(style, "name")
            .and_then(|node| attribute(node, "val"))
            .unwrap_or(style_id)
            .to_owned();
        let outline_level = style
            .descendants()
            .find(|node| is_element(*node, "outlineLvl"))
            .and_then(|node| attribute(node, "val"))
            .and_then(|value| value.parse::<u8>().ok());
        styles.insert(
            style_id.to_owned(),
            StyleInfo {
                name,
                outline_level,
            },
        );
    }
    Ok(styles)
}

fn parse_numbering(xml: &str) -> Result<NumberingInfo, String> {
    let document = Document::parse(xml)
        .map_err(|error| format!("The Word numbering XML is invalid: {error}"))?;
    let mut abstract_formats: HashMap<(String, usize), String> = HashMap::new();
    for abstract_number in document
        .descendants()
        .filter(|node| is_element(*node, "abstractNum"))
    {
        let Some(abstract_id) = attribute(abstract_number, "abstractNumId") else {
            continue;
        };
        for level in abstract_number.children().filter(|node| is_element(*node, "lvl")) {
            let index = attribute(level, "ilvl")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            let format = child(level, "numFmt")
                .and_then(|node| attribute(node, "val"))
                .unwrap_or("decimal")
                .to_owned();
            abstract_formats.insert((abstract_id.to_owned(), index), format);
        }
    }

    let mut number_to_abstract = HashMap::new();
    for number in document.descendants().filter(|node| is_element(*node, "num")) {
        let Some(number_id) = attribute(number, "numId") else {
            continue;
        };
        let Some(abstract_id) = child(number, "abstractNumId")
            .and_then(|node| attribute(node, "val"))
        else {
            continue;
        };
        number_to_abstract.insert(number_id.to_owned(), abstract_id.to_owned());
    }

    let mut formats = HashMap::new();
    for (number_id, abstract_id) in number_to_abstract {
        for ((candidate_id, level), format) in &abstract_formats {
            if candidate_id == &abstract_id {
                formats.insert((number_id.clone(), *level), format.clone());
            }
        }
    }
    Ok(NumberingInfo { formats })
}

fn parse_relationships(xml: &str) -> Result<HashMap<String, String>, String> {
    let document = Document::parse(xml)
        .map_err(|error| format!("The Word relationships XML is invalid: {error}"))?;
    let mut relationships = HashMap::new();
    for relationship in document
        .descendants()
        .filter(|node| is_element(*node, "Relationship"))
    {
        let Some(id) = attribute(relationship, "Id") else {
            continue;
        };
        let Some(target) = attribute(relationship, "Target") else {
            continue;
        };
        let kind = attribute(relationship, "Type").unwrap_or_default();
        if kind.ends_with("/hyperlink") && safe_link(target) {
            relationships.insert(id.to_owned(), target.to_owned());
        }
    }
    Ok(relationships)
}

fn parse_title(xml: &str) -> Option<String> {
    let document = Document::parse(xml).ok()?;
    document
        .descendants()
        .find(|node| is_element(*node, "title"))
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn source_title(source_name: &str) -> String {
    Path::new(source_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("Imported document")
        .to_owned()
}

fn collect_body_blocks(
    body: Node<'_, '_>,
    context: &mut ImportContext,
    output: &mut Vec<MarkdownBlock>,
) {
    for node in body.children().filter(Node::is_element) {
        if is_element(node, "p") {
            if let Some(block) = paragraph_block(node, context) {
                output.push(block);
            }
        } else if is_element(node, "tbl") {
            if let Some(table) = table_block(node, context) {
                output.push(table);
            }
        } else if is_element(node, "sdt") || is_element(node, "customXml") {
            collect_body_blocks(node, context, output);
        }
    }
}

fn paragraph_block(node: Node<'_, '_>, context: &mut ImportContext) -> Option<MarkdownBlock> {
    let properties = child(node, "pPr");
    let mut inlines = Vec::new();
    for child_node in node.children().filter(Node::is_element) {
        if !is_element(child_node, "pPr") {
            collect_inlines(child_node, None, context, &mut inlines);
        }
    }
    merge_inlines(&mut inlines);

    let mut kind = paragraph_kind(properties, context);
    if matches!(kind, BlockKind::Heading(_))
        && inlines
            .iter()
            .filter(|inline| !inline.text.trim().is_empty())
            .all(|inline| inline.style.bold)
    {
        for inline in &mut inlines {
            inline.style.bold = false;
        }
    }
    if matches!(kind, BlockKind::Normal)
        && !inlines.is_empty()
        && inlines.iter().all(|inline| inline.style.code || inline.text.trim().is_empty())
        && properties.is_some_and(has_code_box)
    {
        kind = BlockKind::Code;
    }
    if inlines.is_empty() && properties.is_some_and(has_bottom_border) {
        kind = BlockKind::Rule;
    } else if matches!(kind, BlockKind::Normal)
        && properties.is_some_and(has_quote_border)
    {
        kind = BlockKind::Quote;
    }

    let formatted = match kind {
        BlockKind::Rule => "---".to_owned(),
        BlockKind::Code => {
            let raw = inlines.iter().map(|inline| inline.text.as_str()).collect::<String>();
            let fence = "`".repeat(longest_backtick_run(&raw).saturating_add(1).max(3));
            format!("{fence}\n{}\n{fence}", raw.trim_end_matches('\n'))
        }
        _ => format_inlines(&inlines),
    };
    if formatted.trim().is_empty() {
        return None;
    }

    let text = match kind {
        BlockKind::Normal | BlockKind::Code | BlockKind::Rule => formatted,
        BlockKind::Heading(level) => format!("{} {}", "#".repeat(level as usize), formatted.trim()),
        BlockKind::Quote => formatted
            .lines()
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        BlockKind::List { ordered, level } => format!(
            "{}{} {}",
            "  ".repeat(level.min(8)),
            if ordered { "1." } else { "-" },
            formatted.trim()
        ),
    };
    Some(MarkdownBlock {
        text,
        is_list: matches!(kind, BlockKind::List { .. }),
    })
}

fn paragraph_kind(properties: Option<Node<'_, '_>>, context: &ImportContext) -> BlockKind {
    let Some(properties) = properties else {
        return BlockKind::Normal;
    };
    if let Some(number_properties) = child(properties, "numPr") {
        let level = child(number_properties, "ilvl")
            .and_then(|node| attribute(node, "val"))
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let number_id = child(number_properties, "numId")
            .and_then(|node| attribute(node, "val"))
            .unwrap_or("0");
        return BlockKind::List {
            ordered: context.numbering.ordered(number_id, level),
            level,
        };
    }

    let style_id = child(properties, "pStyle")
        .and_then(|node| attribute(node, "val"))
        .unwrap_or_default();
    let style = context.styles.get(style_id);
    if let Some(level) = style.and_then(|value| value.outline_level) {
        return BlockKind::Heading(level.saturating_add(1).clamp(1, 6));
    }
    let label = format!(
        "{} {}",
        style_id,
        style.map(|value| value.name.as_str()).unwrap_or_default()
    )
    .to_ascii_lowercase();
    if let Some(level) = heading_level(&label) {
        return BlockKind::Heading(level);
    }
    if label.contains("title") && !label.contains("subtitle") {
        return BlockKind::Heading(1);
    }
    if label.contains("quote") || label.contains("citation") {
        return BlockKind::Quote;
    }
    if label.contains("code") || label.contains("preformatted") || label.contains("verbatim") {
        return BlockKind::Code;
    }
    if label.contains("list") {
        return BlockKind::List {
            ordered: label.contains("number") || label.contains("decimal"),
            level: 0,
        };
    }
    if child(properties, "outlineLvl")
        .and_then(|node| attribute(node, "val"))
        .and_then(|value| value.parse::<u8>().ok())
        .is_some_and(|level| level < 6)
    {
        let level = child(properties, "outlineLvl")
            .and_then(|node| attribute(node, "val"))
            .and_then(|value| value.parse::<u8>().ok())
            .unwrap_or(0);
        return BlockKind::Heading(level + 1);
    }
    BlockKind::Normal
}

fn heading_level(label: &str) -> Option<u8> {
    let index = label.find("heading")? + "heading".len();
    let suffix = label[index..].trim_start_matches(|character: char| !character.is_ascii_digit());
    suffix
        .chars()
        .next()
        .and_then(|character| character.to_digit(10))
        .map(|value| (value as u8).clamp(1, 6))
}

fn collect_inlines(
    node: Node<'_, '_>,
    link: Option<String>,
    context: &mut ImportContext,
    output: &mut Vec<Inline>,
) {
    if is_deleted(node) {
        return;
    }
    if is_element(node, "r") {
        if let Some(inline) = run_inline(node, link, context) {
            output.push(inline);
        }
        return;
    }
    if is_element(node, "hyperlink") {
        let relationship_id = attribute(node, "id");
        let anchor = attribute(node, "anchor");
        let target = relationship_id
            .and_then(|id| context.relationships.get(id).cloned())
            .or_else(|| anchor.map(|value| format!("#{value}")))
            .or(link);
        for child_node in node.children().filter(Node::is_element) {
            collect_inlines(child_node, target.clone(), context, output);
        }
        return;
    }
    if is_element(node, "oMath") || is_element(node, "oMathPara") {
        context.equation_count += 1;
        output.push(Inline {
            text: "[Equation]".to_owned(),
            style: RunStyle::default(),
        });
        return;
    }
    for child_node in node.children().filter(Node::is_element) {
        collect_inlines(child_node, link.clone(), context, output);
    }
}

fn run_inline(
    run: Node<'_, '_>,
    link: Option<String>,
    context: &mut ImportContext,
) -> Option<Inline> {
    let properties = child(run, "rPr");
    if properties.is_some_and(|node| property_enabled(node, "vanish")) {
        return None;
    }
    let mut style = RunStyle {
        bold: properties.is_some_and(|node| {
            property_enabled(node, "b") || property_enabled(node, "bCs")
        }),
        italic: properties.is_some_and(|node| {
            property_enabled(node, "i") || property_enabled(node, "iCs")
        }),
        strike: properties.is_some_and(|node| property_enabled(node, "strike")),
        highlight: properties
            .and_then(|node| child(node, "highlight"))
            .and_then(|node| attribute(node, "val"))
            .is_some_and(|value| value != "none"),
        code: false,
        link,
    };
    if let Some(properties) = properties {
        let run_style = child(properties, "rStyle")
            .and_then(|node| attribute(node, "val"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        let fonts = child(properties, "rFonts")
            .map(|node| {
                node.attributes()
                    .map(|attribute| attribute.value())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_ascii_lowercase()
            })
            .unwrap_or_default();
        style.code = run_style.contains("code")
            || run_style.contains("verbatim")
            || ["consolas", "courier", "menlo", "monaco", "cascadia", "mono"]
                .iter()
                .any(|font| fonts.contains(font));
    }

    let mut text = String::new();
    let mut saw_image = false;
    for node in run.descendants().filter(Node::is_element) {
        if is_deleted(node) {
            continue;
        }
        if is_element(node, "t") {
            text.push_str(node.text().unwrap_or_default());
        } else if is_element(node, "tab") {
            text.push('\t');
        } else if is_element(node, "br") || is_element(node, "cr") {
            text.push('\n');
        } else if is_element(node, "noBreakHyphen") {
            text.push('‑');
        } else if is_element(node, "footnoteReference") {
            if let Some(id) = attribute(node, "id") {
                text.push_str(&format!("[^{}]", escape_markdown(id)));
            }
        } else if !saw_image && (is_element(node, "drawing") || is_element(node, "pict")) {
            let description = node
                .descendants()
                .find(|candidate| {
                    is_element(*candidate, "docPr") || is_element(*candidate, "cNvPr")
                })
                .and_then(|candidate| {
                    attribute(candidate, "descr")
                        .or_else(|| attribute(candidate, "title"))
                        .or_else(|| attribute(candidate, "name"))
                })
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("Embedded image");
            if !text.is_empty()
                && !text
                    .chars()
                    .last()
                    .is_some_and(|character| character.is_whitespace())
            {
                text.push(' ');
            }
            text.push_str(&format!("[Image: {}]", description.trim()));
            context.image_count += 1;
            saw_image = true;
        }
    }
    if text.is_empty() {
        None
    } else {
        Some(Inline { text, style })
    }
}

fn table_block(node: Node<'_, '_>, context: &mut ImportContext) -> Option<MarkdownBlock> {
    let mut rows = Vec::new();
    for row in node.children().filter(|node| is_element(*node, "tr")) {
        let mut cells = Vec::new();
        for cell in row.children().filter(|node| is_element(*node, "tc")) {
            let mut parts = Vec::new();
            for paragraph in cell.children().filter(|node| is_element(*node, "p")) {
                if let Some(block) = paragraph_block(paragraph, context) {
                    parts.push(block.text);
                }
            }
            let value = parts
                .join("<br>")
                .replace('|', "\\|")
                .replace('\n', "<br>");
            cells.push(value);
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    if rows.is_empty() {
        return None;
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    for row in &mut rows {
        row.resize(columns, String::new());
    }
    let mut lines = vec![format!("| {} |", rows[0].join(" | "))];
    lines.push(format!("| {} |", vec!["---"; columns].join(" | ")));
    for row in rows.iter().skip(1) {
        lines.push(format!("| {} |", row.join(" | ")));
    }
    Some(MarkdownBlock {
        text: lines.join("\n"),
        is_list: false,
    })
}

fn merge_inlines(inlines: &mut Vec<Inline>) {
    let mut merged: Vec<Inline> = Vec::with_capacity(inlines.len());
    for inline in inlines.drain(..) {
        if let Some(previous) = merged.last_mut() {
            if previous.style == inline.style {
                previous.text.push_str(&inline.text);
                continue;
            }
        }
        merged.push(inline);
    }
    *inlines = merged;
}

fn format_inlines(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(format_inline)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn format_inline(inline: &Inline) -> String {
    let mut output = String::new();
    let mut lines = inline.text.split('\n').peekable();
    while let Some(line) = lines.next() {
        output.push_str(&format_inline_line(line, &inline.style));
        if lines.peek().is_some() {
            output.push_str("  \n");
        }
    }
    output
}

fn format_inline_line(text: &str, style: &RunStyle) -> String {
    if text.is_empty() {
        return String::new();
    }
    let leading_len = text.len() - text.trim_start_matches(char::is_whitespace).len();
    let trailing_start = text.trim_end_matches(char::is_whitespace).len();
    if leading_len >= trailing_start {
        return text.replace('\t', "    ");
    }
    let leading = text[..leading_len].replace('\t', "    ");
    let trailing = text[trailing_start..].replace('\t', "    ");
    let core = &text[leading_len..trailing_start];
    let mut formatted = if style.code {
        inline_code(core)
    } else {
        escape_markdown(core)
    };
    if style.bold {
        formatted = format!("**{formatted}**");
    }
    if style.italic {
        formatted = format!("_{formatted}_");
    }
    if style.strike {
        formatted = format!("~~{formatted}~~");
    }
    if style.highlight {
        formatted = format!("=={formatted}==");
    }
    if let Some(link) = style.link.as_deref().filter(|value| safe_link(value)) {
        formatted = format!("[{formatted}]({})", escape_link(link));
    }
    format!("{leading}{formatted}{trailing}")
}

fn inline_code(value: &str) -> String {
    let ticks = "`".repeat(longest_backtick_run(value).saturating_add(1).max(1));
    if value.starts_with('`') || value.ends_with('`') {
        format!("{ticks} {value} {ticks}")
    } else {
        format!("{ticks}{value}{ticks}")
    }
}

fn longest_backtick_run(value: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for character in value.chars() {
        if character == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn escape_markdown(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '~' | '|') {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

fn escape_link(value: &str) -> String {
    value
        .replace(' ', "%20")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn join_blocks(blocks: &[MarkdownBlock]) -> String {
    let mut output = String::new();
    let mut previous_was_list = false;
    for block in blocks {
        if !output.is_empty() {
            if previous_was_list && block.is_list {
                output.push('\n');
            } else {
                output.push_str("\n\n");
            }
        }
        output.push_str(block.text.trim());
        previous_was_list = block.is_list;
    }
    output.trim().to_owned() + "\n"
}

fn property_enabled(properties: Node<'_, '_>, name: &str) -> bool {
    child(properties, name)
        .map(|node| {
            !matches!(
                attribute(node, "val").map(str::to_ascii_lowercase).as_deref(),
                Some("0") | Some("false") | Some("off") | Some("none")
            )
        })
        .unwrap_or(false)
}

fn has_code_box(properties: Node<'_, '_>) -> bool {
    child(properties, "shd").is_some() || child(properties, "pBdr").is_some()
}

fn has_bottom_border(properties: Node<'_, '_>) -> bool {
    child(properties, "pBdr").and_then(|node| child(node, "bottom")).is_some()
}

fn has_quote_border(properties: Node<'_, '_>) -> bool {
    child(properties, "pBdr").and_then(|node| child(node, "left")).is_some()
        && child(properties, "ind").is_some()
}

fn is_deleted(node: Node<'_, '_>) -> bool {
    node.ancestors().any(|ancestor| {
        is_element(ancestor, "del")
            || is_element(ancestor, "moveFrom")
            || is_element(ancestor, "delText")
    })
}

fn safe_link(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:")
        || lower.starts_with('#')
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children().find(|child| is_element(*child, name))
}

fn attribute<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn is_element(node: Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_docx;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn test_docx(parts: &[(&str, &str)]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut archive = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, contents) in parts {
            archive.start_file(*name, options).unwrap();
            archive.write_all(contents.as_bytes()).unwrap();
        }
        archive.finish().unwrap().into_inner()
    }

    #[test]
    fn imports_exported_markdown_structure_and_lao_text() {
        let source = "# ສະບາຍດີ\n\nRead **bold**, *italic*, ==marked==, and `code`.\n\n- ລາຍການ\n- Second\n\n| Name | Value |\n|---|---|\n| Language | Lao |\n\n```\nlet value = 42;\n```";
        let bytes = export_docx(source, "Round trip").expect("export DOCX");
        let imported = import_docx(&bytes, "round-trip.docx").expect("import DOCX");
        assert!(
            imported.markdown.contains("# ສະບາຍດີ"),
            "{}",
            imported.markdown
        );
        assert!(imported.markdown.contains("**bold**"));
        assert!(imported.markdown.contains("_italic_"));
        assert!(imported.markdown.contains("==marked=="));
        assert!(imported.markdown.contains("- ລາຍການ"));
        assert!(imported.markdown.contains("| Name | Value |"));
        assert!(imported.markdown.contains("let value = 42;"));
    }

    #[test]
    fn rejects_unsafe_archive_entries() {
        let cursor = Cursor::new(Vec::new());
        let mut archive = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o120777);
        archive.start_file("../unsafe-link", options).unwrap();
        archive.write_all(b"target").unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        let error = import_docx(&bytes, "unsafe.docx").unwrap_err();
        assert!(error.contains("unsafe file entry"), "{error}");
    }

    #[test]
    fn imports_inserted_text_and_ignores_tracked_deletions() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:ins><w:r><w:t>Keep</w:t></w:r></w:ins><w:del><w:r><w:delText>Remove</w:delText></w:r></w:del></w:p></w:body></w:document>"#;
        let cursor = Cursor::new(Vec::new());
        let mut archive = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        archive.start_file("word/document.xml", options).unwrap();
        archive.write_all(xml.as_bytes()).unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        let imported = import_docx(&bytes, "tracked.docx").unwrap();
        assert_eq!(imported.markdown, "Keep\n");
    }

    #[test]
    fn imports_safe_hyperlinks_and_image_alt_text() {
        let document = r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><w:body><w:p><w:hyperlink r:id="rId9"><w:r><w:t>MiraMD site</w:t></w:r></w:hyperlink></w:p><w:p><w:r><w:drawing><wp:docPr id="1" name="Picture 1" descr="Lao workflow diagram"/></w:drawing></w:r></w:p></w:body></w:document>"#;
        let relationships = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://miramd.example/docs" TargetMode="External"/></Relationships>"#;
        let bytes = test_docx(&[
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels", relationships),
        ]);
        let imported = import_docx(&bytes, "links.docx").unwrap();
        assert!(
            imported
                .markdown
                .contains("[MiraMD site](https://miramd.example/docs)")
        );
        assert!(
            imported
                .markdown
                .contains("\\[Image: Lao workflow diagram\\]"),
            "{}",
            imported.markdown
        );
        assert_eq!(imported.warnings.len(), 1);
    }
}
