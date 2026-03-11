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
use crate::db::items::UniqueItem;
use crate::db::worker::DbOp;
use crate::feeds::work::Work;
use crate::utils::{clean_text, clean_url};
use ammonia::clean;
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use feedparser_rs::types::{Content, Entry, Person};
use log::{error, info};
use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Debug)]
pub struct Item {
    pub guid: String,
    pub url: String,
    pub title: String,
    pub author: String,
    pub tags: Vec<String>,
    pub content: String,
    pub published: DateTime<Utc>,
    pub updated: Option<DateTime<Utc>>,
}

impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.guid, self.title, self.url)
    }
}

fn first_author_name(entry: &Entry) -> String {
    match entry.authors.first() {
        Some(Person { name: Some(s), .. }) => s.as_str(),
        _ => "Unknown",
    }
    .to_string()
}

fn sha256(s: String) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn extract(e: &Entry) -> Result<Item> {
    let author = clean_text(&first_author_name(e));
    let title = clean_text(&e.title.clone().unwrap_or_else(|| "Untitled".to_string()));
    let tags: Vec<String> = e
        .tags
        .iter()
        .map(|cat| clean_text(&cat.term.clone().into_string().to_owned()))
        .collect();
    let url = match e.links.first() {
        Some(l) => clean_url(&l.href)?,
        _ => bail!("no link for {}", title),
    };
    let no_content = "no content".to_string();
    let content = clean(match e.content.first() {
        Some(Content { value: s, .. }) => s,
        _ => match &e.summary {
            Some(s) => s,
            _ => &no_content,
        },
    });
    let guid = match e.id.clone() {
        Some(s) => s.into_string(),
        _ => match url.as_str() {
            "" => sha256(content.clone()),
            _ => url.clone(),
        },
    };
    Ok(Item {
        guid,
        url,
        title,
        author,
        tags,
        content,
        published: e.published.unwrap_or_else(Utc::now),
        updated: e.updated,
    })
}

pub async fn process_rss(work: Work, reply: Option<tokio::sync::oneshot::Sender<(u32, u32)>>) {
    // match serde_json::to_string(&work.rss) {
    //     Err(e) => info!("feed {} parsed but unserializable: {}", work.feed_uid, e),
    //     Ok(s) => info!("feed {} parsed successfully {}", work.feed_uid, s),
    // };
    let Work {
        feed_uid,
        rss,
        bloom,
        filters,
        db_q,
        // etag: _,
    } = work;
    let mut added: u32 = 0;
    let mut filtered: u32 = 0;
    for entry in rss.entries {
        let entry_id = entry.id.clone();
        let item = match extract(&entry) {
            Err(e) => {
                error!(
                    "FEED-{} error {} extracting {}",
                    feed_uid,
                    e,
                    entry_id.as_deref().unwrap_or("no GUID")
                );
                continue;
            }
            Ok(v) => v,
        };
        let key = UniqueItem {
            feed: feed_uid,
            guid: item.guid.clone(),
        };
        if bloom.contains(&key) {
            // info!(
            //     "item already recorded {} {} {}",
            //     item.title, key.feed, key.guid
            // );
            continue;
        } else {
            info!("NEW ITEM feed={} {} {}", feed_uid, item.guid, item.title);
            bloom.insert(&key);
            added += 1;
        }
        let result = match filters.apply_filter(feed_uid, &item).await {
            Ok(rule_uid) => {
                if let Some(uid) = rule_uid {
                    info!(
                        "FILTERED ITEM feed={} rule={} {} {}",
                        feed_uid, uid, item.guid, item.title
                    );
                    filtered += 1;
                }
                db_q.send(DbOp::NewItem {
                    feed_uid,
                    rule_uid,
                    item,
                })
            }
            Err(e) => {
                error!("item filtering error {e}");
                db_q.send(DbOp::NewItem {
                    feed_uid,
                    rule_uid: None,
                    item,
                })
            }
        };
        if let Err(e) = result {
            error!("error saving new item: {}", e);
        }
    }
    if let Some(channel) = reply {
        let _ = channel.send((added, filtered));
    }
}
