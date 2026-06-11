use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::ffi::OsStr;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use atto_ui_file_tree::{FileTreeGitStatus, FileTreeNode, FileTreeNodeId, FileTreeNodeKind};

#[derive(Debug, Clone)]
pub struct WorkspaceTree {
    pub roots: Vec<FileTreeNode>,
    pub id_to_path: HashMap<FileTreeNodeId, PathBuf>,
    pub id_to_kind: HashMap<FileTreeNodeId, FileTreeNodeKind>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceGitStatuses {
    by_id: HashMap<FileTreeNodeId, FileTreeGitStatus>,
    by_path: HashMap<PathBuf, FileTreeGitStatus>,
}

impl WorkspaceGitStatuses {
    pub fn insert_id(&mut self, id: FileTreeNodeId, status: FileTreeGitStatus) {
        self.by_id.insert(id, status);
    }

    pub fn insert_path(&mut self, path: PathBuf, status: FileTreeGitStatus) {
        let path = canonicalize_best_effort(&path).unwrap_or(path);
        self.by_path.insert(path, status);
    }

    pub fn extend(&mut self, other: WorkspaceGitStatuses) {
        self.by_id.extend(other.by_id);
        self.by_path.extend(other.by_path);
    }

    pub fn status_for(&self, id: FileTreeNodeId, path: &Path) -> Option<FileTreeGitStatus> {
        self.by_id.get(&id).copied().or_else(|| {
            self.by_path.get(path).copied().or_else(|| {
                canonicalize_best_effort(path).and_then(|p| self.by_path.get(&p).copied())
            })
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFileEntry {
    pub path: PathBuf,
    pub display_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFileIndex {
    pub roots: Vec<PathBuf>,
    pub entries: Vec<WorkspaceFileEntry>,
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
    build_workspace_tree_with_git_statuses(roots, options, &WorkspaceGitStatuses::default())
}

pub fn build_workspace_tree_with_git_statuses(
    roots: &[PathBuf],
    options: WorkspaceTreeOptions,
    git_statuses: &WorkspaceGitStatuses,
) -> WorkspaceTree {
    let mut id_to_path: HashMap<FileTreeNodeId, PathBuf> = HashMap::new();
    let mut id_to_kind: HashMap<FileTreeNodeId, FileTreeNodeKind> = HashMap::new();

    let roots = roots
        .iter()
        .filter_map(|p| canonicalize_best_effort(p))
        .collect::<Vec<_>>();

    let mut out_roots = Vec::new();
    for root in roots {
        if let Some(node) = build_node(
            &root,
            0,
            &options,
            git_statuses,
            &mut id_to_path,
            &mut id_to_kind,
        ) {
            out_roots.push(node.with_expanded(true));
        }
    }

    WorkspaceTree {
        roots: out_roots,
        id_to_path,
        id_to_kind,
    }
}

/// Children of a single directory loaded one level deep, used for on-demand
/// (lazy) expansion. Sub-directories are left collapsed and unloaded.
#[derive(Debug, Clone)]
pub struct DirChildren {
    pub children: Vec<FileTreeNode>,
    pub id_to_path: HashMap<FileTreeNodeId, PathBuf>,
    pub id_to_kind: HashMap<FileTreeNodeId, FileTreeNodeKind>,
}

/// Builds a workspace tree lazily: only directories that are roots or whose
/// canonical path is in `expanded` have their children read from disk. Every
/// other directory is returned collapsed and unloaded (`children_loaded = false`)
/// so the caller can load it on demand.
pub fn build_workspace_tree_lazy(
    roots: &[PathBuf],
    options: WorkspaceTreeOptions,
    git_statuses: &WorkspaceGitStatuses,
    expanded: &HashSet<PathBuf>,
) -> WorkspaceTree {
    let mut id_to_path: HashMap<FileTreeNodeId, PathBuf> = HashMap::new();
    let mut id_to_kind: HashMap<FileTreeNodeId, FileTreeNodeKind> = HashMap::new();

    let roots = roots
        .iter()
        .filter_map(|p| canonicalize_best_effort(p))
        .collect::<Vec<_>>();

    let mut out_roots = Vec::new();
    for root in roots {
        if let Some(node) = build_node_lazy(
            &root,
            0,
            &options,
            git_statuses,
            expanded,
            &mut id_to_path,
            &mut id_to_kind,
        ) {
            out_roots.push(node);
        }
    }

    WorkspaceTree {
        roots: out_roots,
        id_to_path,
        id_to_kind,
    }
}

/// Reads a single directory one level deep for lazy expansion. Sub-directories
/// are marked collapsed and unloaded; files are normal leaves.
pub fn load_dir_children(
    dir: &Path,
    options: WorkspaceTreeOptions,
    git_statuses: &WorkspaceGitStatuses,
) -> DirChildren {
    let mut id_to_path: HashMap<FileTreeNodeId, PathBuf> = HashMap::new();
    let mut id_to_kind: HashMap<FileTreeNodeId, FileTreeNodeKind> = HashMap::new();

    let mut children = read_dir_sorted(dir, &options)
        .into_iter()
        .filter_map(|child| build_lazy_leaf(&child, git_statuses, &mut id_to_path, &mut id_to_kind))
        .take(options.max_entries_per_dir)
        .collect::<Vec<_>>();

    sort_nodes_dirs_first(&mut children);

    DirChildren {
        children,
        id_to_path,
        id_to_kind,
    }
}

pub fn build_workspace_file_index(roots: &[PathBuf], max_entries: usize) -> WorkspaceFileIndex {
    let max_entries = max_entries.max(1);
    let roots = roots
        .iter()
        .filter_map(|p| canonicalize_best_effort(p))
        .collect::<Vec<_>>();
    let tree = build_workspace_tree(
        &roots,
        WorkspaceTreeOptions {
            max_entries_per_dir: max_entries,
            ..WorkspaceTreeOptions::default()
        },
    );

    let mut entries = Vec::new();
    for root in &tree.roots {
        collect_file_entries(root, &tree.id_to_path, &roots, &mut entries, max_entries);
        if entries.len() >= max_entries {
            break;
        }
    }

    WorkspaceFileIndex { roots, entries }
}

pub fn git_statuses_for_root(root: &Path) -> Result<WorkspaceGitStatuses, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-z")
        .arg("--ignored=matching")
        .output()
        .map_err(|err| format!("git status failed for {}: {err}", root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "git status failed for {}: {}",
            root.display(),
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_git_status_porcelain_v1(root, &stdout))
}

pub fn parse_git_status_porcelain_v1(root: &Path, output: &str) -> WorkspaceGitStatuses {
    let root = canonicalize_best_effort(root).unwrap_or_else(|| root.to_path_buf());
    let mut statuses = WorkspaceGitStatuses::default();

    if output.as_bytes().contains(&0) {
        for (status, path) in parse_git_status_z_records(output.as_bytes()) {
            statuses.insert_path(root.join(path), status);
        }
    } else {
        for line in output.lines() {
            let Some((status, path)) = parse_git_status_line(line) else {
                continue;
            };
            statuses.insert_path(root.join(path), status);
        }
    }

    statuses
}

fn collect_file_entries(
    node: &FileTreeNode,
    id_to_path: &HashMap<FileTreeNodeId, PathBuf>,
    roots: &[PathBuf],
    entries: &mut Vec<WorkspaceFileEntry>,
    max_entries: usize,
) {
    if entries.len() >= max_entries {
        return;
    }

    if node.kind == FileTreeNodeKind::File {
        if let Some(path) = id_to_path.get(&node.id) {
            entries.push(WorkspaceFileEntry {
                path: path.clone(),
                display_path: display_path_for_file(path, roots),
            });
        }
        return;
    }

    for child in &node.children {
        collect_file_entries(child, id_to_path, roots, entries, max_entries);
        if entries.len() >= max_entries {
            break;
        }
    }
}

fn display_path_for_file(path: &Path, roots: &[PathBuf]) -> String {
    roots
        .iter()
        .filter_map(|root| path.strip_prefix(root).ok())
        .min_by_key(|relative| relative.components().count())
        .map(|relative| relative.to_string_lossy().to_string())
        .filter(|relative| !relative.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn parse_git_status_line(line: &str) -> Option<(FileTreeGitStatus, PathBuf)> {
    let bytes = line.as_bytes();
    if bytes.len() < 4 {
        return None;
    }

    let x = bytes[0] as char;
    let y = bytes[1] as char;
    let status = git_status_from_porcelain_xy(x, y)?;
    let raw_path = line.get(3..)?.trim_end();
    let path = raw_path
        .rsplit_once(" -> ")
        .map(|(_, new_path)| new_path)
        .unwrap_or(raw_path);
    Some((status, PathBuf::from(unquote_porcelain_path(path))))
}

fn parse_git_status_z_records(output: &[u8]) -> Vec<(FileTreeGitStatus, PathBuf)> {
    let records = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut parsed = Vec::new();
    let mut index = 0;

    while let Some(record) = records.get(index).copied() {
        index += 1;
        if record.is_empty() || record.len() < 4 {
            continue;
        }

        let x = record[0] as char;
        let y = record[1] as char;
        let Some(status) = git_status_from_porcelain_xy(x, y) else {
            continue;
        };
        let path = String::from_utf8_lossy(&record[3..]).into_owned();
        parsed.push((status, PathBuf::from(path)));
        if status == FileTreeGitStatus::Renamed {
            index += 1;
        }
    }

    parsed
}

fn git_status_from_porcelain_xy(x: char, y: char) -> Option<FileTreeGitStatus> {
    if x == '!' && y == '!' {
        return Some(FileTreeGitStatus::Ignored);
    }
    if x == '?' && y == '?' {
        return Some(FileTreeGitStatus::Untracked);
    }
    if x == 'R' || y == 'R' {
        return Some(FileTreeGitStatus::Renamed);
    }
    if x == 'A' || y == 'A' {
        return Some(FileTreeGitStatus::Added);
    }
    if x == 'D' || y == 'D' {
        return Some(FileTreeGitStatus::Deleted);
    }
    if matches!(x, 'M' | 'T') || matches!(y, 'M' | 'T') {
        return Some(FileTreeGitStatus::Modified);
    }
    None
}

fn unquote_porcelain_path(path: &str) -> String {
    let Some(inner) = path.strip_prefix('"').and_then(|p| p.strip_suffix('"')) else {
        return path.to_string();
    };

    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
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
    git_statuses: &WorkspaceGitStatuses,
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
    let git_status = git_statuses.status_for(id, path);

    if kind == FileTreeNodeKind::File {
        return Some(apply_git_status(FileTreeNode::file(id, name), git_status));
    }

    if depth >= options.max_depth {
        return Some(apply_git_status(
            FileTreeNode::dir(id, name, Vec::new()),
            git_status,
        ));
    }

    let mut children = read_dir_sorted(path, options)
        .into_iter()
        .filter_map(|child| {
            build_node(
                &child,
                depth + 1,
                options,
                git_statuses,
                id_to_path,
                id_to_kind,
            )
        })
        .take(options.max_entries_per_dir)
        .collect::<Vec<_>>();

    // Show directories first to match typical file explorers.
    sort_nodes_dirs_first(&mut children);

    Some(apply_git_status(
        FileTreeNode::dir(id, name, children),
        git_status,
    ))
}

fn build_node_lazy(
    path: &Path,
    depth: usize,
    options: &WorkspaceTreeOptions,
    git_statuses: &WorkspaceGitStatuses,
    expanded: &HashSet<PathBuf>,
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
    let git_status = git_statuses.status_for(id, path);

    if kind == FileTreeNodeKind::File {
        return Some(apply_git_status(FileTreeNode::file(id, name), git_status));
    }

    let is_expanded = depth == 0 || expanded.contains(path);
    let can_descend = depth < options.max_depth;
    let should_load = is_expanded && can_descend;

    if !should_load {
        // Collapsed (or depth-capped) directory: leave children unloaded so the
        // caller can fetch them on demand. At the depth cap we mark it loaded to
        // avoid advertising an expansion we cannot fulfil.
        let children_loaded = !can_descend;
        return Some(apply_git_status(
            FileTreeNode::dir(id, name, Vec::new())
                .with_expanded(false)
                .with_children_loaded(children_loaded),
            git_status,
        ));
    }

    let mut children = read_dir_sorted(path, options)
        .into_iter()
        .filter_map(|child| {
            build_node_lazy(
                &child,
                depth + 1,
                options,
                git_statuses,
                expanded,
                id_to_path,
                id_to_kind,
            )
        })
        .take(options.max_entries_per_dir)
        .collect::<Vec<_>>();

    sort_nodes_dirs_first(&mut children);

    Some(apply_git_status(
        FileTreeNode::dir(id, name, children)
            .with_expanded(true)
            .with_children_loaded(true),
        git_status,
    ))
}

fn build_lazy_leaf(
    path: &Path,
    git_statuses: &WorkspaceGitStatuses,
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
    let git_status = git_statuses.status_for(id, path);

    let node = match kind {
        FileTreeNodeKind::File => FileTreeNode::file(id, name),
        FileTreeNodeKind::Directory => FileTreeNode::dir(id, name, Vec::new())
            .with_expanded(false)
            .with_children_loaded(false),
    };
    Some(apply_git_status(node, git_status))
}

fn sort_nodes_dirs_first(nodes: &mut [FileTreeNode]) {
    nodes.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a
            .name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase()),
    });
}

fn apply_git_status(node: FileTreeNode, status: Option<FileTreeGitStatus>) -> FileTreeNode {
    match status {
        Some(status) => node.with_git_status(status),
        None => node,
    }
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
    fn build_workspace_tree_applies_git_statuses_by_path_and_id() {
        let root = TempDir::new("git_status");
        let modified = root.path.join("modified.rs");
        let added = root.path.join("added.rs");
        fs::write(&modified, "modified\n").unwrap();
        fs::write(&added, "added\n").unwrap();

        let mut statuses = WorkspaceGitStatuses::default();
        statuses.insert_path(modified.clone(), FileTreeGitStatus::Modified);
        let added_id = node_id_for_path(&fs::canonicalize(&added).unwrap_or(added.clone()));
        statuses.insert_id(added_id, FileTreeGitStatus::Added);

        let tree = build_workspace_tree_with_git_statuses(
            std::slice::from_ref(&root.path),
            WorkspaceTreeOptions::default(),
            &statuses,
        );
        let root_node = tree.roots.first().expect("root node");
        let modified_node = root_node
            .children
            .iter()
            .find(|node| node.name == "modified.rs")
            .expect("modified node");
        let added_node = root_node
            .children
            .iter()
            .find(|node| node.name == "added.rs")
            .expect("added node");

        assert_eq!(modified_node.git_status, Some(FileTreeGitStatus::Modified));
        assert_eq!(added_node.git_status, Some(FileTreeGitStatus::Added));
    }

    #[test]
    fn parse_git_status_porcelain_v1_maps_common_statuses() {
        let root = TempDir::new("git_status_parse");
        fs::write(root.path.join("modified.rs"), "modified\n").unwrap();
        fs::write(root.path.join("added.rs"), "added\n").unwrap();
        fs::write(root.path.join("untracked file.rs"), "untracked\n").unwrap();
        fs::write(root.path.join("ignored.log"), "ignored\n").unwrap();

        let statuses = parse_git_status_porcelain_v1(
            &root.path,
            " M modified.rs\nA  added.rs\n?? untracked file.rs\n!! ignored.log\n",
        );
        let tree = build_workspace_tree_with_git_statuses(
            std::slice::from_ref(&root.path),
            WorkspaceTreeOptions {
                show_hidden: true,
                ..WorkspaceTreeOptions::default()
            },
            &statuses,
        );
        let root_node = tree.roots.first().expect("root node");

        let status_for = |name: &str| {
            root_node
                .children
                .iter()
                .find(|node| node.name == name)
                .and_then(|node| node.git_status)
        };

        assert_eq!(status_for("modified.rs"), Some(FileTreeGitStatus::Modified));
        assert_eq!(status_for("added.rs"), Some(FileTreeGitStatus::Added));
        assert_eq!(
            status_for("untracked file.rs"),
            Some(FileTreeGitStatus::Untracked)
        );
        assert_eq!(status_for("ignored.log"), Some(FileTreeGitStatus::Ignored));
    }

    #[test]
    fn parse_git_status_porcelain_v1_z_maps_renamed_paths_verbatim() {
        let root = TempDir::new("git_status_parse_renamed");
        let renamed = "renamed -> file with spaces.rs";
        fs::write(root.path.join(renamed), "renamed\n").unwrap();

        let statuses = parse_git_status_porcelain_v1(
            &root.path,
            "R  renamed -> file with spaces.rs\0old name.rs\0",
        );
        let tree = build_workspace_tree_with_git_statuses(
            std::slice::from_ref(&root.path),
            WorkspaceTreeOptions::default(),
            &statuses,
        );
        let root_node = tree.roots.first().expect("root node");
        let renamed_node = root_node
            .children
            .iter()
            .find(|node| node.name == renamed)
            .expect("renamed node");

        assert_eq!(renamed_node.git_status, Some(FileTreeGitStatus::Renamed));
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

    #[test]
    fn build_workspace_file_index_flattens_visible_files_only() {
        let root = TempDir::new("file_index");
        fs::create_dir_all(root.path.join("src")).unwrap();
        fs::create_dir_all(root.path.join(".git")).unwrap();
        fs::write(root.path.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.path.join("README.md"), "# readme\n").unwrap();
        fs::write(root.path.join(".hidden"), "secret\n").unwrap();
        fs::write(root.path.join(".git").join("config"), "ignore\n").unwrap();

        let index = build_workspace_file_index(std::slice::from_ref(&root.path), 100);
        let display_paths = index
            .entries
            .iter()
            .map(|entry| entry.display_path.as_str())
            .collect::<Vec<_>>();

        assert!(display_paths.contains(&"src/main.rs"));
        assert!(display_paths.contains(&"README.md"));
        assert!(!display_paths.iter().any(|path| path.contains(".git")));
        assert!(!display_paths.iter().any(|path| path.contains(".hidden")));
    }

    #[test]
    fn build_workspace_tree_lazy_loads_only_root_children() {
        let root = TempDir::new("lazy_root");
        fs::create_dir_all(root.path.join("src").join("deep")).unwrap();
        fs::write(root.path.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.path.join("README.md"), "# readme\n").unwrap();

        let tree = build_workspace_tree_lazy(
            std::slice::from_ref(&root.path),
            WorkspaceTreeOptions::default(),
            &WorkspaceGitStatuses::default(),
            &HashSet::new(),
        );
        let root_node = tree.roots.first().expect("root node");
        assert!(root_node.is_expanded);
        assert!(root_node.children_loaded);

        let src = root_node
            .children
            .iter()
            .find(|node| node.name == "src")
            .expect("src dir");
        // The sub-directory is present but its children are not yet loaded.
        assert!(!src.children_loaded);
        assert!(src.children.is_empty());
        assert!(!src.is_expanded);
    }

    #[test]
    fn build_workspace_tree_lazy_loads_expanded_subdirectory() {
        let root = TempDir::new("lazy_expanded");
        let src = root.path.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("main.rs"), "fn main() {}\n").unwrap();

        let canonical_src = fs::canonicalize(&src).unwrap_or(src);
        let expanded = HashSet::from([canonical_src]);
        let tree = build_workspace_tree_lazy(
            std::slice::from_ref(&root.path),
            WorkspaceTreeOptions::default(),
            &WorkspaceGitStatuses::default(),
            &expanded,
        );
        let root_node = tree.roots.first().expect("root node");
        let src_node = root_node
            .children
            .iter()
            .find(|node| node.name == "src")
            .expect("src dir");

        assert!(src_node.children_loaded);
        assert!(src_node.is_expanded);
        assert!(src_node.children.iter().any(|node| node.name == "main.rs"));
    }

    #[test]
    fn load_dir_children_reads_one_level_with_collapsed_subdirs() {
        let root = TempDir::new("load_children");
        fs::create_dir_all(root.path.join("nested").join("deep")).unwrap();
        fs::write(root.path.join("nested").join("file.rs"), "x\n").unwrap();

        let nested = root.path.join("nested");
        let loaded = load_dir_children(
            &nested,
            WorkspaceTreeOptions::default(),
            &WorkspaceGitStatuses::default(),
        );

        let names = loaded
            .children
            .iter()
            .map(|node| node.name.clone())
            .collect::<Vec<_>>();
        assert!(names.contains(&"deep".to_string()));
        assert!(names.contains(&"file.rs".to_string()));

        let deep = loaded
            .children
            .iter()
            .find(|node| node.name == "deep")
            .expect("deep dir");
        assert!(!deep.children_loaded);
        assert!(deep.children.is_empty());
    }

    #[test]
    fn build_workspace_file_index_respects_global_entry_limit() {
        let root = TempDir::new("file_index_limit");
        fs::write(root.path.join("a.txt"), "a\n").unwrap();
        fs::write(root.path.join("b.txt"), "b\n").unwrap();

        let index = build_workspace_file_index(std::slice::from_ref(&root.path), 1);

        assert_eq!(index.entries.len(), 1);
    }
}
