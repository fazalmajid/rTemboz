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
use crate::filter::{FilterError, Filters, Rule, new_filters, rule_from_string};
use log::error;
use sqlx::error::Error;
use sqlx::sqlite::SqlitePool;

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
                text: row.text.unwrap_or("Unknown".to_string()),
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
                text: row.text.unwrap_or("Unknown".to_string()),
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
            text: row.text.clone().unwrap_or("Unknown".to_string()),
            feed: row.feed.map(|uid| uid as u32),
            feed_title: row.feed_title.clone(),
        })
        .collect();
    Ok(rules)
}
