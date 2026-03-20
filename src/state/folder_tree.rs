use crate::state::mail::{Folder, FolderKind};

/// A tree node for sidebar display. Each node may represent a real mailbox
/// (with a `folder_index` pointing into the flat folder list) or a virtual
/// grouping node that only exists to hold children.
#[derive(Debug, Clone)]
pub struct FolderNode {
    /// Display name — just the last path segment (e.g. "Projects" not "Work/Projects").
    pub display_name: String,
    /// Full mailbox name for the IMAP server (e.g. "Work/Projects").
    pub full_name: String,
    /// Index into the flat `MailboxState::folders` vec, if this is a real mailbox.
    pub folder_index: Option<usize>,
    pub kind: FolderKind,
    pub unread_count: usize,
    pub children: Vec<FolderNode>,
}

impl FolderNode {}

/// Build a tree from a flat list of folders. Folders whose names contain the
/// delimiter are nested under parent nodes. Parent nodes that don't correspond
/// to a real mailbox are created as virtual grouping nodes.
///
/// Display names are formatted: known folder kinds get canonical names
/// (e.g. "INBOX" → "Inbox"), custom folders are title-cased.
pub fn build_folder_tree(folders: &[Folder]) -> Vec<FolderNode> {
    let mut roots: Vec<FolderNode> = Vec::new();

    for (idx, folder) in folders.iter().enumerate() {
        let parts: Vec<&str> = match &folder.delimiter {
            Some(delim) if !delim.is_empty() => folder.name.split(delim).collect(),
            _ => vec![&folder.name],
        };

        insert_into_tree(&mut roots, &parts, &folder.name, idx, folder);
    }

    format_display_names(&mut roots);
    roots
}

/// Recursively apply human-friendly display names to known folder kinds.
/// Custom folders keep their original names as-is.
fn format_display_names(nodes: &mut [FolderNode]) {
    for node in nodes.iter_mut() {
        if let Some(name) = node.kind.display_name() {
            node.display_name = name.to_string();
        }
        format_display_names(&mut node.children);
    }
}

fn insert_into_tree(
    nodes: &mut Vec<FolderNode>,
    parts: &[&str],
    full_name: &str,
    folder_index: usize,
    folder: &Folder,
) {
    let Some((&first, rest)) = parts.split_first() else {
        return;
    };

    // Find or create the node for this path segment
    let pos = nodes.iter().position(|n| n.display_name == first);

    if rest.is_empty() {
        // This is the leaf — the actual mailbox
        if let Some(pos) = pos {
            // A virtual grouping node already exists for this name, promote it
            let node = &mut nodes[pos];
            node.folder_index = Some(folder_index);
            node.kind = folder.kind.clone();
            node.unread_count = folder.unread_count;
            node.full_name = full_name.to_string();
        } else {
            nodes.push(FolderNode {
                display_name: first.to_string(),
                full_name: full_name.to_string(),
                folder_index: Some(folder_index),
                kind: folder.kind.clone(),
                unread_count: folder.unread_count,
                children: Vec::new(),
            });
        }
    } else {
        // Intermediate segment — find or create a grouping node
        let node = if let Some(pos) = pos {
            &mut nodes[pos]
        } else {
            nodes.push(FolderNode {
                display_name: first.to_string(),
                full_name: String::new(),
                folder_index: None,
                kind: FolderKind::Custom(first.to_string()),
                unread_count: 0,
                children: Vec::new(),
            });
            nodes.last_mut().unwrap()
        };

        insert_into_tree(&mut node.children, rest, full_name, folder_index, folder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, delimiter: &str) -> Folder {
        Folder {
            kind: FolderKind::Custom(name.to_string()),
            name: name.to_string(),
            delimiter: Some(delimiter.to_string()),
            unread_count: 0,
        }
    }

    #[test]
    fn flat_folders_stay_flat() {
        let folders = vec![
            Folder {
                kind: FolderKind::Inbox,
                name: "INBOX".into(),
                delimiter: Some("/".into()),
                unread_count: 3,
            },
            folder("Sent", "/"),
        ];

        let tree = build_folder_tree(&folders);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].display_name, "Inbox");
        assert_eq!(tree[0].folder_index, Some(0));
        assert!(tree[0].children.is_empty());
    }

    #[test]
    fn nested_folders_become_children() {
        let folders = vec![
            folder("Work", "/"),
            folder("Work/Projects", "/"),
            folder("Work/Meetings", "/"),
        ];

        let tree = build_folder_tree(&folders);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].display_name, "Work");
        assert_eq!(tree[0].folder_index, Some(0));
        assert_eq!(tree[0].children.len(), 2);
        assert_eq!(tree[0].children[0].display_name, "Projects");
        assert_eq!(tree[0].children[0].folder_index, Some(1));
        assert_eq!(tree[0].children[1].display_name, "Meetings");
        assert_eq!(tree[0].children[1].folder_index, Some(2));
    }

    #[test]
    fn virtual_parent_created_when_no_matching_folder() {
        let folders = vec![folder("Work/Projects", "/"), folder("Work/Meetings", "/")];

        let tree = build_folder_tree(&folders);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].display_name, "Work");
        // Virtual parent has no folder_index
        assert_eq!(tree[0].folder_index, None);
        assert_eq!(tree[0].children.len(), 2);
    }

    #[test]
    fn deeply_nested_folders() {
        let folders = vec![folder("A/B/C", "/"), folder("A/B/D", "/")];

        let tree = build_folder_tree(&folders);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].display_name, "A");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].display_name, "B");
        assert_eq!(tree[0].children[0].children.len(), 2);
        assert_eq!(tree[0].children[0].children[0].display_name, "C");
    }

    #[test]
    fn dot_delimiter() {
        let folders = vec![folder("INBOX.Subfolder", "."), folder("INBOX.Other", ".")];

        let tree = build_folder_tree(&folders);
        assert_eq!(tree.len(), 1);
        // Virtual parent keeps the raw segment name (no real folder to derive kind from)
        assert_eq!(tree[0].display_name, "INBOX");
        assert_eq!(tree[0].children.len(), 2);
    }

    #[test]
    fn display_names_are_formatted() {
        let folders = vec![
            Folder {
                kind: FolderKind::Inbox,
                name: "INBOX".into(),
                delimiter: Some("/".into()),
                unread_count: 0,
            },
            Folder {
                kind: FolderKind::Sent,
                name: "SENT".into(),
                delimiter: Some("/".into()),
                unread_count: 0,
            },
            folder("MAILBOX", "/"),
            folder("my folder", "/"),
        ];

        let tree = build_folder_tree(&folders);
        assert_eq!(tree[0].display_name, "Inbox");
        assert_eq!(tree[1].display_name, "Sent");
        // Custom folders keep their original names
        assert_eq!(tree[2].display_name, "MAILBOX");
        assert_eq!(tree[3].display_name, "my folder");
    }
}
