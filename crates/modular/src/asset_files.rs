//! Where the files a patch names live: `/...` paths in the workspace folder,
//! `builtin/...` paths in the assets bundled with the app.

use napi::Result;
use std::path::{Path, PathBuf};

/// The prefix of a path naming an asset bundled with the app. A workspace path
/// starts with `/`, so the two never collide.
pub const BUILTIN_PREFIX: &str = "builtin/";

/// Why `file_inside` found no file.
enum FileInsideError {
    /// The path is not plain names, or it leads out of the folder.
    Outside,
    /// The file or the folder cannot be read.
    Unreadable(std::io::Error),
}

/// Whether links in a folder may lead out of it.
#[derive(Clone, Copy)]
enum Links {
    /// Followed wherever they point: a workspace can link in a sample library.
    Followed,
    /// Followed only to files inside the folder: the bundled assets.
    Contained,
}

/// The file `relative` names inside `root`, with links followed as `links`
/// allows.
fn file_inside(
    root: &Path,
    relative: &str,
    links: Links,
) -> std::result::Result<PathBuf, FileInsideError> {
    let relative = Path::new(relative);
    let mut components = relative.components().peekable();
    if components.peek().is_none()
        || !components.all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err(FileInsideError::Outside);
    }
    let file = std::fs::canonicalize(root.join(relative)).map_err(FileInsideError::Unreadable)?;
    if let Links::Contained = links {
        let root = std::fs::canonicalize(root).map_err(FileInsideError::Unreadable)?;
        if !file.starts_with(&root) {
            return Err(FileInsideError::Outside);
        }
    }
    Ok(file)
}

/// The two folders asset paths resolve against. Either is empty until the
/// host sets it.
#[derive(Default)]
pub struct AssetResolver {
    workspace_path: PathBuf,
    /// The folder `builtin/...` paths name: the assets bundled with the app.
    builtin_path: PathBuf,
}

impl AssetResolver {
    pub fn workspace_path(&self) -> &Path {
        &self.workspace_path
    }

    /// Sets the workspace folder, returning whether it changed.
    pub fn set_workspace_path(&mut self, path: PathBuf) -> bool {
        let changed = self.workspace_path != path;
        self.workspace_path = path;
        changed
    }

    pub fn set_builtin_path(&mut self, path: PathBuf) {
        self.builtin_path = path;
    }

    /// The file `path` names: a workspace-rooted `/...` path or a bundled
    /// `builtin/...` one. Every segment after the prefix must be a plain name.
    /// A workspace file may lie wherever its links lead; a bundled file, with
    /// links followed, must lie inside the bundled folder.
    pub fn file(&self, path: &str) -> Result<PathBuf> {
        if let Some(relative) = path.strip_prefix(BUILTIN_PREFIX) {
            return file_inside(&self.builtin_path, relative, Links::Contained).map_err(|e| {
                napi::Error::from_reason(match e {
                    FileInsideError::Outside => {
                        format!("path must name a built-in file: {path}")
                    }
                    FileInsideError::Unreadable(e) => format!("no built-in file {path} ({e})"),
                })
            });
        }
        if self.workspace_path.as_os_str().is_empty() {
            return Err(napi::Error::from_reason(format!(
                "open a workspace folder to load {path}"
            )));
        }
        let outside = || {
            napi::Error::from_reason(format!(
                "path must name a file inside the workspace folder: {path}"
            ))
        };
        let relative = path.strip_prefix('/').ok_or_else(outside)?;
        file_inside(&self.workspace_path, relative, Links::Followed).map_err(|e| match e {
            FileInsideError::Outside => outside(),
            FileInsideError::Unreadable(e) => {
                napi::Error::from_reason(format!("no file {path} in the workspace folder ({e})"))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AssetResolver;
    use std::path::PathBuf;

    /// Unique temp folder with a `wavs/` directory.
    fn temp_folder(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "modular_asset_files_{}_{}_{}",
            name,
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(path.join("wavs")).expect("create folder");
        path
    }

    #[test]
    fn paths_resolve_from_the_workspace_root_and_follow_its_links() {
        let workspace = temp_folder("paths");
        let outside = temp_folder("paths_outside");
        std::fs::File::create(workspace.join("wavs").join("kick.wav")).expect("write wav");
        std::fs::File::create(outside.join("wavs").join("linked.wav")).expect("write wav");
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("wavs"), workspace.join("linked")).expect("link");
        let mut assets = AssetResolver::default();
        assets.set_workspace_path(workspace.clone());

        let resolved = assets.file("/wavs/kick.wav");
        #[cfg(unix)]
        let linked = assets.file("/linked/linked.wav");
        let escapes: Vec<_> = [
            "wavs/kick.wav",
            "/",
            "//wavs/kick.wav",
            "/../kick.wav",
            "/wavs/../../kick.wav",
            "/./wavs/kick.wav",
        ]
        .into_iter()
        .filter(|bad| assets.file(bad).is_ok())
        .collect();
        let _ = std::fs::remove_dir_all(&workspace);
        let _ = std::fs::remove_dir_all(&outside);

        assert!(
            resolved.expect("rooted path").ends_with("wavs/kick.wav"),
            "a plain rooted path resolves inside the workspace"
        );
        #[cfg(unix)]
        assert!(
            linked.expect("linked path").ends_with("wavs/linked.wav"),
            "a link out of the workspace is followed"
        );
        assert!(escapes.is_empty(), "accepted {escapes:?}");
    }

    #[test]
    fn builtin_paths_resolve_from_the_builtin_folder_and_stay_inside_it() {
        let builtin = temp_folder("builtin");
        let outside = temp_folder("builtin_outside");
        std::fs::File::create(builtin.join("wavs").join("table.wav")).expect("write wav");
        std::fs::File::create(outside.join("wavs").join("secret.wav")).expect("write wav");
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("wavs"), builtin.join("linked")).expect("link");
        let mut assets = AssetResolver::default();
        assets.set_builtin_path(builtin.clone());

        let resolved = assets.file("builtin/wavs/table.wav");
        let escapes: Vec<_> = [
            "builtin/",
            "builtin//wavs/table.wav",
            "builtin/../wavs/table.wav",
            "builtin/./wavs/table.wav",
            "builtin/linked/secret.wav",
        ]
        .into_iter()
        .filter(|bad| assets.file(bad).is_ok())
        .collect();
        let _ = std::fs::remove_dir_all(&builtin);
        let _ = std::fs::remove_dir_all(&outside);

        assert!(
            resolved
                .expect("builtin path without a workspace")
                .ends_with("wavs/table.wav"),
            "a builtin path resolves inside the builtin folder"
        );
        assert!(escapes.is_empty(), "accepted {escapes:?}");
    }

    #[test]
    fn without_a_workspace_no_path_resolves() {
        let error = AssetResolver::default()
            .file("/kick.wav")
            .expect_err("no workspace");
        assert!(
            error.reason.contains("open a workspace folder"),
            "{}",
            error.reason
        );
    }
}
