-- -*- mode: sql; sql-product: sqlite -*-

-- whether this is an aggregator like Hacker News or Lobste.rs
ALTER TABLE feed ADD COLUMN aggregator integer DEFAULT 0;

ALTER TABLE item ADD COLUMN normalized_link VARCHAR(255);
CREATE INDEX item_link_i ON item(normalized_link);
CREATE INDEX item_normalized_link_i ON item(normalized_link);

-- sqlx skip
UPDATE item SET normalized_link = normalize_url(link);
CREATE TRIGGER update_normalized_link AFTER INSERT ON item
BEGIN
  UPDATE item SET normalized_link = normalize_url(link)
  WHERE item.uid=NEW.uid AND item.normalized_link IS NULL;
END;
-- end sqlx skip

DROP VIEW v_feeds_snr;
CREATE VIEW v_feeds_snr AS
SELECT uid, title, html, xml, pubxml,
  CAST(julianday('now') - last_modified AS real) AS last_modified,
  ifnull(interesting, 0) AS interesting,
  ifnull(unread, 0) AS unread,
  ifnull(uninteresting, 0) AS uninteresting,
  ifnull(filtered, 0) AS filtered,
  ifnull(total, 0) AS total,
  ifnull(snr, 0.0) AS snr,
  status, private, exempt, dupcheck, errors,
  description, etag, last_fetched, last_parsed, last_error, aggregator
FROM feed
LEFT OUTER JOIN mv_feed_stats m ON uid=m.feed
GROUP BY uid, title, html, xml;

ALTER TABLE item ADD COLUMN parent integer
  REFERENCES item(uid) ON DELETE CASCADE;
CREATE INDEX item_parent_i ON item(parent) WHERE parent IS NOT NULL;

-- recreate the FTS5 index
DROP TABLE search;
DROP TRIGGER fts_ai;
DROP TRIGGER fts_ad;
DROP TRIGGER fts_au;

CREATE VIRTUAL TABLE IF NOT EXISTS search
USING fts5(content="item", title, content, content_rowid=uid);

CREATE TRIGGER fts_ai AFTER INSERT ON item
BEGIN
  INSERT INTO search(rowid, title, content)
  values (NEW.uid, NEW.title, NEW.content);
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

INSERT INTO search(search) VALUES ('rebuild');
