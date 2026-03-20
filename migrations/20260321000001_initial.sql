CREATE TABLE IF NOT EXISTS mailboxes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id TEXT NOT NULL,
    name TEXT NOT NULL,
    delimiter TEXT,
    folder_kind TEXT NOT NULL DEFAULT 'custom',
    UNIQUE(account_id, name)
);

CREATE INDEX idx_mailboxes_account ON mailboxes(account_id);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id TEXT NOT NULL,
    mailbox_name TEXT NOT NULL,
    uid INTEGER NOT NULL,
    subject TEXT NOT NULL DEFAULT '',
    from_name TEXT NOT NULL DEFAULT '',
    from_email TEXT NOT NULL DEFAULT '',
    to_addresses TEXT NOT NULL DEFAULT '[]',
    date TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    is_read INTEGER NOT NULL DEFAULT 0,
    is_starred INTEGER NOT NULL DEFAULT 0,
    fetched_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(account_id, mailbox_name, uid)
);

CREATE INDEX idx_messages_account_mailbox ON messages(account_id, mailbox_name);
CREATE INDEX idx_messages_uid ON messages(account_id, mailbox_name, uid);
