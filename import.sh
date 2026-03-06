#!/bin/sh
sqlite3 temboz.db <<EOF
ATTACH 'rss.db' as temboz;

INSERT INTO feed (
  uid, xml, pubxml, etag, modified, html, title, description, errors, lang,
  private, dupcheck, oldest, status, frequency, auth, exempt
)
SELECT
  feed_uid, feed_xml, feed_pubxml, feed_etag, feed_modified, feed_html,
  feed_title, feed_desc, feed_errors, feed_lang, feed_private, feed_dupcheck,
  feed_oldest, feed_status, feed_frequency, feed_auth, feed_exempt
FROM temboz.fm_feeds;

INSERT INTO item (
  uid, guid, feed, loaded, published, modified, rated, link, title,
  content, creator, rating, rule
)
SELECT
  item_uid, item_guid, item_feed_uid, item_loaded, item_created, item_modified,
  item_rated, item_link, item_title, item_content, item_creator,
  CASE WHEN item_rating=-2 AND item_rule_uid=0 THEN -1 ELSE item_rating END,
  CASE WHEN item_rule_uid=0 OR item_rating<>-2 THEN NULL
       WHEN item_rule_uid < 0 THEN -item_rule_uid
       ELSE item_rule_uid END
FROM temboz.fm_items;

INSERT INTO rule (
  uid, type, feed, text
)
SELECT
  rule_uid, rule_type, rule_feed_uid, rule_text
FROM temboz.fm_rules
WHERE rule_expires IS NULL;

INSERT INTO tag (
  name, item, by
)
SELECT tag_name, tag_item_uid, tag_by
FROM temboz.fm_tags;

INSERT INTO setting (
  name, value
)
SELECT name, value
FROM temboz.fm_settings;

EOF
