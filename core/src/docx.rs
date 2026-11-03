use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::{fs, io::Write, path::Path};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

#[derive(Debug, Clone, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    highlight: bool,
    link: Option<String>,
}

#[derive(Debug, Clone)]
struct Inline {
    text: String,
    style: InlineStyle,
}

#[derive(Debug, Clone)]
enum ParagraphKind {
    Normal,
    Heading(u8),
    Quote,
    List { ordered: bool, level: usize },
}

#[derive(Debug, Clone)]
struct Paragraph {
    kind: ParagraphKind,
    inlines: Vec<Inline>,
}

#[derive(Debug, Clone)]
enum Block {
    Paragraph(Paragraph),
    Code(String),
    Rule,
    Table(Vec<Vec<Vec<Inline>>>),
}

#[derive(Default)]
struct TableBuilder {
    rows: Vec<Vec<Vec<Inline>>>,
}

struct MarkdownState {
    blocks: Vec<Block>,
    paragraph: Option<Paragraph>,
    styles: Vec<InlineStyle>,
    lists: Vec<bool>,
    item_depth: usize,
    quote_depth: usize,
    code: Option<String>,
    table: Option<TableBuilder>,
}

impl MarkdownState {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            paragraph: None,
            styles: vec![InlineStyle::default()],
            lists: Vec::new(),
            item_depth: 0,
            quote_depth: 0,
            code: None,
            table: None,
        }
    }

    fn current_style(&self) -> InlineStyle {
        self.styles.last().cloned().unwrap_or_default()
    }

    fn push_style(&mut self, update: impl FnOnce(&mut InlineStyle)) {
        let mut style = self.current_style();
        update(&mut style);
        self.styles.push(style);
    }

    fn pop_style(&mut self) {
        if self.styles.len() > 1 {
            self.styles.pop();
        }
    }

    fn inferred_kind(&self) -> ParagraphKind {
        if self.item_depth > 0 && !self.lists.is_empty() {
            ParagraphKind::List {
                ordered: *self.lists.last().unwrap_or(&false),
                level: self.lists.len().saturating_sub(1),
            }
        } else if self.quote_depth > 0 {
            ParagraphKind::Quote
        } else {
            ParagraphKind::Normal
        }
    }

    fn begin_paragraph(&mut self, kind: Option<ParagraphKind>) {
        if self.table.is_some() {
            return;
        }
        self.flush_paragraph();
        self.paragraph = Some(Paragraph {
            kind: kind.unwrap_or_else(|| self.inferred_kind()),
            inlines: Vec::new(),
        });
    }

    fn ensure_paragraph(&mut self) {
        if self.paragraph.is_none() && self.table.is_none() {
            let kind = self.inferred_kind();
            self.paragraph = Some(Paragraph {
                kind,
                inlines: Vec::new(),
            });
        }
    }

    fn push_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        if let Some(code) = self.code.as_mut() {
            code.push_str(&text);
            return;
        }
        let inline = Inline {
            text,
            style: self.current_style(),
        };
        if let Some(table) = self.table.as_mut() {
            if table.rows.is_empty() {
                table.rows.push(Vec::new());
            }
            if table.rows.last().is_some_and(Vec::is_empty) {
                table.rows.last_mut().unwrap().push(Vec::new());
            }
            table
                .rows
                .last_mut()
                .unwrap()
                .last_mut()
                .unwrap()
                .push(inline);
        } else {
            self.ensure_paragraph();
            self.paragraph.as_mut().unwrap().inlines.push(inline);
        }
    }

    fn flush_paragraph(&mut self) {
        if let Some(paragraph) = self.paragraph.take() {
            if !paragraph.inlines.is_empty() {
                self.blocks.push(Block::Paragraph(paragraph));
            }
        }
    }
}

/// Convert Markdown into a self-contained Word document. The OOXML intentionally
/// marks Lao runs as complex script so Word uses dictionary-based Lao wrapping.
pub fn export_docx(markdown: &str, title: &str) -> Result<Vec<u8>, String> {
    let blocks = parse_markdown(markdown);
    let document = document_xml(&blocks);
    let cursor = std::io::Cursor::new(Vec::new());
    let mut archive = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let files = [
        ("[Content_Types].xml", content_types_xml()),
        ("_rels/.rels", package_relationships_xml()),
        ("docProps/core.xml", core_properties_xml(title)),
        ("docProps/app.xml", app_properties_xml()),
        ("word/document.xml", document),
        ("word/styles.xml", styles_xml()),
        ("word/settings.xml", settings_xml()),
        ("word/numbering.xml", numbering_xml()),
        ("word/_rels/document.xml.rels", document_relationships_xml()),
    ];

    for (name, content) in files {
        archive
            .start_file(name, options)
            .map_err(|error| format!("Could not create Word document: {error}"))?;
        archive
            .write_all(content.as_bytes())
            .map_err(|error| format!("Could not write Word document: {error}"))?;
    }
    let cursor = archive
        .finish()
        .map_err(|error| format!("Could not finish Word document: {error}"))?;
    Ok(cursor.into_inner())
}

pub fn export_docx_to_path(markdown: &str, title: &str, path: &str) -> Result<(), String> {
    let bytes = export_docx(markdown, title)?;
    fs::write(Path::new(path), bytes)
        .map_err(|error| format!("Could not save Word document to {path}: {error}"))
}

fn markdown_options() -> Options {
    Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_HEADING_ATTRIBUTES
}

fn parse_markdown(markdown: &str) -> Vec<Block> {
    let mut state = MarkdownState::new();
    let highlighted = crate::highlight_markup(markdown);
    for event in Parser::new_ext(&highlighted, markdown_options()) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => state.begin_paragraph(None),
                Tag::Heading { level, .. } => state.begin_paragraph(Some(ParagraphKind::Heading(
                    heading_level(level),
                ))),
                Tag::BlockQuote(_) => {
                    state.flush_paragraph();
                    state.quote_depth += 1;
                }
                Tag::CodeBlock(CodeBlockKind::Indented | CodeBlockKind::Fenced(_)) => {
                    state.flush_paragraph();
                    state.code = Some(String::new());
                }
                Tag::List(first) => {
                    state.flush_paragraph();
                    state.lists.push(first.is_some());
                }
                Tag::Item => {
                    state.flush_paragraph();
                    state.item_depth += 1;
                }
                Tag::Emphasis => state.push_style(|style| style.italic = true),
                Tag::Strong => state.push_style(|style| style.bold = true),
                Tag::Strikethrough => state.push_style(|style| style.strike = true),
                Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                    state.push_style(|style| style.link = Some(dest_url.to_string()))
                }
                Tag::Table(_) => {
                    state.flush_paragraph();
                    state.table = Some(TableBuilder::default());
                }
                Tag::TableHead | Tag::TableRow => {
                    if let Some(table) = state.table.as_mut() {
                        table.rows.push(Vec::new());
                    }
                }
                Tag::TableCell => {
                    if let Some(table) = state.table.as_mut() {
                        if table.rows.is_empty() {
                            table.rows.push(Vec::new());
                        }
                        table.rows.last_mut().unwrap().push(Vec::new());
                    }
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph | TagEnd::Heading(_) => state.flush_paragraph(),
                TagEnd::BlockQuote(_) => {
                    state.flush_paragraph();
                    state.quote_depth = state.quote_depth.saturating_sub(1);
                }
                TagEnd::CodeBlock => {
                    if let Some(code) = state.code.take() {
                        state.blocks.push(Block::Code(code.trim_end_matches('\n').to_owned()));
                    }
                }
                TagEnd::List(_) => {
                    state.flush_paragraph();
                    state.lists.pop();
                }
                TagEnd::Item => {
                    state.flush_paragraph();
                    state.item_depth = state.item_depth.saturating_sub(1);
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => state.pop_style(),
                TagEnd::Link | TagEnd::Image => {
                    let destination = state.current_style().link.clone();
                    state.pop_style();
                    if let Some(destination) = destination {
                        state.push_text(format!(" ({destination})"));
                    }
                }
                TagEnd::Table => {
                    if let Some(table) = state.table.take() {
                        if !table.rows.is_empty() {
                            state.blocks.push(Block::Table(table.rows));
                        }
                    }
                }
                _ => {}
            },
            Event::Text(text) => state.push_text(text.to_string()),
            Event::Code(text) => {
                let mut style = state.current_style();
                style.code = true;
                state.styles.push(style);
                state.push_text(text.to_string());
                state.pop_style();
            }
            Event::SoftBreak => state.push_text(" "),
            Event::HardBreak => state.push_text("\n"),
            Event::Rule => {
                state.flush_paragraph();
                state.blocks.push(Block::Rule);
            }
            Event::TaskListMarker(checked) => {
                state.push_text(if checked { "☒ " } else { "☐ " });
            }
            Event::FootnoteReference(name) => state.push_text(format!("[{name}]")),
            Event::InlineHtml(tag) if tag.trim().eq_ignore_ascii_case("<mark>") => {
                state.push_style(|style| style.highlight = true);
            }
            Event::InlineHtml(tag) if tag.trim().eq_ignore_ascii_case("</mark>") => {
                state.pop_style();
            }
            Event::Html(_) | Event::InlineHtml(_) => {}
            _ => {}
        }
    }
    state.flush_paragraph();
    if let Some(code) = state.code.take() {
        state.blocks.push(Block::Code(code));
    }
    state.blocks
}

fn document_xml(blocks: &[Block]) -> String {
    let mut body = String::new();
    for block in blocks {
        match block {
            Block::Paragraph(paragraph) => body.push_str(&paragraph_xml(paragraph)),
            Block::Code(code) => body.push_str(&code_block_xml(code)),
            Block::Rule => body.push_str(
                r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="B7BCC5"/></w:pBdr><w:spacing w:before="120" w:after="120"/></w:pPr></w:p>"#,
            ),
            Block::Table(rows) => body.push_str(&table_xml(rows)),
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/><w:cols w:space="720"/><w:docGrid w:linePitch="360"/></w:sectPr></w:body></w:document>"#
    )
}

fn paragraph_xml(paragraph: &Paragraph) -> String {
    let (properties, size, force_bold) = match paragraph.kind {
        ParagraphKind::Normal => (
            r#"<w:pPr><w:spacing w:after="160" w:line="396" w:lineRule="auto"/></w:pPr>"#.to_owned(),
            22,
            false,
        ),
        ParagraphKind::Heading(level) => {
            let size = match level {
                1 => 36,
                2 => 30,
                3 => 26,
                4 => 24,
                _ => 22,
            };
            (
                format!(r#"<w:pPr><w:pStyle w:val="Heading{level}"/><w:keepNext/><w:spacing w:before="280" w:after="120"/></w:pPr>"#),
                size,
                true,
            )
        }
        ParagraphKind::Quote => (
            r#"<w:pPr><w:pBdr><w:left w:val="single" w:sz="18" w:space="12" w:color="7C3AED"/></w:pBdr><w:shd w:val="clear" w:fill="F5F3FF"/><w:spacing w:before="100" w:after="180" w:line="396" w:lineRule="auto"/><w:ind w:left="720" w:right="240"/></w:pPr>"#.to_owned(),
            22,
            false,
        ),
        ParagraphKind::List { ordered, level } => (
            format!(r#"<w:pPr><w:numPr><w:ilvl w:val="{}"/><w:numId w:val="{}"/></w:numPr><w:spacing w:after="80" w:line="396" w:lineRule="auto"/></w:pPr>"#, level.min(8), if ordered { 2 } else { 1 }),
            22,
            false,
        ),
    };
    let runs = paragraph
        .inlines
        .iter()
        .map(|inline| run_xml(inline, size, force_bold))
        .collect::<String>();
    format!("<w:p>{properties}{runs}</w:p>")
}

fn run_xml(inline: &Inline, size: u16, force_bold: bool) -> String {
    let lao = contains_lao(&inline.text);
    let bold = force_bold || inline.style.bold;
    let font = if lao {
        "Phetsarath OT"
    } else if inline.style.code {
        "Consolas"
    } else {
        "Aptos"
    };
    let mut properties = format!(
        r#"<w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="Phetsarath OT"/>"#
    );
    if bold {
        properties.push_str("<w:b/>");
        if lao {
            properties.push_str("<w:bCs/>");
        }
    }
    if inline.style.italic {
        properties.push_str("<w:i/>");
        if lao {
            properties.push_str("<w:iCs/>");
        }
    }
    if inline.style.strike {
        properties.push_str("<w:strike/>");
    }
    if inline.style.link.is_some() {
        properties.push_str(r#"<w:color w:val="1769E0"/>"#);
    }
    properties.push_str(&format!(r#"<w:sz w:val="{size}"/>"#));
    if lao {
        properties.push_str(&format!(r#"<w:szCs w:val="{size}"/>"#));
    }
    if inline.style.highlight {
        properties.push_str(r#"<w:highlight w:val="yellow"/>"#);
    }
    if inline.style.link.is_some() {
        properties.push_str(r#"<w:u w:val="single"/>"#);
    }
    if lao {
        properties.push_str(r#"<w:cs/><w:lang w:val="en-US" w:bidi="lo-LA"/>"#);
    }
    properties.push_str("</w:rPr>");

    let mut text = String::new();
    for (index, part) in inline.text.split('\n').enumerate() {
        if index > 0 {
            text.push_str("<w:br/>");
        }
        text.push_str(&format!(
            r#"<w:t xml:space="preserve">{}</w:t>"#,
            xml_text(part)
        ));
    }
    format!("<w:r>{properties}{text}</w:r>")
}

fn code_block_xml(code: &str) -> String {
    let inline = Inline {
        text: code.to_owned(),
        style: InlineStyle {
            code: true,
            ..InlineStyle::default()
        },
    };
    format!(
        r#"<w:p><w:pPr><w:pBdr><w:top w:val="single" w:sz="4" w:color="D7DAE0"/><w:left w:val="single" w:sz="4" w:color="D7DAE0"/><w:bottom w:val="single" w:sz="4" w:color="D7DAE0"/><w:right w:val="single" w:sz="4" w:color="D7DAE0"/></w:pBdr><w:shd w:val="clear" w:fill="F3F4F6"/><w:spacing w:before="120" w:after="180" w:line="320" w:lineRule="auto"/><w:ind w:left="360" w:right="360"/></w:pPr>{}</w:p>"#,
        run_xml(&inline, 19, false)
    )
}

fn table_xml(rows: &[Vec<Vec<Inline>>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let width = 9000 / columns;
    let grid = (0..columns)
        .map(|_| format!(r#"<w:gridCol w:w="{width}"/>"#))
        .collect::<String>();
    let body = rows
        .iter()
        .map(|row| {
            let cells = (0..columns)
                .map(|index| {
                    let contents = row
                        .get(index)
                        .into_iter()
                        .flatten()
                        .map(|inline| run_xml(inline, 20, false))
                        .collect::<String>();
                    format!(r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/><w:tcMar><w:top w:w="80" w:type="dxa"/><w:left w:w="100" w:type="dxa"/><w:bottom w:w="80" w:type="dxa"/><w:right w:w="100" w:type="dxa"/></w:tcMar></w:tcPr><w:p><w:pPr><w:spacing w:after="0"/></w:pPr>{contents}</w:p></w:tc>"#)
                })
                .collect::<String>();
            format!("<w:tr>{cells}</w:tr>")
        })
        .collect::<String>();
    format!(r#"<w:tbl><w:tblPr><w:tblW w:w="9000" w:type="dxa"/><w:tblBorders><w:top w:val="single" w:sz="4" w:color="B7BCC5"/><w:left w:val="single" w:sz="4" w:color="B7BCC5"/><w:bottom w:val="single" w:sz="4" w:color="B7BCC5"/><w:right w:val="single" w:sz="4" w:color="B7BCC5"/><w:insideH w:val="single" w:sz="4" w:color="D7DAE0"/><w:insideV w:val="single" w:sz="4" w:color="D7DAE0"/></w:tblBorders><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{body}</w:tbl>"#)
}

fn content_types_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>"#.to_owned()
}

fn package_relationships_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#.to_owned()
}

fn document_relationships_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#.to_owned()
}

fn settings_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:zoom w:percent="100"/><w:defaultTabStop w:val="720"/><w:characterSpacingControl w:val="doNotCompress"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat><w:themeFontLang w:val="en-US" w:bidi="lo-LA"/></w:settings>"#.to_owned()
}

fn styles_xml() -> String {
    let mut headings = String::new();
    for level in 1..=6 {
        headings.push_str(&format!(r#"<w:style w:type="paragraph" w:styleId="Heading{level}"><w:name w:val="heading {level}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="240" w:after="120"/><w:outlineLvl w:val="{}"/></w:pPr><w:rPr><w:rFonts w:ascii="Phetsarath OT" w:hAnsi="Phetsarath OT" w:cs="Phetsarath OT"/><w:b/><w:bCs/><w:sz w:val="{}"/><w:szCs w:val="{}"/><w:cs/><w:lang w:val="en-US" w:bidi="lo-LA"/></w:rPr></w:style>"#, level - 1, 38_u16.saturating_sub((level - 1) as u16 * 4), 38_u16.saturating_sub((level - 1) as u16 * 4)));
    }
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Aptos" w:hAnsi="Aptos" w:cs="Phetsarath OT"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="en-US" w:bidi="lo-LA"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="396" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/><w:rPr><w:rFonts w:ascii="Aptos" w:hAnsi="Aptos" w:cs="Phetsarath OT"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="en-US" w:bidi="lo-LA"/></w:rPr></w:style>{headings}</w:styles>"#)
}

fn numbering_xml() -> String {
    let bullet_levels = (0..9)
        .map(|level| format!(r#"<w:lvl w:ilvl="{level}"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val=""/><w:lvlJc w:val="left"/><w:pPr><w:tabs><w:tab w:val="num" w:pos="{}"/></w:tabs><w:ind w:left="{}" w:hanging="360"/></w:pPr><w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol"/></w:rPr></w:lvl>"#, 720 + level * 360, 720 + level * 360))
        .collect::<String>();
    let number_levels = (0..9)
        .map(|level| format!(r#"<w:lvl w:ilvl="{level}"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%{})."/><w:lvlJc w:val="left"/><w:pPr><w:tabs><w:tab w:val="num" w:pos="{}"/></w:tabs><w:ind w:left="{}" w:hanging="360"/></w:pPr></w:lvl>"#, level + 1, 720 + level * 360, 720 + level * 360))
        .collect::<String>();
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="hybridMultilevel"/>{bullet_levels}</w:abstractNum><w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="multilevel"/>{number_levels}</w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num></w:numbering>"#)
}

fn core_properties_xml(title: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title><dc:creator>MiraMD</dc:creator><cp:lastModifiedBy>MiraMD</cp:lastModifiedBy></cp:coreProperties>"#, xml_text(title))
}

fn app_properties_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>MiraMD</Application><AppVersion>0.1</AppVersion></Properties>"#.to_owned()
}

fn contains_lao(value: &str) -> bool {
    value.chars().any(|character| matches!(character, '\u{0E80}'..='\u{0EFF}'))
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn xml_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            matches!(*character, '\u{9}' | '\u{A}' | '\u{D}' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
        })
        .fold(String::new(), |mut output, character| {
            match character {
                '&' => output.push_str("&amp;"),
                '<' => output.push_str("&lt;"),
                '>' => output.push_str("&gt;"),
                _ => output.push(character),
            }
            output
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use zip::ZipArchive;

    fn entry(bytes: Vec<u8>, name: &str) -> String {
        let mut archive = ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut value = String::new();
        archive.by_name(name).unwrap().read_to_string(&mut value).unwrap();
        value
    }

    #[test]
    fn exports_valid_ooxml_package_with_markdown_structure() {
        let bytes = export_docx("# Title\n\n- one\n- two\n\n**bold**", "Test").unwrap();
        assert!(bytes.starts_with(b"PK"));
        let document = entry(bytes, "word/document.xml");
        assert!(document.contains(r#"w:pStyle w:val="Heading1""#));
        assert!(document.contains(r#"w:numId w:val="1""#));
        assert!(document.contains("<w:b/>"));
    }

    #[test]
    fn lao_runs_have_complex_script_font_style_size_and_language() {
        let bytes = export_docx("# **ສະບາຍດີ**\n\n*ພາສາລາວ*", "ເອກະສານ").unwrap();
        let document = entry(bytes.clone(), "word/document.xml");
        assert!(document.contains(r#"w:ascii="Phetsarath OT" w:hAnsi="Phetsarath OT" w:cs="Phetsarath OT""#));
        assert!(!document.contains("Phetsarath Lao"));
        assert!(document.contains("<w:bCs/>"));
        assert!(document.contains("<w:iCs/>"));
        assert!(document.contains("<w:szCs"));
        assert!(document.contains("<w:cs/>"));
        assert!(document.contains(r#"w:bidi="lo-LA""#));
        assert!(document.contains("ສະບາຍດີ"));
        let settings = entry(bytes, "word/settings.xml");
        assert!(settings.contains(r#"<w:themeFontLang w:val="en-US" w:bidi="lo-LA"/>"#));
    }

    #[test]
    fn highlights_become_native_word_highlights_without_markers() {
        let bytes = export_docx("Important ==human note ສຳຄັນ== here.", "Highlights").unwrap();
        let document = entry(bytes, "word/document.xml");
        assert!(document.contains(r#"<w:highlight w:val="yellow"/>"#));
        assert!(document.contains("human note ສຳຄັນ"));
        assert!(!document.contains("==human note"));
    }
}
