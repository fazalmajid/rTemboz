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
use crate::db::fts5::fts5_term;
use crate::db::{safe_truncate, since, unix_ts};
use crate::feeds::normalize::Item as RssItem;
use crate::filter::{rule_from_string, Rule};
use crate::utils::{clean_text, clean_url};
use ammonia::clean;
use chrono::prelude::*;
use fastbloom::AtomicBloomFilter;
use futures::TryStreamExt;
use log::{error, info};
use sqlx::error::Error;
use sqlx::sqlite::{SqliteConnection, SqlitePool};
use std::fmt;
use std::hash::Hash;
use std::str::FromStr;
use thiserror::Error as ThisError;
use url_normalize::NormalizeUrlError;

#[derive(Copy, Clone)]
pub enum ItemStatus {
    Filtered = -2,
    Uninteresting = -1,
    Unread = 0,
    Interesting = 1,
    All,
}
impl fmt::Display for ItemStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemStatus::Filtered => write!(f, "Filtered"),
            ItemStatus::Uninteresting => write!(f, "Uninteresting"),
            ItemStatus::Unread => write!(f, "Unread"),
            ItemStatus::Interesting => write!(f, "Interesting"),
            ItemStatus::All => write!(f, "All"),
        }
    }
}

impl FromStr for ItemStatus {
    type Err = ();
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "all" => Ok(ItemStatus::All),
            "all articles" => Ok(ItemStatus::All),
            "unread" => Ok(ItemStatus::Unread),
            "down" => Ok(ItemStatus::Uninteresting),
            "uninteresting" => Ok(ItemStatus::Uninteresting),
            "up" => Ok(ItemStatus::Interesting),
            "interesting" => Ok(ItemStatus::Interesting),
            "filtered" => Ok(ItemStatus::Filtered),
            _ => Err(()),
        }
    }
}

#[derive(Copy, Clone)]
pub enum ItemOrder {
    Published,
    Seen,
    Rated,
    Snr,
    Oldest,
    Random,
}
impl fmt::Display for ItemOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemOrder::Published => write!(f, "published"),
            ItemOrder::Seen => write!(f, "seen"),
            ItemOrder::Rated => write!(f, "rated"),
            ItemOrder::Snr => write!(f, "SNR"),
            ItemOrder::Oldest => write!(f, "oldest"),
            ItemOrder::Random => write!(f, "random"),
        }
    }
}

impl FromStr for ItemOrder {
    type Err = ();
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "published" => Ok(ItemOrder::Published),
            "seen" => Ok(ItemOrder::Seen),
            "rated" => Ok(ItemOrder::Rated),
            "snr" => Ok(ItemOrder::Snr),
            "oldest" => Ok(ItemOrder::Oldest),
            "random" => Ok(ItemOrder::Random),
            _ => Err(()),
        }
    }
}

impl ItemOrder {
    fn where_clause(self) -> String {
        match self {
            ItemOrder::Published => "item.published DESC".to_string(),
            ItemOrder::Seen => "item.uid DESC".to_string(),
            ItemOrder::Rated => "item.rated DESC".to_string(),
            ItemOrder::Snr => "snr DESC".to_string(),
            ItemOrder::Oldest => "item.uid ASC".to_string(),
            ItemOrder::Random => "random() ASC".to_string(),
        }
        .to_string()
    }

    // fn human(self) -> String {
    //     match self {
    //         ItemOrder::Published => "Article date".to_string(),
    //         ItemOrder::Seen => "Cached date".to_string(),
    //         ItemOrder::Rated => "Rated on".to_string(),
    //         ItemOrder::Snr => "Feed SNR".to_string(),
    //         ItemOrder::Oldest => "Oldest seen".to_string(),
    //         ItemOrder::Random => "Random order".to_string(),
    //     }
    //     .to_string()
    // }
}

pub struct Item {
    pub uid: u64,
    pub since_when: String,
    pub creator: String,
    pub loaded: DateTime<Local>,
    pub feed_uid: u32,
    pub title: String,
    pub feed_html: String,
    pub content: String,
    pub tags: Vec<String>,
    pub redirect: String,
    pub feed_title: String,
    //pub rating: i8,
    pub feed_exempt: bool,
    //pub feed_snr: f32,
    //pub updated_ts: DateTime<Local>,
    pub rule: Option<Rule>,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    item_uid: u64,
    feed_uid: u32,
    creator: String,
    item_title: String,
    link: String,
    content: String,
    loaded: i64,
    // published: i64,
    // rated: i64,
    delta_published: f64,
    rule_uid: Option<u32>,
    rule_type: Option<String>,
    rule_feed: Option<u32>,
    rule_text: Option<String>,
    // rating: i8,
    feed_title: String,
    html: String,
    // xml: String,
    exempt: u8,
    tags: String,
}

#[derive(ThisError, Debug)]
pub enum GetError {
    #[error("Database error")]
    DB(#[from] Error),
    #[error("Sanitize Error")]
    Sanitize(#[from] NormalizeUrlError),
}

pub async fn get_items(
    db: &SqlitePool,
    show: ItemStatus,
    feed_uid: Option<u32>,
    search: Option<String>,
    search_in: Option<String>,
    order: ItemOrder,
) -> Result<Vec<Item>, GetError> {
    let mut bind_index = 1;
    let show_clause = match show {
        ItemStatus::All => String::new(),
        _ => {
            bind_index += 1;
            format!(" AND item.rating=${}", bind_index - 1)
        }
    };
    let feed_clause = match feed_uid {
        Some(_) => {
            bind_index += 1;
            format!(" AND feed_uid=${}", bind_index - 1)
        }
        _ => String::new(),
    };
    let search_context = match search_in.as_deref() {
        Some("title") => "title match ",
        _ => "search=",
    };
    let search_clause = match search {
        Some(_) => {
            bind_index += 1;
            format!(
                " AND item.uid IN (SELECT rowid FROM search WHERE {}${})",
                search_context,
                bind_index - 1
            )
        }
        _ => String::new(),
    };
    let sql = format!(
        r###"
SELECT
  item.uid AS item_uid, feed.uid AS feed_uid, creator,
  item.title AS item_title, link, content, unixepoch(loaded) AS loaded,
  julianday('now') - julianday(published) AS delta_published,
  rule.uid AS rule_uid, rule.type AS rule_type, rule.feed AS rule_feed,
  rule.text AS rule_text,
  feed.title AS feed_title, html, exempt,
  COALESCE(json_group_array(tag.name), '[]') AS tags
FROM item
JOIN feed ON item.feed=feed.uid
LEFT OUTER JOIN tag ON tag.item=item.uid
LEFT OUTER JOIN rule ON item.rule=rule.uid
LEFT OUTER JOIN mv_feed_stats ON feed.uid=mv_feed_stats.feed
WHERE item.feed=feed.uid{}{}{}
GROUP BY 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15
ORDER BY {} limit 200"###,
        show_clause,
        feed_clause,
        search_clause,
        order.where_clause()
    );
    // XXX this is clunky
    let query = sqlx::query_as::<_, ItemRow>(sql.as_str());
    let query = match show {
        ItemStatus::All => query,
        _ => query.bind(show as i8),
    };
    let query = match feed_uid {
        Some(uid) => query.bind(uid),
        None => query,
    };
    let query = match search {
        Some(term) => query.bind(fts5_term(&term)),
        None => query,
    };
    let rows = query.fetch_all(db).await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let tag_str = row.tags.as_str();
        if row.item_uid == 8835079 {
            info!("XXXXXXXXXXX {}", row.tags);
        }
        let tags: Vec<String> = if row.tags == "[null]" {
            vec![]
        } else {
            serde_json::from_str::<Vec<String>>(tag_str)
                .unwrap()
                .into_iter()
                .map(|tag| clean_text(&tag))
                .collect()
        };
        let rule = match row.rule_uid {
            Some(uid) => Some(Rule {
                uid,
                rule_type: rule_from_string(row.rule_type.unwrap()),
                text: row.rule_text.unwrap(),
                feed: row.rule_feed,
                feed_title: None,
            }),
            _ => None,
        };

        result.push(Item {
            uid: row.item_uid,
            feed_uid: row.feed_uid,
            since_when: since(row.delta_published),
            creator: clean_text(&row.creator),
            loaded: unix_ts(Some(row.loaded)),
            title: row.item_title,
            feed_html: match clean_url(&row.html) {
                Err(e) => {
                    error!(
                        "FEED-{} item {} invalid feed URL \"{}\": {}",
                        row.feed_uid, row.item_uid, row.html, e
                    );
                    "about:blank".to_string()
                }
                Ok(s) => s,
            },
            content: clean(row.content.as_str()).to_string(),
            tags,
            redirect: match clean_url(&row.link) {
                Err(e) => {
                    error!(
                        "FEED-{} item {} invalid item URL \"{}\": {}",
                        row.feed_uid, row.item_uid, row.link, e
                    );
                    "about:blank".to_string()
                }
                Ok(s) => s,
            },
            feed_title: row.feed_title,
            //rating: row.rating.unwrap_or(0) as i8,
            feed_exempt: row.exempt != 0,
            rule,
        });
    }
    Ok(result)
}

pub async fn update_status(
    conn: &mut SqliteConnection,
    new_status: ItemStatus,
    uid: i64,
) -> Result<(), Error> {
    let status_numeric = new_status as i8;
    let _ = sqlx::query!(
        r###"
UPDATE item
SET rating=?, rated=julianday('now')
WHERE uid=?"###,
        status_numeric,
        uid,
    )
    .fetch_optional(conn)
    .await?;
    Ok(())
}

// XXX use this as the return type for the row to avoid copy
#[derive(Hash)]
pub struct UniqueItem {
    pub feed: u32,
    pub guid: String,
}

pub async fn get_bloom(db: &SqlitePool) -> Result<AtomicBloomFilter, Error> {
    let row = sqlx::query!("SELECT COUNT(*) AS cnt FROM item")
        .fetch_one(db)
        .await?;
    info!("Creating bloom filter for {} items", row.cnt);
    // fastbloom doesn't like a filter with expected capacity 0
    // https://github.com/tomtomwombat/fastbloom/issues/17
    let bf = AtomicBloomFilter::with_false_pos(0.001)
        .expected_items(std::cmp::max(10_000_000, 4 * row.cnt as usize));
    // stream data rather than load them all into RAM
    let mut stream = sqlx::query!("SELECT feed, guid FROM item").fetch(db);
    while let Some(row) = stream.try_next().await? {
        let u = UniqueItem {
            feed: row.feed as u32,
            guid: row.guid,
        };
        bf.insert(&u);
        // info!("BF insert feed={} {}", u.feed, u.guid);
    }
    Ok(bf)
}

pub async fn save_item(
    conn: &mut SqliteConnection,
    feed_uid: u32,
    rule_uid: Option<u32>,
    item: &RssItem,
) -> Result<u64, Error> {
    let rating = match rule_uid {
        Some(_) => -2,
        None => 0,
    };
    let row = sqlx::query!(
        r###"
INSERT INTO item (
  feed,
  guid,
  loaded,
  published,
  modified,
  link,
  title,
  content,
  creator,
  rating,
  rule
) VALUES (
  ?, ?,julianday('now'), ?, ?, ?, ?, ?, ?, ?, ?
) RETURNING uid"###,
        feed_uid,
        item.guid,
        item.published,
        item.updated,
        item.url,
        item.title,
        item.content,
        item.author,
        rating,
        rule_uid
    )
    .fetch_one(&mut *conn)
    .await?;
    let item_uid = row.uid.unwrap();
    for tag in &item.tags {
        let tag = safe_truncate(tag.to_string(), 64);
        if let Err(e) = sqlx::query!(
            r###"INSERT INTO tag (name, item) VALUES (?, ?)"###,
            tag,
            item_uid
        )
        .execute(&mut *conn)
        .await
        {
            error!("error setting tag: {}", e)
        }
    }
    Ok(item_uid as u64)
}
