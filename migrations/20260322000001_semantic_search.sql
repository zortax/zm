CREATE TABLE IF NOT EXISTS embeddings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id INTEGER NOT NULL,
    model_name TEXT NOT NULL,
    embedding BLOB NOT NULL,
    embedded_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(message_id, model_name),
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);

CREATE INDEX idx_embeddings_model ON embeddings(model_name);
CREATE INDEX idx_embeddings_message ON embeddings(message_id);
