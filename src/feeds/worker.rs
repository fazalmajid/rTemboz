/// Copyright (C) 2026 Fazal Majid
///
/// This program is free software: you can redistribute it and/or modify
/// it under the terms of the GNU Affero General Public License as published by
/// the Free Software Foundation, either version 3 of the License, or
/// (at your option) any later version.
///
/// This program is distributed in the hope that it will be useful,
/// but WITHOUT ANY WARRANTY; without even the implied warranty of
/// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
/// GNU Affero General Public License for more details.
///
/// You should have received a copy of the GNU Affero General Public License
/// along with this program.  If not, see <https://www.gnu.org/licenses/>.
///
use crate::db::feeds::{get_feeds, Feed, FeedStatus};
use crate::db::items::get_bloom;
use crate::db::rules::get_filters;
use crate::db::worker::DbOp;
use crate::feeds::normalize::{extract, process_rss};
use crate::feeds::work::Work;
use crate::filter::Filters;
use chrono::NaiveDateTime;
use fastbloom::AtomicBloomFilter;
use feedparser_rs::types::ParsedFeed;
use feedparser_rs::{parse, FeedError};
use log::{error, info};
use reqwest::header::{ToStrError, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, USER_AGENT};
use reqwest::Client;
use sqlx::sqlite::SqlitePool;
use std::error::Error;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error as ThisError;
use time;
use tokio::task;

// XXX make this configurable
const DEFAULT_TIMEOUT: u64 = 10;
const MY_USER_AGENT: &str = "rTemboz (https://github.com/fazalmajid/rTemboz)";
// minimum time between refreshes in seconds
const MIN_REFRESH_INTERVAL: i64 = 3600;

/// fetch feed over HTTP, being respectful of servers' capacity, e.g.
/// https://utcc.utoronto.ca/~cks/space/blog/web/FeedLimitingImportance
/// https://brntn.me/blog/respectfully-requesting-rss-feeds/
/// https://rachelbythebay.com/w/2022/03/07/get/
/// https://rachelbythebay.com/w/2023/01/18/http/
///
struct Fetch {
    status: u16,
    body: String,
    etag: String,
}

#[derive(Debug, ThisError)]
pub enum FetchError {
    #[error("reqwest::Error")]
    ReqwestError(#[from] reqwest::Error),
    #[error("ToStrError")]
    ToStrError(#[from] ToStrError),
    #[error("time::error::Format")]
    TimeFormat(#[from] time::error::Format),
    #[error("feedparser_rs::FeedError")]
    FeedError(#[from] FeedError),
    #[error("SendError<FeedOp>")]
    Send(#[from] Box<tokio::sync::mpsc::error::SendError<FeedOp>>),
    #[error("SendError<FeedOp>")]
    Recv(#[from] tokio::sync::oneshot::error::RecvError),
}

async fn http_fetch(
    feed_uid: u32,
    url: &String,
    etag: &String,
    last_fetched: NaiveDateTime,
    timeout_secs: u64,
) -> Result<Fetch, FetchError> {
    let client = Client::builder().cookie_store(true).build()?;
    let req = client
        .get(url.clone())
        .timeout(Duration::from_secs(timeout_secs))
        .header(USER_AGENT, MY_USER_AGENT);
    let last_fetched_rfc7231 = last_fetched.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let req2 = match etag.as_str() {
        "" => req.header(IF_MODIFIED_SINCE, last_fetched_rfc7231.clone()),
        _ => req.header(IF_NONE_MATCH, etag),
    };
    let res = req2.send().await?;
    let status = res.status().as_u16();
    let etag = match res.headers().get(ETAG) {
        Some(h) => h.to_str()?,
        _ => "",
    }
    .to_string();
    let body = res.text().await?;
    info!(
        "FEED-{} fetched {} HTTP status={} etag={} last_fetched={}",
        feed_uid, url, status, etag, last_fetched_rfc7231
    );
    Ok(Fetch { status, body, etag })
}

pub async fn fetch_and_parse(url: String) -> Result<ParsedFeed, FetchError> {
    let r = http_fetch(
        0,
        &url,
        &"".to_string(),
        chrono::DateTime::UNIX_EPOCH.naive_utc(),
        60,
    )
    .await?;
    Ok(parse(r.body.as_bytes())?)
}

pub async fn dump(url: String) -> Result<(), FetchError> {
    let parsed = fetch_and_parse(url).await?;
    println!("{:#?}", parsed);
    for i in parsed.entries {
        println!("{:#?}", extract(&i).unwrap());
    }
    Ok(())
}

async fn fetch(
    f: Feed,
    db_q: mpsc::Sender<DbOp>,
    bf: Arc<AtomicBloomFilter>,
    filters: Arc<Filters>,
) -> Option<Work> {
    info!("FEED-{} fetching {} etag=\"{}\"", f.uid, f.xml, f.etag);
    // XXX make this timeout configurable
    let r = match http_fetch(f.uid, &f.xml, &f.etag, f.last_fetched, DEFAULT_TIMEOUT).await {
        Ok(body) => {
            let _ = db_q.send(DbOp::FeedFetchSuccess {
                uid: f.uid,
                etag: body.etag.clone(),
            });
            body
        }
        Err(e) => {
            let _ = db_q.send(DbOp::FeedError {
                uid: f.uid,
                error: e.to_string(),
                source: match e.source() {
                    Some(src) => src.to_string(),
                    _ => "".to_string(),
                },
            });
            return None;
        }
    };
    // HTTP 304 - unchanged
    if r.status == 304 {
        info!("FEED-{} no change to {}", f.uid, f.xml);
        return None;
    }
    // XXX TODO should handle HTTP 301, 302, 429 status
    info!("FEED-{} fetched {} -> etag {}", f.uid, f.xml, r.etag);
    let parsed = match parse(r.body.as_bytes()) {
        Err(e) => {
            let _ = db_q.send(DbOp::FeedError {
                uid: f.uid,
                error: e.to_string(),
                source: match e.source() {
                    Some(src) => src.to_string(),
                    _ => "".to_string(),
                },
            });
            return None;
        }
        Ok(p) => p,
    };
    Some(Work {
        feed_uid: f.uid,
        rss: parsed,
        // etag: r.etag,
        bloom: bf,
        filters,
        db_q: db_q.clone(),
    })
}

async fn need_fetching(db: &SqlitePool) -> Result<Vec<Feed>, sqlx::error::Error> {
    let feeds = get_feeds(db).await?;
    let active: Vec<Feed> = feeds
        .into_iter()
        .filter(|f| {
            matches!(f.status, FeedStatus::Active)
                && (f.last_fetched
                    < (chrono::Local::now() - chrono::Duration::seconds(MIN_REFRESH_INTERVAL))
                        .naive_local())
        })
        .collect();
    Ok(active)
}

async fn record_one_already_fetched(
    db_q: mpsc::Sender<DbOp>,
    bf: Arc<AtomicBloomFilter>,
    filters: Arc<Filters>,
    feed_uid: u32,
    parsed: ParsedFeed,
    reply: Option<tokio::sync::oneshot::Sender<(u32, u32)>>,
) -> Result<(), sqlx::error::Error> {
    let _: () = process_rss(
        Work {
            feed_uid,
            rss: parsed,
            bloom: bf.clone(),
            filters: filters.clone(),
            db_q: db_q.clone(),
        },
        reply,
    )
    .await;
    Ok(())
}

pub async fn refresh_one(
    work_q: &tokio::sync::mpsc::Sender<FeedOp>,
    feed: Feed,
) -> Result<(u32, u32), FetchError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    if let Err(e) = work_q
        .send(FeedOp::RefreshOne {
            feed: Box::new(feed),
            reply: tx,
        })
        .await
    {
        error!("error enqueuing new feed to be refreshed: {}", e);
        return Err(FetchError::Send(Box::new(e)));
    }
    let (added, filtered) = match rx.await {
        Ok(tup) => tup,
        Err(e) => {
            let source = match e.source() {
                Some(src) => src.to_string(),
                _ => "".to_string(),
            };
            error!("could not receive fetch counts: {} ({})", e, source);
            return Err(FetchError::Recv(e));
        }
    };

    Ok((added, filtered))
}

async fn fetch_one(
    db_q: mpsc::Sender<DbOp>,
    bf: Arc<AtomicBloomFilter>,
    filters: Arc<Filters>,
    feed: Feed,
    reply: tokio::sync::oneshot::Sender<(u32, u32)>,
) -> Result<(), sqlx::error::Error> {
    let bf = bf.clone();
    let filters = filters.clone();
    let db_q = db_q.clone();
    let task = task::spawn(async move {
        match fetch(feed, db_q, bf.clone(), filters.clone()).await {
            Some(work) => process_rss(work, Some(reply)).await,
            None => {
                let _ = reply.send((0, 0));
            }
        }
    });
    if let Err(e) = task.await {
        error!("error running RSS fetch: {}", e)
    }
    Ok(())
}

async fn fetch_some(
    _db: &SqlitePool,
    db_q: mpsc::Sender<DbOp>,
    bf: Arc<AtomicBloomFilter>,
    filters: Arc<Filters>,
    to_fetch: Vec<Feed>,
) -> Result<(), sqlx::error::Error> {
    let tasks: Vec<_> = to_fetch
        .into_iter()
        .map(|f| {
            let bf = bf.clone();
            let filters = filters.clone();
            let db_q = db_q.clone();
            task::spawn(async move {
                if let Some(work) = fetch(f, db_q, bf.clone(), filters.clone()).await {
                    process_rss(work, None).await
                }
            })
        })
        .collect();
    for task in tasks {
        if let Err(e) = task.await {
            error!("error running RSS fetch: {}", e)
        }
    }
    Ok(())
}

pub async fn fetch_all(
    db: &SqlitePool,
    db_q: mpsc::Sender<DbOp>,
    bf: Arc<AtomicBloomFilter>,
    filters: Arc<Filters>,
) -> Result<(), sqlx::error::Error> {
    fetch_some(db, db_q, bf, filters, need_fetching(db).await?).await
}

pub async fn feed_worker_setup(db: &SqlitePool) -> (Arc<AtomicBloomFilter>, Arc<Filters>) {
    info!("building guid bloom filter");
    let bloom_db = db.clone();
    let bf = Arc::new(get_bloom(&bloom_db).await.unwrap());
    info!(
        "num_bits = {}, num_hashes = {}",
        bf.num_bits(),
        bf.num_hashes()
    );
    let rules_db = db.clone();
    let filters = Arc::new(get_filters(&rules_db).await.unwrap());
    (bf, filters)
}

pub enum FeedOp {
    RefreshOne {
        feed: Box<Feed>,
        reply: tokio::sync::oneshot::Sender<(u32, u32)>,
    },
    AlreadyFetched {
        feed_uid: u32,
        parsed: Box<ParsedFeed>,
        reply: tokio::sync::oneshot::Sender<(u32, u32)>,
    },
    InvalidateFilters,
}

pub fn spawn_worker(
    db: SqlitePool,
    work_q: mpsc::Sender<DbOp>,
) -> (tokio::sync::mpsc::Sender<FeedOp>, task::JoinHandle<()>) {
    let (sender, mut receiver) = tokio::sync::mpsc::channel::<FeedOp>(1);
    let join_handle = task::spawn(async move {
        let (mut bf, mut filters) = feed_worker_setup(&db).await;
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15 * 60));
        loop {
            tokio::select! {
                Some(op) = receiver.recv() => {
                    match op {
                        // FeedOp::RefreshAll => {
                        //     fetch_all(&db.clone(), work_q.clone(), bf.clone(), filters.clone()).await;
                        //     interval.reset()
                        // },
                        FeedOp::RefreshOne{feed, reply} => {
                            if let Err(e) = fetch_one(work_q.clone(), bf.clone(), filters.clone(), *feed, reply).await {
                                error!("error in RefreshOne: {}", e);
                            }
                        },
                        FeedOp::AlreadyFetched{feed_uid, parsed, reply} => {
                            if let Err(e) = record_one_already_fetched(work_q.clone(), bf.clone(), filters.clone(), feed_uid, *parsed, Some(reply)).await {
                                error!("error in AlreadyFetched: {}", e);
                            }
                        },
                        FeedOp::InvalidateFilters => {
                            info!("reloading filtering rules");
                            (bf, filters) = feed_worker_setup(&db).await;

                        }
                    }
                }
                _ = interval.tick() => {
                    let _ = fetch_all(&db.clone(), work_q.clone(), bf.clone(), filters.clone()).await;
                }
            }
        }
    });
    (sender, join_handle)
}
