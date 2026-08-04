-- no-transaction
-- -*- mode: sql; sql-product: sqlite -*-
--
-- Tighten the data model:
-- * PRIMARY KEYs are not NOT NULL by default in SQLite, fix this.
-- * enable foreign key verification
-- * recreate tables in STRICT mode
-- * use SQLite's REAL Julian day timestamp convention consistently, sqlx
--   was saving NaiveDateTime as TEXT for item published/modified timestamps

PRAGMA foreign_keys=OFF;

BEGIN TRANSACTION;

-- drop the views, we will recreate them after the tables are reorganized
DROP VIEW top20;
DROP VIEW daily_stats;
DROP VIEW v_feeds_snr;

-- feed --------------------------------------------------------------

CREATE TABLE new_feed (
  uid             INTEGER PRIMARY KEY NOT NULL,
  xml             TEXT UNIQUE NOT NULL,
  pubxml          TEXT,
  etag            TEXT,
  modified        REAL,
  html            TEXT NOT NULL,
  title           TEXT NOT NULL DEFAULT 'Unknown',
  description     TEXT,
  errors          INTEGER NOT NULL DEFAULT 0 CHECK(errors >= 0),
  lang            TEXT DEFAULT 'en',
  private         INTEGER DEFAULT 0,
  dupcheck        INTEGER DEFAULT 0,
  oldest          REAL,
  -- 0=active, 1=suspended
  status          INTEGER NOT NULL DEFAULT 0,
  -- 0=hourly, 1=daily, 2=weekly, 3=monthly
  frequency       INTEGER DEFAULT 0,
  auth            TEXT,
  exempt          INTEGER NOT NULL DEFAULT 0,
  last_fetched    REAL,
  last_parsed     REAL,
  last_error      REAL,
  error_text      TEXT,
  aggregator      INTEGER DEFAULT 0
) STRICT;

INSERT INTO new_feed
SELECT
  uid, xml, pubxml, etag,
  CAST(modified AS REAL),
  html, title, description, errors, lang, private, dupcheck,
  CAST(oldest AS REAL),
  status, frequency, auth, exempt, last_fetched, last_parsed, last_error,
  error_text, aggregator
FROM feed;

DROP TABLE feed;
ALTER TABLE new_feed RENAME TO feed;

-- rule --------------------------------------------------------------

CREATE TABLE new_rule (
  uid             INTEGER PRIMARY KEY NOT NULL,
  type            TEXT NOT NULL DEFAULT 'python',
  feed            INTEGER
  REFERENCES feed (uid) ON DELETE CASCADE,
  text            TEXT NOT NULL
) STRICT;

INSERT INTO new_rule SELECT uid, type, feed, text FROM rule;

DROP TABLE rule;
ALTER TABLE new_rule RENAME TO rule;

-- item --------------------------------------------------------------

CREATE TABLE new_item (
  uid             INTEGER PRIMARY KEY NOT NULL,
  guid            TEXT NOT NULL,
  feed            INTEGER NOT NULL
  REFERENCES feed (uid) ON DELETE CASCADE,
  loaded          REAL NOT NULL,
  published       REAL,
  modified        REAL,
  rated           REAL,
  link            TEXT,
  title           TEXT NOT NULL DEFAULT 'Untitled',
  content         TEXT,
  creator         TEXT,
  -- 1=interesting, 0=unread, -1=uninteresting, -2=filtered
  rating          INTEGER NOT NULL DEFAULT 0
  CHECK (rating BETWEEN -2 AND +1),
  rule            INTEGER
  REFERENCES rule (uid) ON DELETE CASCADE,
  normalized_link TEXT,
  parent          INTEGER
  REFERENCES item(uid) ON DELETE CASCADE,
  CHECK (rule IS NULL OR rating=-2 AND rule>0)
) STRICT;

-- for those who migrated data from the old (Python) Temboz,
-- item.rule can point at a rule.uid that no longer exists (rules
-- deleted, so we convert them into uninteresting (-1) items instead
INSERT INTO new_item
SELECT
  item.uid, item.guid, item.feed,
  CAST(item.loaded AS REAL),
  julianday(item.published),
  julianday(item.modified),
  CAST(item.rated AS REAL),
  item.link, item.title, item.content, item.creator,
  CASE WHEN item.rule IS NOT NULL AND rule.uid IS NULL
       THEN -1 ELSE item.rating END,
  CASE WHEN item.rule IS NOT NULL AND rule.uid IS NULL
       THEN NULL ELSE item.rule END,
  item.normalized_link, item.parent
FROM item
LEFT JOIN rule ON rule.uid = item.rule;

DROP TABLE item;
ALTER TABLE new_item RENAME TO item;

CREATE INDEX item_feed_link_i ON item(feed, link);
CREATE UNIQUE INDEX item_feed_guid_i ON item(feed, guid);
CREATE INDEX item_rating_i ON item(rating, feed);
CREATE INDEX item_title_i ON item(feed, title);
CREATE INDEX item_link_i ON item(normalized_link);
CREATE INDEX item_normalized_link_i ON item(normalized_link);
CREATE INDEX item_parent_i ON item(parent) WHERE parent IS NOT NULL;

CREATE TRIGGER update_timestamp AFTER INSERT ON item
BEGIN
  UPDATE item SET
    loaded = COALESCE(NEW.loaded, julianday('now')),
    published = MIN(NEW.published, julianday('now'))
  WHERE item.uid=NEW.uid;
END;

CREATE TRIGGER clear_rule AFTER UPDATE ON item
FOR EACH ROW WHEN NEW.rating<>-2 AND OLD.rule IS NOT NULL
BEGIN
  UPDATE item set rule = NULL
  WHERE item.uid=new.uid;
END;

CREATE TRIGGER fts_ai AFTER INSERT ON item
BEGIN
  INSERT INTO search(rowid, title, content)
  VALUES (NEW.uid, NEW.title, NEW.content);
END;

CREATE TRIGGER fts_ad AFTER DELETE ON item
BEGIN
  INSERT INTO search(search, rowid, title, content)
  VALUES ('delete', OLD.uid, OLD.title, OLD.content);
END;

CREATE TRIGGER fts_au AFTER UPDATE OF title, content ON item
BEGIN
  INSERT INTO search(search, rowid, title, content)
  VALUES ('delete', OLD.uid, OLD.title, OLD.content);
  INSERT INTO SEARCH(rowid, title, content)
  VALUES (NEW.uid, NEW.title, NEW.content);
END;

-- tag ---------------------------------------------------------------

CREATE TABLE new_tag (
  name            TEXT NOT NULL,
  item            INTEGER NOT NULL
  REFERENCES item (uid) ON DELETE CASCADE,
  -- 0=by the feed, 1=by the user, 2=by an algorithm
  by              INTEGER DEFAULT 0
  CHECK (by BETWEEN 0 AND 2) NOT NULL,
  PRIMARY KEY(item, name, by)
) STRICT;

INSERT INTO new_tag SELECT name, item, by FROM tag;

DROP TABLE tag;
ALTER TABLE new_tag RENAME TO tag;

CREATE INDEX tag_name_i ON tag (name);

-- setting -----------------------------------------------------------

CREATE TABLE new_setting (
        name            TEXT PRIMARY KEY NOT NULL,
        value           TEXT NOT NULL
) STRICT;

INSERT INTO new_setting SELECT name, value FROM setting;

DROP TABLE setting;
ALTER TABLE new_setting RENAME TO setting;

-- recreate the views dropped above, verbatim -----------------------

CREATE VIEW top20 AS
  SELECT
    feed.title,
    ROUND(100*interesting/(interesting+uninteresting)) AS interest_ratio
  FROM (
    SELECT
      feed.title,
      SUM(CASE WHEN item.rating=1 THEN 1 ELSE 0 END) AS interesting,
      SUM(CASE WHEN item.rating=-1 THEN 1 ELSE 0 END) AS uninteresting
    FROM feed, item
    WHERE item.feed=feed.uid
    GROUP BY feed.title
    ORDER BY feed.title
  )
ORDER BY interest_ratio DESC
limit 20;

CREATE VIEW daily_stats AS
  SELECT
    date(item.published) AS day,
    SUM(CASE WHEN item.rating>0 THEN 1 ELSE 0 END) AS interesting,
    SUM(CASE WHEN item.rating=-2 THEN 1 ELSE 0 END) AS filtered,
    SUM(CASE WHEN item.rating=-1 THEN 1 ELSE 0 END) AS uninteresting
  FROM feed, item
  WHERE item.feed=feed.uid
  GROUP BY day
  ORDER BY day;

CREATE VIEW v_feeds_snr AS
SELECT uid, title, html, xml, pubxml,
  CAST(julianday('now') - last_modified AS real) AS last_modified,
  IFNULL(interesting, 0) AS interesting,
  IFNULL(unread, 0) AS unread,
  IFNULL(uninteresting, 0) AS uninteresting,
  IFNULL(filtered, 0) AS filtered,
  IFNULL(total, 0) AS total,
  IFNULL(snr, 0.0) AS snr,
  status, private, exempt, dupcheck, errors,
  description, etag, last_fetched, last_parsed, last_error, aggregator
FROM feed
LEFT OUTER JOIN mv_feed_stats m ON uid=m.feed
GROUP BY uid, title, html, xml;

-- verify no dangling references were introduced by the rebuild;
-- when run via the sqlite3 CLI (as in `make boot.db`), any violation
-- rows print here. sqlx::migrate! discards the result set, so this
-- is a belt-and-suspenders check for the CLI path, not a hard gate.
PRAGMA foreign_key_check;

COMMIT;

PRAGMA foreign_keys=ON;

-- VACUUM can't run inside a transaction
VACUUM;
