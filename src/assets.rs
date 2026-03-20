use gpui::{AssetSource, SharedString};
use gpui_component::IconNamed;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "./assets"]
#[include = "icons/**/*.svg"]
#[include = "themes/*.json"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        Ok(Self::get(path).map(|f| f.data))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| p.starts_with(path).then(|| p.into()))
            .collect())
    }
}

#[allow(dead_code)]
#[derive(strum::Display, Debug, Clone)]
#[strum(serialize_all = "kebab-case")]
pub enum IconName {
    Archive,
    ArchiveRestore,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    AtSign,
    Bell,
    BellOff,
    Bold,
    Calendar,
    CircleUser,
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ChevronsUpDown,
    CircleAlert,
    CircleCheck,
    CircleX,
    Clock,
    Download,
    Ellipsis,
    EllipsisVertical,
    Eye,
    EyeOff,
    File,
    FileText,
    Flag,
    Folder,
    FolderOpen,
    FolderPlus,
    Forward,
    Funnel,
    Image,
    Inbox,
    Italic,
    Link,
    List,
    ListOrdered,
    Lock,
    LockOpen,
    LogOut,
    Mail,
    MailCheck,
    MailOpen,
    MailPlus,
    MailWarning,
    MailX,
    Minus,
    Paperclip,
    Pencil,
    Plus,
    RefreshCw,
    Reply,
    ReplyAll,
    Search,
    Send,
    Server,
    Settings,
    Star,
    StarOff,
    Tag,
    Tags,
    #[strum(serialize = "trash-2")]
    Trash2,
    Underline,
    User,
    UserPlus,
    Users,
    X,
}

impl IconNamed for IconName {
    fn path(self) -> SharedString {
        SharedString::new(format!("icons/{}.svg", &self.to_string()))
    }
}
