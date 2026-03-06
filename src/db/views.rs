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
use log::info;
use sqlx::error::Error;
use sqlx::sqlite::SqlitePool;

const DECAY: i32 = 90;

pub async fn rebuild(db: &SqlitePool) -> Result<(), Error> {
    info!("DROP TRIGGER IF EXISTS update_stat_mv");
    let _ = sqlx::query("DROP TRIGGER IF EXISTS update_stat_mv")
        .execute(db)
        .await?;
    info!("DROP TRIGGER IF EXISTS insert_stat_mv");
    let _ = sqlx::query("DROP TRIGGER IF EXISTS insert_stat_mv")
        .execute(db)
        .await?;
    info!("DROP TRIGGER IF EXISTS delete_stat_mv");
    let _ = sqlx::query("DROP TRIGGER IF EXISTS delete_stat_mv")
        .execute(db)
        .await?;
    info!("DROP TRIGGER IF EXISTS insert_feed_mv");
    let _ = sqlx::query("DROP TRIGGER IF EXISTS insert_feed_mv")
        .execute(db)
        .await?;
    info!("DROP TRIGGER IF EXISTS delete_feed_mv");
    let _ = sqlx::query("DROP TRIGGER IF EXISTS delete_feed_mv")
        .execute(db)
        .await?;
    info!("DELETE FROM mv_feed_stats");
    let _ = sqlx::query("DELETE FROM mv_feed_stats").execute(db).await?;
    info!("repopulate mv_feed_stats");
    let res = sqlx::query(
        r###"
INSERT INTO mv_feed_stats
WITH feeds AS (
  SELECT feed.uid,
    SUM(CASE WHEN rating=1 THEN 1 ELSE 0 END)  interesting,
    SUM(CASE WHEN rating=0 THEN 1 ELSE 0 END)  unread,
    SUM(CASE WHEN rating=-1 THEN 1 ELSE 0 END) uninteresting,
    SUM(CASE WHEN rating=-2 THEN 1 ELSE 0 END) filtered,
    SUM(1)                                     total,
    MAX(item_modified)                         latest,
    SUM(CASE WHEN rating > 0 THEN 1.0 ELSE 0 END
        / (1 << min(62, (julianday('now') - published)/?)))
                                                    snr_sig,
    SUM(1.0
        / (1 << min(62, (julianday('now') - published)/?)))
                                                    snr_norm
  FROM feed
  LEFT OUTER JOIN (
    SELECT rating, feed, published,
      IFNULL(
        julianday(modified),
        julianday(published)
      ) AS item_modified
    FROM item
  ) ON feed=feed.uid
  GROUP BY feed.uid, feed.title, html, xml
)
SELECT uid, interesting, unread, uninteresting, filtered, total,
  latest,
  CASE WHEN snr_norm=0 THEN 0.0
       WHEN snr_norm is null or snr_sig is null THEN 0.0
       ELSE snr_sig / snr_norm
  END AS snr
FROM feeds"###,
    )
    .bind(DECAY)
    .bind(DECAY)
    .execute(db)
    .await?;
    info!("rebuilt {} rows in mv_feed_stats", res.rows_affected());
    info!("recreating trigger update_stat_mv");
    let _ = sqlx::query(
        r###"
CREATE TRIGGER update_stat_mv AFTER UPDATE ON item
BEGIN
  UPDATE mv_feed_stats SET
  interesting = interesting
    + CASE NEW.rating WHEN 1 THEN 1 ELSE 0 END
    - CASE OLD.rating WHEN 1 THEN 1 ELSE 0 END,
  unread = unread
    + CASE NEW.rating WHEN 0 THEN 1 ELSE 0 END
    - CASE OLD.rating WHEN 0 THEN 1 ELSE 0 END,
  uninteresting = uninteresting
    + CASE NEW.rating WHEN -1 THEN 1 ELSE 0 END
    - CASE OLD.rating WHEN -1 THEN 1 ELSE 0 END,
  filtered = filtered
    + CASE NEW.rating WHEN -2 THEN 1 ELSE 0 END
    - CASE OLD.rating WHEN -2 THEN 1 ELSE 0 END,
  last_modified = MAX(IFNULL(last_modified, 0), 
                      IFNULL(julianday(NEW.modified),
                             julianday(NEW.published)))
  WHERE mv_feed_stats.feed=NEW.feed;
END"###,
    )
    .execute(db)
    .await?;
    info!("recreating trigger insert_stat_mv");
    let _ = sqlx::query(
        r###"
CREATE TRIGGER insert_stat_mv AFTER INSERT ON item
BEGIN
  UPDATE mv_feed_stats SET
  interesting = interesting
    + CASE NEW.rating WHEN 1 THEN 1 ELSE 0 END,
  unread = unread
    + CASE NEW.rating WHEN 0 THEN 1 ELSE 0 END,
  uninteresting = uninteresting
    + CASE NEW.rating WHEN -1 THEN 1 ELSE 0 END,
  filtered = filtered
    + CASE NEW.rating WHEN -2 THEN 1 ELSE 0 END,
  total = total + 1,
  last_modified = MAX(IFNULL(last_modified, 0), 
                      IFNULL(julianday(NEW.modified),
                             julianday(NEW.published)))
  WHERE mv_feed_stats.feed=NEW.feed;
END"###,
    )
    .execute(db)
    .await?;
    // XXX there is a possibility last_modified will not be updated if we purge
    // XXX the most recent item. There are no use CASEs WHERE this could happen
    // XXX since garbage-collection works from the oldest item, and
    // XXX purge-reload will reload the item anyway
    info!("recreating trigger delete_stat_mv");
    let _ = sqlx::query(
        r###"
CREATE TRIGGER delete_stat_mv AFTER DELETE ON item
BEGIN
  UPDATE mv_feed_stats SET
  interesting = interesting
    - CASE OLD.rating WHEN 1 THEN 1 ELSE 0 END,
  unread = unread
    - CASE OLD.rating WHEN 0 THEN 1 ELSE 0 END,
  uninteresting = uninteresting
    - CASE OLD.rating WHEN -1 THEN 1 ELSE 0 END,
  filtered = filtered
    - CASE OLD.rating WHEN -2 THEN 1 ELSE 0 END,
  total = total - 1
  WHERE mv_feed_stats.feed=OLD.feed;
END"###,
    )
    .execute(db)
    .await?;
    info!("recreating trigger insert_feed_mv");
    let _ = sqlx::query(
        r###"
CREATE TRIGGER insert_feed_mv AFTER INSERT ON feed
BEGIN
  INSERT into mv_feed_stats (feed) VALUES (NEW.uid);
END"###,
    )
    .execute(db)
    .await?;
    info!("recreating trigger delete_feed_mv");
    let _ = sqlx::query(
        r###"
CREATE TRIGGER delete_feed_mv AFTER DELETE ON feed
BEGIN
  DELETE FROM mv_feed_stats
  WHERE feed=OLD.uid;
END"###,
    )
    .execute(db)
    .await?;
    Ok(())
}
