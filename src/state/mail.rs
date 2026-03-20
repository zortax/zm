use crate::assets::IconName;

#[derive(Debug, Clone)]
pub struct MailMessage {
    pub id: i64,
    pub account_id: String,
    pub mailbox_name: String,
    pub uid: i64,
    pub from_name: String,
    pub from_email: String,
    pub to: Vec<String>,
    pub subject: String,
    pub date: String,
    pub body: String,
    pub is_read: bool,
    pub is_starred: bool,
}

impl MailMessage {
    /// Returns the first line of the body as a preview.
    pub fn preview(&self) -> &str {
        self.body.lines().next().unwrap_or("")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderKind {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Archive,
    Custom(String),
}

impl FolderKind {
    pub fn icon(&self) -> IconName {
        match self {
            FolderKind::Inbox => IconName::Inbox,
            FolderKind::Sent => IconName::Send,
            FolderKind::Drafts => IconName::Pencil,
            FolderKind::Trash => IconName::Trash2,
            FolderKind::Archive => IconName::Archive,
            FolderKind::Custom(_) => IconName::Folder,
        }
    }

    /// Human-friendly display name for known folder kinds.
    pub fn display_name(&self) -> Option<&'static str> {
        match self {
            FolderKind::Inbox => Some("Inbox"),
            FolderKind::Sent => Some("Sent"),
            FolderKind::Drafts => Some("Drafts"),
            FolderKind::Trash => Some("Trash"),
            FolderKind::Archive => Some("Archive"),
            FolderKind::Custom(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Folder {
    pub kind: FolderKind,
    pub name: String,
    pub delimiter: Option<String>,
    pub unread_count: usize,
}

impl Folder {
    /// Split the folder name into breadcrumb segments using the delimiter.
    /// Known folder kinds get their canonical display name for the relevant segment.
    pub fn breadcrumb_segments(&self) -> Vec<String> {
        let segments: Vec<&str> = match &self.delimiter {
            Some(delim) if !delim.is_empty() => self.name.split(delim).collect(),
            _ => vec![&self.name],
        };

        segments
            .into_iter()
            .map(|seg| {
                // Check if this segment matches a known folder kind
                let kind = crate::db::repo::mailboxes::folder_kind_from_name(seg);
                match kind {
                    "inbox" => "Inbox",
                    "sent" => "Sent",
                    "drafts" => "Drafts",
                    "trash" => "Trash",
                    "archive" => "Archive",
                    _ => seg,
                }
                .to_string()
            })
            .collect()
    }
}
