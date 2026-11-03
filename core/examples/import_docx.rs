use std::{env, fs, process};

fn main() {
    let arguments = env::args().collect::<Vec<_>>();
    if !(2..=3).contains(&arguments.len()) {
        eprintln!("Usage: import_docx INPUT.docx [OUTPUT.md]");
        process::exit(2);
    }
    let imported = miramd_core::import_docx_from_path(&arguments[1]).unwrap_or_else(|error| {
        eprintln!("{error}");
        process::exit(1);
    });
    if let Some(output) = arguments.get(2) {
        fs::write(output, imported.markdown).unwrap_or_else(|error| {
            eprintln!("Could not write {output}: {error}");
            process::exit(1);
        });
    } else {
        print!("{}", imported.markdown);
    }
    for warning in imported.warnings {
        eprintln!("Warning: {warning}");
    }
}
