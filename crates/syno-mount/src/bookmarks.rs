//! Entries in the file manager's sidebar (GTK bookmarks: Nemo, Nautilus, Caja,
//! Thunar and the GTK file dialog read them).

use std::fs;
use std::io;
use std::path::Path;

/// "file:///home/anna/NAS/My%20Media", as GTK writes it.
pub fn uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

fn line(path: &Path, label: &str) -> String {
    format!("{} {label}", uri(path))
}

fn edit(file: &Path, change: impl FnOnce(Vec<String>) -> Vec<String>) -> io::Result<()> {
    let text = fs::read_to_string(file).unwrap_or_default();
    let before: Vec<String> = text.lines().map(str::to_owned).collect();
    let after = change(before.clone());
    if after == before {
        return Ok(());
    }
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut text = after.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    fs::write(file, text)
}

fn points_to(line: &str, path: &Path) -> bool {
    line.split(' ').next() == Some(uri(path).as_str())
}

/// Adds `path` with `label`, or renames its entry.
pub fn add(file: &Path, path: &Path, label: &str) -> io::Result<()> {
    edit(file, |mut lines| {
        let entry = line(path, label);
        match lines.iter_mut().find(|existing| points_to(existing, path)) {
            Some(existing) => *existing = entry,
            None => lines.push(entry),
        }
        lines
    })
}

pub fn remove(file: &Path, path: &Path) -> io::Result<()> {
    edit(file, |lines| {
        lines
            .into_iter()
            .filter(|existing| !points_to(existing, path))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_renames_and_removes_only_its_entry() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("gtk-3.0/bookmarks");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "file:///home/anna/Documents\n").unwrap();
        let path = Path::new("/home/anna/NAS/nas/My Media");

        add(&file, path, "Media on nas").unwrap();
        add(&file, path, "Media (nas)").unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "file:///home/anna/Documents\nfile:///home/anna/NAS/nas/My%20Media Media (nas)\n"
        );
        remove(&file, path).unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "file:///home/anna/Documents\n"
        );
    }
}
