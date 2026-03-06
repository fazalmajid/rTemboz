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
use crate::db::feed::{update_feed, update_feed_error};
use crate::db::items::{ItemStatus, save_item, update_status};
use crate::feeds::normalize::Item;
use log::{error, info};
use sqlx::sqlite::{SqliteConnection, SqlitePool};
use std::fmt;
use std::sync::mpsc;
use std::thread;

pub enum DbOp {
    Quit,
    // RebuildViews,
    UpDown {
        new_status: ItemStatus,
        uid: u64,
    },
    FeedError {
        uid: u32,
        error: String,
        source: String,
    },
    FeedFetchSuccess {
        uid: u32,
        etag: String,
    },
    NewItem {
        feed_uid: u32,
        rule_uid: Option<u32>,
        item: Item,
    },
}

impl fmt::Display for DbOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbOp::Quit => write!(f, "Quit"),
            DbOp::UpDown { new_status, uid } => write!(f, "UpdateStatus({}, {})", new_status, uid),
            DbOp::FeedError { uid, error, source } => {
                write!(f, "FeedError uid={} error={} source={}", uid, error, source)
            }
            DbOp::FeedFetchSuccess { uid, etag } => {
                write!(f, "FeedFetchSuccess uid={} etag={}", uid, etag)
            }
            DbOp::NewItem {
                feed_uid,
                rule_uid,
                item,
            } => write!(
                f,
                "NewItem feed={} rule={} title={}",
                feed_uid,
                rule_uid.unwrap_or(0),
                item.title
            ),
        }
    }
}

async fn work(conn: &mut SqliteConnection, work_q: mpsc::Receiver<DbOp>) {
    info!("starting DB single writer worker");
    while let Ok(op) = work_q.recv() {
        info!("dbworker: received {}", op);
        match op {
            DbOp::Quit => return,
            DbOp::UpDown { new_status, uid } => {
                update_status(conn, new_status, uid as i64).await.unwrap()
            }
            DbOp::FeedError { uid, error, source } => {
                match update_feed_error(conn, uid, format!("error: {}\nsource: {}", error, source))
                    .await
                {
                    Ok(()) => info!("FEED-{} updated errors", uid),
                    Err(e) => error!(
                        "FEED-{} error updating feed errors: {} original_error={} source={}",
                        uid, e, error, source
                    ),
                }
            }
            DbOp::FeedFetchSuccess {
                uid: feed_uid,
                etag,
            } => match update_feed(conn, feed_uid, etag).await {
                Ok(()) => info!("FEED-{} updated", feed_uid),
                Err(e) => error!("FEED-{} error updating feed: {}", feed_uid, e),
            },
            DbOp::NewItem {
                feed_uid,
                rule_uid,
                item,
            } => match save_item(conn, feed_uid, rule_uid, &item).await {
                Ok(uid) => info!("FEED-{} saved {} as uid {}", feed_uid, item.title, uid),
                Err(e) => error!("error saving NewItem: {}", e),
            },
        }
    }
}

pub fn spawn(pool: &SqlitePool) -> (mpsc::Sender<DbOp>, thread::JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel::<DbOp>();
    let pool = pool.clone();
    let handle = thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut conn = rt.block_on(pool.acquire()).unwrap().leak();
        rt.block_on(work(&mut conn, receiver));
    });
    (sender, handle)
}
