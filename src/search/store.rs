use std::collections::HashSet;

use sqlx::{Row, SqlitePool};

use crate::db::repo::messages::DbMessage;
use crate::error::Result;

/// A message with its embedding vector, for in-memory similarity search.
#[derive(Clone)]
pub struct EmbeddedMessage {
    pub message_id: i64,
    pub embedding: Vec<f32>,
}

/// Result of a similarity search.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub message_id: i64,
    pub score: f32,
}

/// A full search result with message data included.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub message_id: i64,
    pub score: f32,
    pub account_id: String,
    pub mailbox_name: String,
    pub uid: i64,
    pub subject: String,
    pub from_name: String,
    pub from_email: String,
    pub to_addresses: String,
    pub date: String,
    pub body: String,
    pub is_read: bool,
    pub is_starred: bool,
}

impl SearchResult {
    /// Convert a `DbMessage` into a `SearchResult` with the given relevance score.
    pub fn from_db_message(msg: DbMessage, score: f32) -> Self {
        Self {
            message_id: msg.id,
            score,
            account_id: msg.account_id,
            mailbox_name: msg.mailbox_name,
            uid: msg.uid,
            subject: msg.subject,
            from_name: msg.from_name,
            from_email: msg.from_email,
            to_addresses: msg.to_addresses,
            date: msg.date,
            body: msg.body,
            is_read: msg.is_read,
            is_starred: msg.is_starred,
        }
    }
}

/// Store and query embeddings in SQLite.
pub struct EmbeddingStore;

impl EmbeddingStore {
    /// Get message IDs that don't have embeddings for the given model.
    pub async fn missing_message_ids(pool: &SqlitePool, model_name: &str) -> Result<Vec<i64>> {
        let rows = sqlx::query(
            r#"SELECT m.id
               FROM messages m
               LEFT JOIN embeddings e ON m.id = e.message_id AND e.model_name = ?
               WHERE e.id IS NULL"#,
        )
        .bind(model_name)
        .fetch_all(pool)
        .await?;
        Ok(rows.iter().map(|r| r.get::<i64, _>("id")).collect())
    }

    /// Fetch subject and body for given message IDs (for embedding).
    /// Prepends the model's passage prefix (e.g. "passage: " for E5 models).
    pub async fn fetch_texts(
        pool: &SqlitePool,
        ids: &[i64],
        model_name: &str,
    ) -> Result<Vec<(i64, String)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let prefix = crate::search::embedder::passage_prefix(model_name);
        let placeholders: Vec<&str> = ids.iter().map(|_| "?").collect();
        let sql = format!(
            "SELECT id, subject, body FROM messages WHERE id IN ({})",
            placeholders.join(", ")
        );
        let mut query = sqlx::query(&sql);
        for id in ids {
            query = query.bind(id);
        }
        let rows = query.fetch_all(pool).await?;
        Ok(rows
            .iter()
            .map(|r| {
                let id: i64 = r.get("id");
                let subject: String = r.get("subject");
                let body: String = r.get("body");
                // Truncate to ~1500 chars (~512 tokens) for E5's context window
                let mut text = format!("{prefix}{subject}\n{body}");
                if text.len() > 1500 {
                    let boundary = text.floor_char_boundary(1500);
                    text.truncate(boundary);
                }
                (id, text)
            })
            .collect())
    }

    /// Insert an embedding for a message.
    pub async fn insert(
        pool: &SqlitePool,
        message_id: i64,
        model_name: &str,
        embedding: &[f32],
    ) -> Result<()> {
        let blob = floats_to_bytes(embedding);
        sqlx::query(
            r#"INSERT INTO embeddings (message_id, model_name, embedding)
               VALUES (?, ?, ?)
               ON CONFLICT(message_id, model_name) DO UPDATE
               SET embedding = excluded.embedding,
                   embedded_at = datetime('now')"#,
        )
        .bind(message_id)
        .bind(model_name)
        .bind(blob)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Load all embeddings for a given model into memory.
    pub async fn load_all(pool: &SqlitePool, model_name: &str) -> Result<Vec<EmbeddedMessage>> {
        let rows = sqlx::query("SELECT message_id, embedding FROM embeddings WHERE model_name = ?")
            .bind(model_name)
            .fetch_all(pool)
            .await?;

        Ok(rows
            .iter()
            .map(|r| {
                let message_id: i64 = r.get("message_id");
                let blob: Vec<u8> = r.get("embedding");
                EmbeddedMessage {
                    message_id,
                    embedding: bytes_to_floats(&blob),
                }
            })
            .collect())
    }

    /// Delete embeddings for a specific model.
    pub async fn delete_by_model(pool: &SqlitePool, model_name: &str) -> Result<()> {
        sqlx::query("DELETE FROM embeddings WHERE model_name = ?")
            .bind(model_name)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Search for similar messages using cosine similarity.
    /// Minimum cosine similarity to include in results.
    const MIN_SCORE: f32 = 0.4;

    pub fn search(
        query_embedding: &[f32],
        all_embeddings: &[EmbeddedMessage],
        limit: usize,
    ) -> Vec<SearchHit> {
        Self::search_filtered(query_embedding, all_embeddings, None, limit)
    }

    /// Search with an optional filter set of candidate message IDs.
    /// When `candidate_ids` is Some, only embeddings for those messages are considered.
    pub fn search_filtered(
        query_embedding: &[f32],
        all_embeddings: &[EmbeddedMessage],
        candidate_ids: Option<&HashSet<i64>>,
        limit: usize,
    ) -> Vec<SearchHit> {
        let iter = all_embeddings.iter().filter(|em| {
            candidate_ids
                .map(|ids| ids.contains(&em.message_id))
                .unwrap_or(true)
        });

        let mut scored: Vec<SearchHit> = iter
            .map(|em| SearchHit {
                message_id: em.message_id,
                score: cosine_similarity(query_embedding, &em.embedding),
            })
            .filter(|hit| hit.score >= Self::MIN_SCORE)
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        scored
    }

    /// Fetch full search results by joining hits with message data.
    pub async fn hydrate_hits(pool: &SqlitePool, hits: &[SearchHit]) -> Result<Vec<SearchResult>> {
        if hits.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<&str> = hits.iter().map(|_| "?").collect();
        let sql = format!(
            r#"SELECT id, account_id, mailbox_name, uid, subject, from_name, from_email,
                      to_addresses, date, body, is_read, is_starred
               FROM messages WHERE id IN ({})"#,
            placeholders.join(", ")
        );
        let mut query = sqlx::query(&sql);
        for hit in hits {
            query = query.bind(hit.message_id);
        }
        let rows = query.fetch_all(pool).await?;

        let score_map: std::collections::HashMap<i64, f32> =
            hits.iter().map(|h| (h.message_id, h.score)).collect();

        let mut results: Vec<SearchResult> = rows
            .iter()
            .map(|r| {
                let id: i64 = r.get("id");
                SearchResult {
                    message_id: id,
                    score: score_map.get(&id).copied().unwrap_or(0.0),
                    account_id: r.get("account_id"),
                    mailbox_name: r.get("mailbox_name"),
                    uid: r.get("uid"),
                    subject: r.get("subject"),
                    from_name: r.get("from_name"),
                    from_email: r.get("from_email"),
                    to_addresses: r.get("to_addresses"),
                    date: r.get("date"),
                    body: r.get("body"),
                    is_read: r.get("is_read"),
                    is_starred: r.get("is_starred"),
                }
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(results)
    }
}

fn floats_to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn bytes_to_floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}
