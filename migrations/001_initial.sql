-- -*- mode: sql; sql-product: sqlite -*-

CREATE TABLE feed (
        uid             integer PRIMARY KEY,
        xml             varchar(255) UNIQUE NOT NULL,
        pubxml          varchar(255),
        etag            varchar(255),
        modified        varchar(255),
        html            varchar(255) NOT NULL,
        title           varchar(255) NOT NULL DEFAULT 'Unknown',
        description     text,
        errors          int NOT NULL DEFAULT 0 CHECK(errors >= 0),
        lang            varchar(2) DEFAULT 'en',
        private         int DEFAULT 0,
        dupcheck        int DEFAULT 0,
        oldest          timestamp,
        -- 0=active, 1=suspended
        status          int NOT NULL DEFAULT 0,
        -- 0=hourly, 1=daily, 2=weekly, 3=monthly
        frequency       int DEFAULT 0,
        auth            varchar(255),
        exempt          int NOT NULL DEFAULT 0,
        last_fetched    timestamp,
        last_parsed     timestamp,
        last_error      timestamp,
        error_text      text
);

CREATE TABLE item (
        uid             integer PRIMARY KEY,
        guid            varchar(255) NOT NULL,
        feed            integer NOT NULL
        REFERENCES feed (uid) ON DELETE CASCADE,
        loaded          timestamp NOT NULL,
        published       timestamp,
        modified        timestamp,
        rated           timestamp,
        link            varchar(255),
        title           text NOT NULL DEFAULT 'Untitled',
        content         text,
        creator         varchar(255),
        -- 1=interesting, 0=unread, -1=uninteresting, -2=filtered
        rating          integer NOT NULL DEFAULT 0
        CHECK (rating BETWEEN -2 AND +1),
        rule            integer NULL
        REFERENCES rule (uid) ON DELETE CASCADE,
        CHECK (rule IS NULL OR rating=-2 AND rule>0)
);

CREATE TRIGGER update_timestamp AFTER INSERT ON item
BEGIN
        UPDATE item SET
          loaded = COALESCE(NEW.loaded, julianday('now')),
          -- sqlx stores datetimes in ISO format, not as julianday
          published = MIN(NEW.published, strftime('%Y-%m-%dT%H:%M:%S', 'now') || '+00:00')
        WHERE item.uid=NEW.uid;
END;

-- sometimes an article is filtered, and then manually upped, so in that case
-- we should remove the filtered by rule
CREATE TRIGGER clear_rule AFTER UPDATE ON item
FOR EACH ROW WHEN NEW.rating<>-2 AND OLD.rule IS NOT NULL
BEGIN
        UPDATE item set rule = NULL
        WHERE item.uid=new.uid;
END;

CREATE INDEX item_feed_link_i ON item(feed, link);
CREATE UNIQUE index item_feed_guid_i ON item(feed, guid);
CREATE INDEX item_rating_i ON item(rating, feed);
CREATE INDEX item_title_i ON item(feed, title);

CREATE TABLE rule (
        uid             integer PRIMARY KEY,
        type            varchar(16) NOT NULL DEFAULT 'python',
        feed            integer
        REFERENCES feed (uid) ON DELETE CASCADE,
        text            text
);

CREATE TABLE tag (
        name            varchar(64) NOT NULL,
        item            integer NOT NULL
        REFERENCES item (uid) ON DELETE CASCADE,
        -- 0=by the feed, 1=by the user, 2=by an algorithm
        by              integer DEFAULT 0
        CHECK (by BETWEEN 0 AND 2),
        PRIMARY KEY(item, name, by)
);
CREATE INDEX tag_name_i ON tag (name);

CREATE TABLE setting (
        name            varchar(255) PRIMARY KEY,
        value           text NOT NULL
);

CREATE VIEW top20 AS
  SELECT
    feed.title,
    round(100*interesting/(interesting+uninteresting)) AS interest_ratio
  FROM (
    SELECT
      feed.title,
      sum(CASE WHEN item.rating=1 THEN 1 ELSE 0 END) AS interesting,
      sum(CASE WHEN item.rating=-1 THEN 1 ELSE 0 END) AS uninteresting
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
    sum(CASE WHEN item.rating>0 THEN 1 ELSE 0 END) AS interesting,
    sum(CASE WHEN item.rating=-2 THEN 1 ELSE 0 END) AS filtered,
    sum(CASE WHEN item.rating=-1 THEN 1 ELSE 0 END) AS uninteresting
  FROM feed, item
  WHERE item.feed=feed.uid
  GROUP BY day
  ORDER BY day;

CREATE TABLE mv_feed_stats (
  feed integer PRIMARY KEY,
  interesting integer DEFAULT 0,
  unread integer DEFAULT 0,
  uninteresting integer DEFAULT 0,
  filtered integer DEFAULT 0,
  total integer DEFAULT 0,
  last_modified timestamp,
  snr real DEFAULT 0.0
);

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
  description, etag, last_fetched, last_parsed, last_error
FROM feed
LEFT OUTER JOIN mv_feed_stats m ON uid=m.feed
GROUP BY uid, title, html, xml;

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

CREATE TRIGGER fts_au AFTER UPDATE ON item
BEGIN
  INSERT INTO search(search, rowid, title, content)
  VALUES ('delete', OLD.uid, OLD.title, OLD.content);
  INSERT INTO SEARCH(rowid, title, content)
  VALUES (NEW.uid, NEW.title, NEW.content);
END;

INSERT INTO search(search) VALUES ('rebuild');
