/// Static file server for the React dashboard dist.
///
/// Serves files from a directory with correct MIME types.
/// SPA fallback: any path not matching a file returns index.html.
use std::path::{Path, PathBuf};

pub struct StaticFiles {
    root: PathBuf,
    index_html: String,
}

impl StaticFiles {
    pub fn new(root: PathBuf) -> Option<Self> {
        let root = root.canonicalize().ok()?;
        let index_path = root.join("index.html");
        let index_html = std::fs::read_to_string(&index_path).ok()?;
        Some(Self { root, index_html })
    }

    /// Try to serve a static file. Returns (status_line, content_type, body).
    /// If the path doesn't match a file, returns the SPA fallback (index.html).
    pub fn serve(&self, path: &str) -> (String, String, String) {
        // Strip query string
        let clean = path.split('?').next().unwrap_or(path);

        // Normalize: strip leading slash, resolve
        let rel = clean.strip_prefix('/').unwrap_or(clean);

        // Security: no path traversal
        if rel.contains("..") {
            return self.spa_fallback();
        }

        let file_path = self.root.join(rel);

        // Security: ensure resolved path is still under root
        match file_path.canonicalize() {
            Ok(canonical) => {
                if !canonical.starts_with(&self.root) {
                    return self.spa_fallback();
                }
            }
            Err(_) => return self.spa_fallback(),
        }

        // Try to read the file
        match std::fs::read(&file_path) {
            Ok(contents) => {
                let mime = mime_from_path(&file_path);
                let status = "HTTP/1.1 200 OK";
                (status.to_string(), mime, String::from_utf8_lossy(&contents).to_string())
            }
            Err(_) => self.spa_fallback(),
        }
    }

    fn spa_fallback(&self) -> (String, String, String) {
        (
            "HTTP/1.1 200 OK".to_string(),
            "text/html; charset=utf-8".to_string(),
            self.index_html.clone(),
        )
    }
}

fn mime_from_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
    .to_string()
}
