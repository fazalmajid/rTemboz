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
use crate::filter::{new_filters, rule_from_string, FilterError, Filters, Rule};
use anyhow::{anyhow, Error as AnyhowError};
use log::{error, info};
use serde::{Deserialize, Serialize};
use sqlx::error::Error;
use sqlx::sqlite::SqlitePool;
use thiserror::Error as ThisError;

pub async fn get_top_rules(db: &SqlitePool, feed: u32) -> Result<Vec<(Rule, u32)>, Error> {
    let rows = sqlx::query!(
        r###"
SELECT rule, type AS rule_type, rule.feed AS rule_feed, text,
   COUNT(*) AS "cnt!: u32"
FROM item
JOIN rule ON rule.uid=item.rule
WHERE item.feed=? AND rating=-2
GROUP BY 1, 2, 3
ORDER BY 4 DESC, 3
LIMIT 25
"###,
        feed
    )
    .fetch_all(db)
    .await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        result.push((
            Rule {
                uid: row.rule.unwrap() as u32,
                rule_type: rule_from_string(row.rule_type),
                text: row.text,
                feed: row.rule_feed.map(|uid| uid as u32),
                feed_title: None,
            },
            row.cnt,
        ));
    }
    Ok(result)
}

pub async fn get_filters(db: &SqlitePool) -> Result<Filters, FilterError> {
    let rows = sqlx::query!(r###"SELECT uid, type AS rule_type, feed, text FROM rule"###)
        .fetch_all(db)
        .await?;
    let mut filters = new_filters();
    for row in rows {
        if let Err(e) = filters.add_rule(
            row.feed,
            Rule {
                uid: row.uid as u32,
                rule_type: rule_from_string(row.rule_type),
                text: row.text,
                feed: row.feed.map(|uid| uid as u32),
                feed_title: None,
            },
        ) {
            error!("error adding filter rule: {}", e)
        }
    }
    filters.finalize()?;
    Ok(filters)
}

pub async fn get_rules(db: &SqlitePool) -> Result<Vec<Rule>, FilterError> {
    let rows = sqlx::query!(
        r###"
SELECT rule.uid AS uid, type AS rule_type, feed, text,
   feed.title AS feed_title
FROM rule
LEFT OUTER JOIN feed ON rule.feed=feed.uid
"###
    )
    .fetch_all(db)
    .await?;
    let rules = rows
        .iter()
        .map(|row| Rule {
            uid: row.uid as u32,
            rule_type: rule_from_string(row.rule_type.clone()),
            text: row.text.clone(),
            feed: row.feed.map(|uid| uid as u32),
            feed_title: row.feed_title.clone(),
        })
        .collect();
    Ok(rules)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RuleData {
    kw: Option<String>,
    stem: Option<String>,
    match_type: String,
    target: String,
    feed_only: Option<String>,
    item_uid: u64,
}

#[derive(ThisError, Debug)]
pub enum SaveError {
    #[error("Database error")]
    DB(#[from] Error),
    #[error("FeedError")]
    Anyhow(#[from] AnyhowError),
}

pub async fn save_rule(db: &SqlitePool, data: &RuleData) -> Result<i64, SaveError> {
    let (rule_type, text) = match data.match_type.as_str() {
        "word" => (
            format!("{}_{}", data.target, data.match_type),
            data.stem.clone().unwrap_or("".to_string()),
        ),
        "exactword" => (
            format!("{}_{}", data.target, data.match_type),
            data.stem.clone().unwrap_or("".to_string()),
        ),
        "all" => (
            format!("{}_{}", data.target, data.match_type),
            data.stem.clone().unwrap_or("".to_string()),
        ),
        "phrase_lc" => (
            format!("{}_{}", data.target, data.match_type),
            data.kw.clone().unwrap_or("".to_string()),
        ),
        "phrase" => (
            format!("{}_{}", data.target, data.match_type),
            data.kw.clone().unwrap_or("".to_string()),
        ),
        "author" | "tag" => (
            data.match_type.clone(),
            data.kw.clone().unwrap_or("".to_string()),
        ),
        _ => {
            return Err(SaveError::Anyhow(anyhow!(
                "unrecognized rule type: {}",
                data.match_type
            )));
        }
    };
    let uid: Option<i64> = match &data.feed_only {
        None => None,
        Some(s) => match s.as_str() {
            "yes" | "on" | "true" => Some(data.item_uid as i64),
            _ => None,
        },
    };
    info!("recording rule {} {} {}", rule_type, uid.unwrap_or(0), text);
    let row = sqlx::query!(
        r###"
INSERT INTO rule (type, feed, text)
VALUES (?, (SELECT feed FROM item WHERE uid=?), ?)
RETURNING uid
"###,
        rule_type,
        uid,
        text
    )
    .fetch_one(db)
    .await?;
    info!("rule uid = {}", row.uid);
    Ok(row.uid)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RuleStats {
    pub uid: i64,
    pub match_type: String,
    pub text: String,
    pub rule_feed_uid: i64,
    pub feed_title: String,
    pub last_7: i64,
    pub prev_7: i64,
    pub min_uid: i64,
    pub max_uid: i64,
}

pub async fn get_rule_stats(db: &SqlitePool) -> Result<Vec<RuleStats>, FilterError> {
    let rows = sqlx::query_as!(
        RuleStats,
        r###"
SELECT rule.uid AS "uid!", rule.type AS "match_type!", text AS "text!",
   COALESCE(rule.feed, -1) AS rule_feed_uid,
   COALESCE(feed.title, 'Not scoped to a feed') AS feed_title,
   SUM(CASE WHEN item.loaded > julianday('now')-7 THEN 1 ELSE 0 END) last_7,
   SUM(CASE WHEN item.loaded < julianday('now')-7 THEN 1 ELSE 0 END) prev_7,
   MIN(CASE WHEN item.loaded > julianday('now')-7
            THEN item.uid ELSE 2000000000 END) AS min_uid,
   MAX(CASE WHEN item.loaded > julianday('now')-7
            THEN item.uid ELSE 0 END) AS max_uid
 FROM rule
 JOIN item ON item.rule=rule.uid
 LEFT JOIN feed ON rule.feed=feed.uid
 WHERE item.loaded > julianday('now')-14
 GROUP BY 1, 2, 3, 4, 5
 ORDER BY 6 DESC
 LIMIT 100
"###
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}
