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
use crate::db::feeds::{Feed, FeedStatus};
use crate::db::since;
use crate::feeds::worker::FeedOp;
use crate::feeds::worker::{fetch_and_parse, FetchError};
use feedparser_rs::FeedError;
use log::error;
use sqlx::error::Error;
use sqlx::sqlite::{SqliteConnection, SqlitePool};
use thiserror::Error as ThisError;

pub async fn get_feed_details(db: &SqlitePool, uid: u32) -> Result<Feed, Error> {
    let row = sqlx::query!(
        r###"
SELECT
  f.uid, f.title, f.description, f.html, f.xml,
  COALESCE(f.pubxml, f.xml) AS pubxml, last_modified, interesting, unread,
  uninteresting, filtered, total, snr, f.status, f.private, f.exempt,
  f.errors, f.dupcheck, f.etag, f.last_fetched
FROM feed f
JOIN v_feeds_snr v ON f.uid=v.uid
WHERE f.uid=?
"###,
        uid
    )
    .fetch_one(db)
    .await?;
    let feed = Feed {
        uid: row.uid as u32,
        title: row.title,
        description: row.description.unwrap_or("Unknown".to_string()),
        html: row.html,
        xml: row.xml,
        pubxml: row.pubxml,
        last_modified: since(row.last_modified.unwrap_or(0.0)),
        interesting: row.interesting as u32,
        unread: row.unread as u32,
        uninteresting: row.uninteresting as u32,
        filtered: row.filtered as u32,
        total: row.total as u32,
        snr: row.snr,
        status: if row.status == 0 {
            FeedStatus::Active
        } else {
            FeedStatus::Suspended
        },
        private: row.private.unwrap_or(0) != 0,
        exempt: row.exempt != 0,
        errors: row.errors as u32,
        dupcheck: row.dupcheck.unwrap_or(0) != 0,
        etag: row.etag.unwrap_or("".to_string()),
        last_fetched: row
            .last_fetched
            .unwrap_or(chrono::DateTime::UNIX_EPOCH.naive_utc()),
    };
    Ok(feed)
}

pub async fn update_feed(conn: &mut SqliteConnection, uid: u32, etag: String) -> Result<(), Error> {
    sqlx::query!(
        r###"
UPDATE feed
SET
   etag = ?,
   last_fetched = julianday('now'),
   last_error = NULL,
   error_text = NULL,
   errors = 0
WHERE uid=?
"###,
        etag,
        uid,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update_feed_error(
    conn: &mut SqliteConnection,
    uid: u32,
    error_text: String,
) -> Result<(), Error> {
    sqlx::query!(
        r###"
UPDATE feed
SET
   last_error = julianday('now'),
   errors = errors + 1,
   error_text = ?
WHERE uid=?
"###,
        error_text,
        uid,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn dedupe(db: &SqlitePool, feed_uid: u32) -> Result<u64, Error> {
    let rowcount = sqlx::query!(
        r###"
UPDATE item SET rating = -1
WHERE item.feed=? AND rating=0 AND EXISTS (
  SELECT * FROM item i2
  WHERE i2.feed=item.feed
    AND i2.uid<>item.uid
    AND i2.title=item.title
  )
"###,
        feed_uid
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(rowcount)
}

pub async fn catchup(db: &SqlitePool, feed_uid: u32) -> Result<u64, Error> {
    let rowcount = sqlx::query!(
        r###"UPDATE item SET rating = -1 WHERE item.feed=? AND rating=0"###,
        feed_uid
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(rowcount)
}

pub async fn reload(db: &SqlitePool, feed_uid: u32) -> Result<u64, Error> {
    let rowcount = sqlx::query!(
        r###"DELETE FROM item WHERE item.feed=? AND rating=0"###,
        feed_uid
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(rowcount)
}

#[derive(ThisError, Debug)]
pub enum AddError {
    #[error("Database error")]
    DB(#[from] Error),
    #[error("FeedError")]
    Feed(#[from] FeedError),
    #[error("FetchError")]
    Fetch(#[from] FetchError),
    #[error("SendError<FeedOp>")]
    Send(#[from] Box<tokio::sync::mpsc::error::SendError<FeedOp>>),
}

#[derive(Debug)]
pub struct Message {
    pub feed_uid: u32,
    pub feed_title: String,
    pub added: u32,
    pub filtered: u32,
}

pub async fn add_feed(
    conn: &SqlitePool,
    work_q: &tokio::sync::mpsc::Sender<FeedOp>,
    url: String,
) -> Result<Message, AddError> {
    let parsed = fetch_and_parse(url.clone()).await?;
    let feed_title = parsed.feed.title.clone().unwrap_or("Untitled".to_string());
    let row = sqlx::query!(
        r###"
INSERT INTO feed (
  xml,
  modified,
  html,
  title,
  description
) VALUES (
  ?, julianday('now'), ?, ?, ?
) RETURNING uid"###,
        url,
        parsed.feed.link,
        parsed.feed.title,
        parsed.feed.subtitle
    )
    .fetch_one(conn)
    .await?;
    // refresh feed
    let (tx, rx) = tokio::sync::oneshot::channel();
    if let Err(e) = work_q
        .send(FeedOp::AlreadyFetched {
            feed_uid: row.uid as u32,
            parsed: Box::new(parsed),
            reply: tx,
        })
        .await
    {
        error!("error enqueuing new feed to be refreshed: {}", e);
        return Err(AddError::Send(Box::new(e)));
    };
    let (added, filtered) = match rx.await {
        Ok(tup) => tup,
        Err(e) => {
            error!("could not receive fetch counts: {}", e);
            (0_u32, 0_u32)
        }
    };

    Ok(Message {
        feed_uid: row.uid as u32,
        feed_title,
        added,
        filtered,
    })
}
