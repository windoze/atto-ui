use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::ffi::OsStr;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use atto_ui_file_tree::{FileTreeNode, FileTreeNodeId, FileTreeNodeKind};

#[derive(Debug, Clone)]
pub struct WorkspaceTree {
    pub roots: Vec<FileTreeNode>,
    pub id_to_path: HashMap<FileTreeNodeId, PathBuf>,
    pub id_to_kind: HashMap<FileTreeNodeId, FileTreeNodeKind>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceTreeOptions {
    pub max_depth: usize,
    pub max_entries_per_dir: usize,
    pub show_hidden: bool,
    pub ignore_git_dir: bool,
}

impl Default for WorkspaceTreeOptions {
    fn default() -> Self {
        Self {
            max_depth: 16,
            max_entries_per_dir: 10_000,
            show_hidden: false,
            ignore_git_dir: true,
        }
    }
}

pub fn build_workspace_tree(roots: &[PathBuf], options: WorkspaceTreeOptions) -> WorkspaceTree {
    let mut id_to_path: HashMap<FileTreeNodeId, PathBuf> = HashMap::new();
    let mut id_to_kind: HashMap<FileTreeNodeId, FileTreeNodeKind> = HashMap::new();

    let roots = roots
        .iter()
        .filter_map(|p| canonicalize_best_effort(p))
        .collect::<Vec<_>>();

    let mut out_roots = Vec::new();
    for root in roots {
        if let Some(node) = build_node(&root, 0, &options, &mut id_to_path, &mut id_to_kind) {
            out_roots.push(node.with_expanded(true));
        }
    }

    WorkspaceTree {
        roots: out_roots,
        id_to_path,
        id_to_kind,
    }
}

fn canonicalize_best_effort(path: &Path) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return None;
    }
    fs::canonicalize(path)
        .ok()
        .or_else(|| Some(path.to_path_buf()))
}

fn build_node(
    path: &Path,
    depth: usize,
    options: &WorkspaceTreeOptions,
    id_to_path: &mut HashMap<FileTreeNodeId, PathBuf>,
    id_to_kind: &mut HashMap<FileTreeNodeId, FileTreeNodeKind>,
) -> Option<FileTreeNode> {
    let meta = fs::metadata(path).ok()?;
    let name = display_name_for_path(path);
    let id = node_id_for_path(path);

    let kind = if meta.is_dir() {
        FileTreeNodeKind::Directory
    } else {
        FileTreeNodeKind::File
    };

    id_to_path.insert(id, path.to_path_buf());
    id_to_kind.insert(id, kind);

    if kind == FileTreeNodeKind::File {
        return Some(FileTreeNode::file(id, name));
    }

    if depth >= options.max_depth {
        return Some(FileTreeNode::dir(id, name, Vec::new()));
    }

    let mut children = read_dir_sorted(path, options)
        .into_iter()
        .filter_map(|child| build_node(&child, depth + 1, options, id_to_path, id_to_kind))
        .take(options.max_entries_per_dir)
        .collect::<Vec<_>>();

    // Show directories first to match typical file explorers.
    children.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a
            .name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase()),
    });

    Some(FileTreeNode::dir(id, name, children))
}

fn read_dir_sorted(dir: &Path, options: &WorkspaceTreeOptions) -> Vec<PathBuf> {
    let mut entries = Vec::<PathBuf>::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return entries;
    };

    for entry in rd.flatten() {
        let p = entry.path();
        if should_skip_entry(&p, options) {
            continue;
        }
        entries.push(p);
    }

    entries.sort_by(|a, b| {
        display_name_for_path(a)
            .to_ascii_lowercase()
            .cmp(&display_name_for_path(b).to_ascii_lowercase())
    });

    entries
}

fn should_skip_entry(path: &Path, options: &WorkspaceTreeOptions) -> bool {
    if !options.show_hidden
        && let Some(name) = path.file_name().and_then(OsStr::to_str)
        && name.starts_with('.')
    {
        return true;
    }

    if options.ignore_git_dir && path.file_name() == Some(OsStr::new(".git")) {
        return true;
    }

    false
}

fn display_name_for_path(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn node_id_for_path(path: &Path) -> FileTreeNodeId {
    let mut hasher = DefaultHasher::new();
    // Use a stable string representation; best-effort canonicalization is done by caller.
    path.to_string_lossy().hash(&mut hasher);
    FileTreeNodeId::new(hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug)]
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let path = unique_temp_dir(prefix);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("atto_editor_app_{prefix}_{nanos}"))
    }

    fn collect_child_names(node: &FileTreeNode) -> Vec<String> {
        node.children.iter().map(|c| c.name.clone()).collect()
    }

    #[test]
    fn build_workspace_tree_filters_hidden_and_git_dir() {
        let root = TempDir::new("filters");
        fs::create_dir_all(root.path.join("src")).unwrap();
        fs::create_dir_all(root.path.join(".git")).unwrap();
        fs::write(root.path.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.path.join("README.md"), "# readme\n").unwrap();
        fs::write(root.path.join(".hidden"), "secret\n").unwrap();
        fs::write(root.path.join(".git").join("config"), "ignore\n").unwrap();

        let tree = build_workspace_tree(
            std::slice::from_ref(&root.path),
            WorkspaceTreeOptions::default(),
        );
        assert_eq!(tree.roots.len(), 1);

        let names = collect_child_names(&tree.roots[0]);
        assert!(names.contains(&"src".to_string()));
        assert!(names.contains(&"README.md".to_string()));
        assert!(!names.contains(&".hidden".to_string()));
        assert!(!names.contains(&".git".to_string()));
    }

    #[test]
    fn build_workspace_tree_sorts_directories_before_files() {
        let root = TempDir::new("sort");
        fs::create_dir_all(root.path.join("b_dir")).unwrap();
        fs::create_dir_all(root.path.join("a_dir")).unwrap();
        fs::write(root.path.join("b_file.txt"), "b\n").unwrap();
        fs::write(root.path.join("a_file.txt"), "a\n").unwrap();

        let tree = build_workspace_tree(
            std::slice::from_ref(&root.path),
            WorkspaceTreeOptions::default(),
        );
        assert_eq!(tree.roots.len(), 1);

        let names = collect_child_names(&tree.roots[0]);
        assert_eq!(
            names,
            vec![
                "a_dir".to_string(),
                "b_dir".to_string(),
                "a_file.txt".to_string(),
                "b_file.txt".to_string(),
            ]
        );
    }

    #[test]
    fn build_workspace_tree_respects_max_depth() {
        let root = TempDir::new("depth");
        fs::create_dir_all(root.path.join("a").join("b")).unwrap();
        fs::write(root.path.join("a").join("b").join("file.txt"), "hi\n").unwrap();

        let options = WorkspaceTreeOptions {
            max_depth: 0,
            ..Default::default()
        };
        let tree = build_workspace_tree(std::slice::from_ref(&root.path), options);
        assert_eq!(tree.roots.len(), 1);
        assert!(tree.roots[0].children.is_empty());
    }
}
