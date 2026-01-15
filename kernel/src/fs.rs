//! Minimal in-memory filesystem for exec()

pub struct FileEntry {
    pub path: &'static str,
    pub data: &'static [u8],
}

static FILES: &[FileEntry] = &[
    FileEntry { path: "/init", data: include_bytes!("../../userland/bin/init") },
    FileEntry { path: "/bin/sh", data: include_bytes!("../../userland/bin/sh") },
];

/// Look up a file by absolute path.
pub fn lookup(path: &str) -> Option<&'static [u8]> {
    FILES.iter().find(|entry| entry.path == path).map(|entry| entry.data)
}
