//! Shared Markdown behavior for MiraMD.
//!
//! The public Rust functions are independently testable. The small C ABI at the
//! bottom is intentionally string-based so Swift and C# can consume the same
//! engine without generated bindings.

use pulldown_cmark::{html, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Serialize;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

mod docx;
pub use docx::{export_docx, export_docx_to_path};

const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentStats {
    pub words: usize,
    pub characters: usize,
    pub lines: usize,
    pub reading_minutes: usize,
}

fn markdown_options() -> Options {
    Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_HEADING_ATTRIBUTES
}

/// Render Markdown to a sanitized HTML fragment suitable for an embedded preview.
/// Scripts, event handlers, iframes, and other active content are removed.
pub fn render_html(markdown: &str) -> String {
    let highlighted = highlight_markup(markdown);
    let parser = Parser::new_ext(&highlighted, markdown_options());
    let mut rendered = String::with_capacity(markdown.len() + markdown.len() / 2);
    html::push_html(&mut rendered, parser);

    ammonia::Builder::default()
        .add_tags(["mark"])
        .add_generic_attributes(["class", "id"])
        .clean(&rendered)
        .to_string()
}

pub fn outline(markdown: &str) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut active: Option<(u8, usize, String)> = None;
    let highlighted = highlight_markup(markdown);

    for (event, range) in Parser::new_ext(&highlighted, markdown_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let line = highlighted.as_bytes()[..range.start]
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count()
                    + 1;
                active = Some((heading_level(level), line, String::new()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, _, current)) = active.as_mut() {
                    current.push_str(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, _, current)) = active.as_mut() {
                    current.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, line, text)) = active.take() {
                    headings.push(Heading {
                        level,
                        text: text.trim().to_owned(),
                        line,
                    });
                }
            }
            _ => {}
        }
    }

    headings
}

pub fn document_stats(markdown: &str) -> DocumentStats {
    let words = markdown.split_whitespace().count();
    DocumentStats {
        words,
        characters: markdown.chars().count(),
        lines: if markdown.is_empty() {
            0
        } else {
            markdown.bytes().filter(|byte| *byte == b'\n').count() + 1
        },
        reading_minutes: if words == 0 { 0 } else { words.div_ceil(220) },
    }
}

/// Build a paste-safe prompt attachment. The fence grows when the source itself
/// contains long backtick runs, so nested code examples cannot escape it.
pub fn export_ai_bundle(markdown: &str, filename: &str) -> String {
    let clean = clean_markdown(markdown);
    let fence = "`".repeat(longest_backtick_run(&clean).saturating_add(1).max(4));
    let safe_name = filename
        .chars()
        .map(|character| match character {
            '\r' | '\n' | '`' => ' ',
            other => other,
        })
        .collect::<String>();

    format!(
        "# Document context: {}\n\nUse the following Markdown document as source material. Preserve its meaning and structure unless asked to change them.\n\n{}markdown\n{}\n{}\n",
        safe_name.trim(),
        fence,
        clean.trim_end(),
        fence
    )
}

/// Remove MiraMD's portable `==highlight==` markers while preserving their text.
/// Markers inside inline or fenced code remain literal.
pub fn clean_markdown(markdown: &str) -> String {
    rewrite_highlights(markdown, false)
}

/// Convert MiraMD highlight markers to safe inline HTML understood by the
/// Markdown renderer and DOCX event parser.
pub(crate) fn highlight_markup(markdown: &str) -> String {
    rewrite_highlights(markdown, true)
}

fn rewrite_highlights(markdown: &str, as_html: bool) -> String {
    let mut output = String::with_capacity(markdown.len() + markdown.len() / 8);
    let mut fence: Option<(u8, usize)> = None;

    for segment in markdown.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map_or((segment, ""), |line| (line, "\n"));

        if let Some((fence_character, fence_length)) = fence {
            output.push_str(line);
            if let Some((character, length, remainder)) = fence_marker(line) {
                if character == fence_character
                    && length >= fence_length
                    && remainder.trim().is_empty()
                {
                    fence = None;
                }
            }
        } else if let Some((character, length, _)) = fence_marker(line) {
            fence = Some((character, length));
            output.push_str(line);
        } else {
            output.push_str(&rewrite_inline_highlights(line, as_html));
        }
        output.push_str(newline);
    }
    output
}

fn fence_marker(line: &str) -> Option<(u8, usize, &str)> {
    let bytes = line.as_bytes();
    let mut start = 0;
    while start < bytes.len() && start < 3 && bytes[start] == b' ' {
        start += 1;
    }
    let character = *bytes.get(start)?;
    if character != b'`' && character != b'~' {
        return None;
    }
    let length = bytes[start..]
        .iter()
        .take_while(|value| **value == character)
        .count();
    (length >= 3).then(|| (character, length, &line[start + length..]))
}

fn rewrite_inline_highlights(line: &str, as_html: bool) -> String {
    let bytes = line.as_bytes();
    let mut output = String::with_capacity(line.len() + line.len() / 8);
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'\\' {
            let next = next_character_end(line, index + 1);
            output.push_str(&line[index..next]);
            index = next;
            continue;
        }
        if bytes[index] == b'`' {
            let run = bytes[index..].iter().take_while(|value| **value == b'`').count();
            if let Some(end) = matching_backtick_end(bytes, index + run, run) {
                output.push_str(&line[index..end]);
                index = end;
            } else {
                output.push_str(&line[index..index + run]);
                index += run;
            }
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"==")
            && bytes.get(index + 2).is_some_and(|value| !value.is_ascii_whitespace())
        {
            if let Some(close) = highlight_close(bytes, index + 2) {
                if as_html {
                    output.push_str("<mark>");
                }
                output.push_str(&line[index + 2..close]);
                if as_html {
                    output.push_str("</mark>");
                }
                index = close + 2;
                continue;
            }
        }

        let next = next_character_end(line, index);
        output.push_str(&line[index..next]);
        index = next;
    }
    output
}

fn matching_backtick_end(bytes: &[u8], mut index: usize, length: usize) -> Option<usize> {
    while index < bytes.len() {
        if bytes[index] == b'`' {
            let run = bytes[index..].iter().take_while(|value| **value == b'`').count();
            if run == length {
                return Some(index + run);
            }
            index += run;
        } else {
            index += 1;
        }
    }
    None
}

fn highlight_close(bytes: &[u8], mut index: usize) -> Option<usize> {
    while index + 1 < bytes.len() {
        if bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
            continue;
        }
        if bytes[index] == b'`' {
            let run = bytes[index..].iter().take_while(|value| **value == b'`').count();
            if let Some(end) = matching_backtick_end(bytes, index + run, run) {
                index = end;
                continue;
            }
            index += run;
            continue;
        }
        if &bytes[index..index + 2] == b"==" && !bytes[index - 1].is_ascii_whitespace() {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn next_character_end(value: &str, index: usize) -> usize {
    if index >= value.len() {
        return value.len();
    }
    index + value[index..].chars().next().map_or(1, char::len_utf8)
}

fn longest_backtick_run(input: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for character in input.chars() {
        if character == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
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

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

fn read_input<'a>(input: *const c_char) -> Result<&'a str, ()> {
    if input.is_null() {
        return Err(());
    }
    // SAFETY: Callers promise a non-null, NUL-terminated pointer that remains
    // valid for the duration of this synchronous call.
    unsafe { CStr::from_ptr(input) }.to_str().map_err(|_| ())
}

fn return_string(value: String) -> *mut c_char {
    let without_nuls = value.replace('\0', "�");
    CString::new(without_nuls)
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

/// Release every string returned by this library.
#[no_mangle]
pub extern "C" fn miramd_string_free(value: *mut c_char) {
    if value.is_null() {
        return;
    }
    // SAFETY: `value` must have been returned by `CString::into_raw` above and
    // may be freed exactly once.
    unsafe { drop(CString::from_raw(value)) };
}

#[no_mangle]
pub extern "C" fn miramd_engine_version() -> *mut c_char {
    return_string(ENGINE_VERSION.to_owned())
}

#[no_mangle]
pub extern "C" fn miramd_render_html(markdown: *const c_char) -> *mut c_char {
    match read_input(markdown) {
        Ok(value) => return_string(render_html(value)),
        Err(()) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn miramd_outline_json(markdown: *const c_char) -> *mut c_char {
    match read_input(markdown) {
        Ok(value) => return_string(json(&outline(value))),
        Err(()) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn miramd_stats_json(markdown: *const c_char) -> *mut c_char {
    match read_input(markdown) {
        Ok(value) => return_string(json(&document_stats(value))),
        Err(()) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn miramd_export_ai(
    markdown: *const c_char,
    filename: *const c_char,
) -> *mut c_char {
    match (read_input(markdown), read_input(filename)) {
        (Ok(markdown), Ok(filename)) => return_string(export_ai_bundle(markdown, filename)),
        _ => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn miramd_clean_markdown(markdown: *const c_char) -> *mut c_char {
    match read_input(markdown) {
        Ok(value) => return_string(clean_markdown(value)),
        Err(()) => std::ptr::null_mut(),
    }
}

/// Export Markdown to a Word document. An empty returned string means success;
/// otherwise the returned UTF-8 string contains a user-facing error.
#[no_mangle]
pub extern "C" fn miramd_export_docx(
    markdown: *const c_char,
    title: *const c_char,
    path: *const c_char,
) -> *mut c_char {
    match (read_input(markdown), read_input(title), read_input(path)) {
        (Ok(markdown), Ok(title), Ok(path)) => match export_docx_to_path(markdown, title, path) {
            Ok(()) => return_string(String::new()),
            Err(error) => return_string(error),
        },
        _ => return_string("MiraMD received invalid text while exporting Word format.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_common_markdown_and_removes_active_html() {
        let rendered = render_html("# Hello\n\n~~old~~\n\n<script>alert(1)</script>");
        assert!(rendered.contains("<h1>Hello</h1>"));
        assert!(rendered.contains("<del>old</del>"));
        assert!(!rendered.contains("<script>"));
    }

    #[test]
    fn renders_highlights_but_not_inside_code() {
        let rendered = render_html("Read ==this Lao ຂໍ້ຄວາມ== and `x == y`.\n\n```\n==literal==\n```");
        assert!(rendered.contains("<mark>this Lao ຂໍ້ຄວາມ</mark>"));
        assert!(rendered.contains("<code>x == y</code>"));
        assert!(rendered.contains("==literal=="));
    }

    #[test]
    fn clean_markdown_removes_only_real_highlight_markers() {
        let clean = clean_markdown("Keep ==these words== and `==inline code==`.\n\n```\n==block==\n```");
        assert_eq!(clean, "Keep these words and `==inline code==`.\n\n```\n==block==\n```");
        let exported = export_ai_bundle("Use ==this==", "notes.md");
        assert!(exported.contains("Use this"));
        assert!(!exported.contains("==this=="));
    }

    #[test]
    fn extracts_heading_levels_and_lines() {
        assert_eq!(
            outline("# One\ntext\n\n### Three *with style*\n"),
            vec![
                Heading {
                    level: 1,
                    text: "One".into(),
                    line: 1,
                },
                Heading {
                    level: 3,
                    text: "Three with style".into(),
                    line: 4,
                },
            ]
        );
    }

    #[test]
    fn ai_export_uses_a_fence_longer_than_nested_code() {
        let exported = export_ai_bundle("````rust\nfn main() {}\n````", "notes.md");
        assert!(exported.contains("`````markdown"));
        assert!(exported.ends_with("`````\n"));
    }

    #[test]
    fn stats_round_reading_time_up() {
        let markdown = std::iter::repeat("word")
            .take(221)
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(document_stats(&markdown).reading_minutes, 2);
    }
}
