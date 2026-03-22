use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use fastembed::TextEmbedding;
use gpui::*;
use gpui_tokio::Tokio;
use sqlx::SqlitePool;

use crate::db::repo::messages;
use crate::search::embedder;
use crate::search::query::ParsedQuery;
use crate::search::store::{EmbeddedMessage, EmbeddingStore, SearchResult};

const EMBED_BATCH_SIZE: usize = 64;
const SEARCH_LIMIT: usize = 50;

/// Status of the search/embedding engine.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchEngineStatus {
    Disabled,
    DownloadingModel,
    Indexing { done: usize, total: usize },
    Ready,
    Searching,
    Failed { error: String },
}

impl SearchEngineStatus {
    pub fn display(&self) -> Option<String> {
        match self {
            Self::Disabled => None,
            Self::DownloadingModel => Some("Downloading model...".into()),
            Self::Indexing { done, total } => Some(format!("Indexing {done}/{total}")),
            Self::Ready => None,
            Self::Searching => Some("Searching...".into()),
            Self::Failed { error } => Some(format!("Search error: {error}")),
        }
    }

    pub fn progress(&self) -> Option<f32> {
        match self {
            Self::DownloadingModel => Some(0.0),
            Self::Indexing { done, total } => {
                if *total == 0 {
                    Some(1.0)
                } else {
                    Some(*done as f32 / *total as f32)
                }
            }
            _ => None,
        }
    }
}

pub struct SearchEngineEvent;
impl EventEmitter<SearchEngineEvent> for SearchEngine {}

type SharedModel = Arc<Mutex<TextEmbedding>>;

pub struct SearchEngine {
    pool: SqlitePool,
    model_name: String,
    model: Option<SharedModel>,
    /// Cached embeddings for in-memory similarity search.
    cached_embeddings: Vec<EmbeddedMessage>,
    status: SearchEngineStatus,
    _task: Option<Task<()>>,
}

impl SearchEngine {
    pub fn new(
        pool: SqlitePool,
        model_name: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut engine = Self {
            pool,
            model_name,
            model: None,
            cached_embeddings: Vec::new(),
            status: if enabled {
                SearchEngineStatus::DownloadingModel
            } else {
                SearchEngineStatus::Disabled
            },
            _task: None,
        };

        if enabled {
            engine.start_init(cx);
        }

        engine
    }

    pub fn status(&self) -> &SearchEngineStatus {
        &self.status
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.status, SearchEngineStatus::Ready)
    }

    pub fn is_enabled(&self) -> bool {
        !matches!(self.status, SearchEngineStatus::Disabled)
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Initialize model and index all unembedded messages.
    fn start_init(&mut self, cx: &mut Context<Self>) {
        let model_name = self.model_name.clone();
        let pool = self.pool.clone();

        let (tx, rx) = flume::unbounded::<EmbedProgress>();

        Tokio::spawn(cx, {
            let model_name = model_name.clone();
            async move { run_init(&model_name, &pool, tx).await }
        })
        .detach();

        self._task = Some(cx.spawn(async move |this, cx| {
            while let Ok(progress) = rx.recv_async().await {
                let should_break = this
                    .update(cx, |this, cx| match progress {
                        EmbedProgress::ModelReady { model } => {
                            this.model = Some(model);
                            this.set_status(SearchEngineStatus::Indexing { done: 0, total: 0 }, cx);
                        }
                        EmbedProgress::Indexing { done, total } => {
                            this.set_status(SearchEngineStatus::Indexing { done, total }, cx);
                        }
                        EmbedProgress::CacheLoaded { embeddings } => {
                            this.cached_embeddings = embeddings;
                        }
                        EmbedProgress::Complete => {
                            this.set_status(SearchEngineStatus::Ready, cx);
                        }
                        EmbedProgress::Failed { error } => {
                            tracing::error!(error, "search engine init failed");
                            this.set_status(SearchEngineStatus::Failed { error }, cx);
                        }
                    })
                    .is_err();
                if should_break {
                    break;
                }
            }
        }));
    }

    /// Embed any messages that were added since last indexing.
    pub fn embed_new_messages(&mut self, cx: &mut Context<Self>) {
        if !self.is_enabled() || self.model.is_none() {
            return;
        }
        if matches!(
            self.status,
            SearchEngineStatus::Indexing { .. } | SearchEngineStatus::DownloadingModel
        ) {
            return;
        }

        let model = self.model.clone().unwrap();
        let pool = self.pool.clone();
        let model_name = self.model_name.clone();

        let (tx, rx) = flume::unbounded::<EmbedProgress>();

        Tokio::spawn(cx, async move {
            run_incremental_embed(&model_name, &model, &pool, tx).await
        })
        .detach();

        self._task = Some(cx.spawn(async move |this, cx| {
            while let Ok(progress) = rx.recv_async().await {
                let should_break = this
                    .update(cx, |this, cx| match progress {
                        EmbedProgress::Indexing { done, total } => {
                            if total > 0 {
                                this.set_status(SearchEngineStatus::Indexing { done, total }, cx);
                            }
                        }
                        EmbedProgress::CacheLoaded { embeddings } => {
                            this.cached_embeddings = embeddings;
                        }
                        EmbedProgress::Complete => {
                            this.set_status(SearchEngineStatus::Ready, cx);
                        }
                        EmbedProgress::Failed { error } => {
                            tracing::error!(error, "incremental embedding failed");
                            this.set_status(SearchEngineStatus::Ready, cx);
                        }
                        _ => {}
                    })
                    .is_err();
                if should_break {
                    break;
                }
            }
        }));
    }

    /// Perform semantic search. Returns results asynchronously.
    pub fn search(&mut self, query: String, cx: &mut Context<Self>) -> Task<Vec<SearchResult>> {
        if !self.is_ready() || self.model.is_none() {
            return Task::ready(Vec::new());
        }

        self.set_status(SearchEngineStatus::Searching, cx);

        let model = self.model.clone().unwrap();
        let pool = self.pool.clone();
        let cached = self.cached_embeddings.clone();
        let model_name = self.model_name.clone();

        cx.spawn(async move |this, cx| {
            let results = Tokio::spawn(cx, async move {
                // Embed the query (needs mutable access via Mutex)
                let query_embedding = {
                    let model = model.clone();
                    let query = query.clone();
                    let model_name = model_name.clone();
                    tokio::task::spawn_blocking(move || {
                        let mut model = model.lock().unwrap();
                        embedder::embed_query(&mut model, &query, &model_name)
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?
                }?;

                // Search against cached embeddings
                let hits = EmbeddingStore::search(&query_embedding, &cached, SEARCH_LIMIT);

                // Hydrate with message data
                EmbeddingStore::hydrate_hits(&pool, &hits).await
            })
            .await;

            let results = match results {
                Ok(Ok(results)) => results,
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "search failed");
                    Vec::new()
                }
                Err(e) => {
                    tracing::error!(error = %e, "search task failed");
                    Vec::new()
                }
            };

            let _ = this.update(cx, |this, cx| {
                this.set_status(SearchEngineStatus::Ready, cx);
            });

            results
        })
    }

    /// Perform hybrid search: uses keyword filtering for exact phrases/modifiers,
    /// then semantic search on the filtered candidates for free text.
    pub fn hybrid_search(
        &mut self,
        parsed: ParsedQuery,
        cx: &mut Context<Self>,
    ) -> Task<Vec<SearchResult>> {
        if !self.is_ready() || self.model.is_none() {
            return Task::ready(Vec::new());
        }

        let has_free_text = !parsed.free_text.is_empty();
        let has_keyword_constraints = parsed.has_keyword_terms() || parsed.has_modifiers();

        // Pure semantic search (no modifiers, no exact phrases)
        if has_free_text && !has_keyword_constraints {
            return self.search(parsed.free_text.clone(), cx);
        }

        self.set_status(SearchEngineStatus::Searching, cx);

        let model = self.model.clone().unwrap();
        let pool = self.pool.clone();
        let cached = self.cached_embeddings.clone();
        let model_name = self.model_name.clone();

        cx.spawn(async move |this, cx| {
            let results = Tokio::spawn(cx, async move {
                // Keyword-only path: no free text for semantic search
                if !has_free_text {
                    let keywords = parsed.keyword_terms(false);
                    let db_results =
                        messages::keyword_search(&pool, &keywords, &parsed.modifiers, SEARCH_LIMIT)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                    let results: Vec<SearchResult> = db_results
                        .into_iter()
                        .map(|msg| SearchResult::from_db_message(msg, 1.0))
                        .collect();
                    return Ok(results);
                }

                // Hybrid path: keyword filter first, then semantic on candidates
                let keywords = parsed.keyword_terms(false); // only exact phrases
                let candidate_ids =
                    messages::keyword_search_ids(&pool, &keywords, &parsed.modifiers)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;

                if candidate_ids.is_empty() {
                    return Ok(Vec::new());
                }

                let id_set: HashSet<i64> = candidate_ids.into_iter().collect();

                // Embed the free text query
                let query_embedding = {
                    let model = model.clone();
                    let query = parsed.free_text.clone();
                    let model_name = model_name.clone();
                    tokio::task::spawn_blocking(move || {
                        let mut model = model.lock().unwrap();
                        embedder::embed_query(&mut model, &query, &model_name)
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?
                }?;

                // Search only within candidate IDs
                let hits = EmbeddingStore::search_filtered(
                    &query_embedding,
                    &cached,
                    Some(&id_set),
                    SEARCH_LIMIT,
                );

                EmbeddingStore::hydrate_hits(&pool, &hits).await
            })
            .await;

            let results = match results {
                Ok(Ok(results)) => results,
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "hybrid search failed");
                    Vec::new()
                }
                Err(e) => {
                    tracing::error!(error = %e, "hybrid search task failed");
                    Vec::new()
                }
            };

            let _ = this.update(cx, |this, cx| {
                this.set_status(SearchEngineStatus::Ready, cx);
            });

            results
        })
    }

    /// Enable or disable the engine.
    pub fn set_enabled(&mut self, enabled: bool, model_name: String, cx: &mut Context<Self>) {
        if enabled && matches!(self.status, SearchEngineStatus::Disabled) {
            self.model_name = model_name;
            self.status = SearchEngineStatus::DownloadingModel;
            self.start_init(cx);
        } else if !enabled {
            self.status = SearchEngineStatus::Disabled;
            self.model = None;
            self.cached_embeddings.clear();
            self._task = None;
            cx.emit(SearchEngineEvent);
            cx.notify();
        }
    }

    /// Force status back to Ready (used when a search task is cancelled by debounce).
    pub fn force_ready(&mut self, cx: &mut Context<Self>) {
        if matches!(self.status, SearchEngineStatus::Searching) {
            self.set_status(SearchEngineStatus::Ready, cx);
        }
    }

    fn set_status(&mut self, status: SearchEngineStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.emit(SearchEngineEvent);
        cx.notify();
    }
}

enum EmbedProgress {
    ModelReady { model: SharedModel },
    Indexing { done: usize, total: usize },
    CacheLoaded { embeddings: Vec<EmbeddedMessage> },
    Complete,
    Failed { error: String },
}

async fn run_init(model_name: &str, pool: &SqlitePool, tx: flume::Sender<EmbedProgress>) {
    let model_name_owned = model_name.to_string();
    let model =
        match tokio::task::spawn_blocking(move || embedder::init_model(&model_name_owned)).await {
            Ok(Ok(model)) => Arc::new(Mutex::new(model)),
            Ok(Err(e)) => {
                let _ = tx.send(EmbedProgress::Failed {
                    error: e.to_string(),
                });
                return;
            }
            Err(e) => {
                let _ = tx.send(EmbedProgress::Failed {
                    error: e.to_string(),
                });
                return;
            }
        };

    let _ = tx.send(EmbedProgress::ModelReady {
        model: model.clone(),
    });

    if let Err(e) = run_batch_embed(model_name, &model, pool, &tx).await {
        let _ = tx.send(EmbedProgress::Failed {
            error: e.to_string(),
        });
        return;
    }

    match EmbeddingStore::load_all(pool, model_name).await {
        Ok(embeddings) => {
            let _ = tx.send(EmbedProgress::CacheLoaded { embeddings });
        }
        Err(e) => {
            let _ = tx.send(EmbedProgress::Failed {
                error: e.to_string(),
            });
            return;
        }
    }

    let _ = tx.send(EmbedProgress::Complete);
}

async fn run_batch_embed(
    model_name: &str,
    model: &SharedModel,
    pool: &SqlitePool,
    tx: &flume::Sender<EmbedProgress>,
) -> anyhow::Result<()> {
    let missing_ids = EmbeddingStore::missing_message_ids(pool, model_name).await?;
    let total = missing_ids.len();

    if total == 0 {
        let _ = tx.send(EmbedProgress::Indexing { done: 0, total: 0 });
        return Ok(());
    }

    let _ = tx.send(EmbedProgress::Indexing { done: 0, total });

    let mut done = 0;
    for chunk_ids in missing_ids.chunks(EMBED_BATCH_SIZE) {
        let texts = EmbeddingStore::fetch_texts(pool, chunk_ids, model_name).await?;
        if texts.is_empty() {
            done += chunk_ids.len();
            continue;
        }

        let ids: Vec<i64> = texts.iter().map(|(id, _)| *id).collect();
        let text_strings: Vec<String> = texts.into_iter().map(|(_, text)| text).collect();

        let model_clone = model.clone();
        let embeddings = tokio::task::spawn_blocking(move || {
            let mut model = model_clone.lock().unwrap();
            embedder::embed_batch(&mut model, &text_strings)
        })
        .await??;

        for (id, embedding) in ids.iter().zip(embeddings.iter()) {
            EmbeddingStore::insert(pool, *id, model_name, embedding).await?;
        }

        done += chunk_ids.len();
        let _ = tx.send(EmbedProgress::Indexing { done, total });
    }

    Ok(())
}

async fn run_incremental_embed(
    model_name: &str,
    model: &SharedModel,
    pool: &SqlitePool,
    tx: flume::Sender<EmbedProgress>,
) {
    match run_batch_embed(model_name, model, pool, &tx).await {
        Ok(()) => {}
        Err(e) => {
            let _ = tx.send(EmbedProgress::Failed {
                error: e.to_string(),
            });
            return;
        }
    }

    match EmbeddingStore::load_all(pool, model_name).await {
        Ok(embeddings) => {
            let _ = tx.send(EmbedProgress::CacheLoaded { embeddings });
        }
        Err(e) => {
            let _ = tx.send(EmbedProgress::Failed {
                error: e.to_string(),
            });
            return;
        }
    }

    let _ = tx.send(EmbedProgress::Complete);
}
