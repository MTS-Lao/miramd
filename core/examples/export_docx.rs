use std::{env, fs, process};

fn main() {
    let arguments = env::args().collect::<Vec<_>>();
    if arguments.len() != 3 {
        eprintln!("Usage: export_docx INPUT.md OUTPUT.docx");
        process::exit(2);
    }
    let markdown = fs::read_to_string(&arguments[1]).unwrap_or_else(|error| {
        eprintln!("Could not read {}: {error}", arguments[1]);
        process::exit(1);
    });
    let title = std::path::Path::new(&arguments[1])
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("MiraMD document");
    if let Err(error) = miramd_core::export_docx_to_path(&markdown, title, &arguments[2]) {
        eprintln!("{error}");
        process::exit(1);
    }
}
