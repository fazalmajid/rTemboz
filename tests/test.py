"""
End-to-end tests for rtemboz
"""

import http.server
import os
import signal
import sqlite3
import subprocess
import threading
import time

import pytest
import requests

# Paths
PROJECT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUGFEED_DIR = os.path.join(PROJECT_DIR, "bugfeed")
BINARY = os.path.join(PROJECT_DIR, "target", "debug", "rtemboz")

# Ports
FEED_SERVER_PORT = 18080
RTEMBOZ_PORT = 9998

# Test working directory (so temboz.db is created here, not in project root)
TEST_WORK_DIR = os.path.join(PROJECT_DIR, "tests")


class FeedHandler(http.server.SimpleHTTPRequestHandler):
    """HTTP handler that serves bugfeed files.
    Uses HTTP/1.1 with Connection: close so reqwest doesn't hang
    waiting for more data on a keep-alive connection."""

    protocol_version = "HTTP/1.1"

    def __init__(self, *args, directory=None, **kwargs):
        super().__init__(*args, directory=directory, **kwargs)

    def end_headers(self):
        self.send_header("Connection", "close")
        super().end_headers()

    def guess_type(self, path):
        """Treat extensionless files as XML (RSS feeds)."""
        if "." not in os.path.basename(path):
            return "application/rss+xml"
        return super().guess_type(path)


class FeedServer:
    """Simple HTTP server to serve bugfeed files."""

    def __init__(self, port, directory):
        self.port = port
        self.directory = directory
        self.httpd = None
        self.thread = None

    def start(self):
        handler = lambda *args, **kwargs: FeedHandler(
            *args, directory=self.directory, **kwargs
        )
        self.httpd = http.server.HTTPServer(("127.0.0.1", self.port), handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    def stop(self):
        if self.httpd:
            self.httpd.shutdown()


def wait_for_port(port, host="127.0.0.1", timeout=30):
    """Wait for a TCP port to become available."""
    import socket

    start = time.time()
    while time.time() - start < timeout:
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(1)
            sock.connect((host, port))
            sock.close()
            return True
        except (ConnectionRefusedError, OSError):
            time.sleep(0.5)
    return False


def insert_auth_settings(conn):
    """Insert test authentication credentials into the setting table."""
    conn.execute("INSERT INTO setting (name, value) VALUES (?, ?)", ("login", "test"))
    conn.execute(
        "INSERT INTO setting (name, value) VALUES (?, ?)",
        ("passwd", "$argon2i$v=19$m=65536,t=3,p=4$eDWHxN3NBDVqtLkQiYMg2g$1tm/H4ZA0WsfZXkVRVbktpT/RRiA93XKeHbsw2wuBsw"),
    )
    conn.commit()


def init_db(work_dir, db_path, feed_url, env):
    """Create a fresh database using rtemboz rebuild, add a feed, and return the connection."""
    if os.path.exists(db_path):
        os.remove(db_path)
    # Let rtemboz create and migrate the database
    result = subprocess.run(
        [BINARY, "rebuild"],
        cwd=work_dir,
        env=env,
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode == 0, (
        f"rtemboz rebuild failed:\nstdout: {result.stdout}\nstderr: {result.stderr}"
    )
    conn = sqlite3.connect(db_path)
    insert_auth_settings(conn)
    # Insert the test feed
    conn.execute(
        "INSERT INTO feed (xml, html, title, status) VALUES (?, ?, ?, 0)",
        (feed_url, "https://blog.majid.info/", "Fazal's blog"),
    )
    conn.commit()
    return conn


@pytest.fixture(scope="session")
def feed_server():
    """Start an HTTP server serving the bugfeed directory."""
    server = FeedServer(FEED_SERVER_PORT, BUGFEED_DIR)
    server.start()
    assert wait_for_port(FEED_SERVER_PORT, timeout=5), "Feed server failed to start"
    yield server
    server.stop()


@pytest.fixture(scope="session")
def build_binary():
    """Ensure the binary is built."""
    env = os.environ.copy()
    env["OPENSSL_DIR"] = os.path.join(os.environ["HOME"], "local", "ssl")
    subprocess.run(
        ["cargo", "build"],
        cwd=PROJECT_DIR,
        env=env,
        check=True,
        capture_output=True,
    )


@pytest.fixture()
def test_env(feed_server, build_binary):
    """Set up a clean test environment with a fresh database in tests/."""
    work_dir = TEST_WORK_DIR
    feed_url = f"http://127.0.0.1:{FEED_SERVER_PORT}/fazal"
    db_path = os.path.join(work_dir, "temboz.db")

    env = os.environ.copy()
    env["OPENSSL_DIR"] = os.path.join(os.environ["HOME"], "local", "ssl")

    # Remove any leftover DB from a previous run
    if os.path.exists(db_path):
        os.remove(db_path)

    conn = init_db(work_dir, db_path, feed_url, env)

    yield {
        "work_dir": work_dir,
        "db_path": db_path,
        "conn": conn,
        "feed_url": feed_url,
        "env": env,
    }

    conn.close()
    # Clean up
    if os.path.exists(db_path):
        os.remove(db_path)


def run_refresh(test_env):
    """Run rtemboz refresh and wait for it to complete."""
    result = subprocess.run(
        [BINARY, "refresh"],
        cwd=test_env["work_dir"],
        env=test_env["env"],
        capture_output=True,
        text=True,
        timeout=60,
    )
    print(f"refresh stdout: {result.stdout}")
    print(f"refresh stderr: {result.stderr}")
    return result


def start_serve(test_env):
    """Start rtemboz serve in the background and wait for it to be ready."""
    proc = subprocess.Popen(
        [BINARY, "serve"],
        cwd=test_env["work_dir"],
        env=test_env["env"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert wait_for_port(RTEMBOZ_PORT, timeout=30), "rtemboz server failed to start"
    return proc


def stop_serve(proc):
    """Stop the rtemboz server."""
    proc.send_signal(signal.SIGTERM)
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()


@pytest.fixture()
def serve_env(feed_server, build_binary):
    """Set up a clean test environment with a running rtemboz server (no pre-inserted feed)."""
    work_dir = TEST_WORK_DIR
    db_path = os.path.join(work_dir, "temboz.db")

    env = os.environ.copy()
    env["OPENSSL_DIR"] = os.path.join(os.environ["HOME"], "local", "ssl")

    # Remove any leftover DB from a previous run
    if os.path.exists(db_path):
        os.remove(db_path)

    # Let rtemboz create and migrate the database
    result = subprocess.run(
        [BINARY, "rebuild"],
        cwd=work_dir,
        env=env,
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode == 0, (
        f"rtemboz rebuild failed:\nstdout: {result.stdout}\nstderr: {result.stderr}"
    )

    # Insert auth credentials before starting server
    with sqlite3.connect(db_path) as setup_conn:
        insert_auth_settings(setup_conn)

    # Start the server
    proc = subprocess.Popen(
        [BINARY, "serve"],
        cwd=work_dir,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert wait_for_port(RTEMBOZ_PORT, timeout=30), "rtemboz server failed to start"

    conn = sqlite3.connect(db_path)

    yield {
        "work_dir": work_dir,
        "db_path": db_path,
        "conn": conn,
        "env": env,
        "proc": proc,
    }

    conn.close()
    stop_serve(proc)
    if os.path.exists(db_path):
        os.remove(db_path)


def login_session(base_url):
    """Create a requests.Session with a valid auth cookie."""
    session = requests.Session()
    resp = session.post(
        f"{base_url}/login",
        data={"login": "test", "password": "test"},
        timeout=10,
        allow_redirects=False,
    )
    assert resp.status_code in (200, 302), f"Login failed with status {resp.status_code}"
    return session


class TestAddFeed:
    """Test adding a feed via the /add endpoint."""

    def test_add_feed_via_web(self, serve_env):
        conn = serve_env["conn"]
        feed_url = f"http://127.0.0.1:{FEED_SERVER_PORT}/fazal"
        base_url = f"http://127.0.0.1:{RTEMBOZ_PORT}"

        session = login_session(base_url)

        # POST to /add with the feed URL
        resp = session.post(
            f"{base_url}/add",
            data={"feed_xml": feed_url},
            timeout=60,
        )
        assert resp.status_code == 200, (
            f"/add returned status {resp.status_code} (base_url={base_url}): {resp.text[:500]}"
        )

        # Verify the feed was inserted into the database
        row = conn.execute("SELECT uid, title, xml FROM feed").fetchone()
        assert row is not None, "Feed was not inserted into the database"
        feed_uid, feed_title, feed_xml = row
        assert feed_xml == feed_url
        assert feed_title is not None and len(feed_title) > 0

        # Verify that items were loaded
        item_count = conn.execute(
            "SELECT COUNT(*) FROM item WHERE feed=?", (feed_uid,)
        ).fetchone()[0]
        assert item_count > 0, (
            f"Expected items to be loaded for feed {feed_uid}, got {item_count}"
        )


class TestFilterUnionAll:
    """Test that a UnionAll rule for 'milk chocolate' filters the Dardenne article."""

    def test_union_all_milk_chocolate(self, test_env):
        conn = test_env["conn"]

        # Create a UnionAll filtering rule for "milk chocolate"
        conn.execute(
            "INSERT INTO rule (type, text) VALUES (?, ?)",
            ("union_all", "milk chocolate"),
        )
        conn.commit()

        # Run refresh to fetch the feed and apply filters
        result = run_refresh(test_env)
        assert result.returncode == 0, (
            f"refresh failed:\nstdout: {result.stdout}\nstderr: {result.stderr}"
        )

        # Verify the Dardenne article was fetched
        row = conn.execute(
            "SELECT uid, title, rating, rule FROM item WHERE title LIKE '%Dardenne%'"
        ).fetchone()
        assert row is not None, "Dardenne article was not fetched"

        item_uid, title, rating, rule_uid = row
        assert "Dardenne" in title
        # rating=-2 means filtered
        assert rating == -2, (
            f"Expected rating=-2 (filtered), got {rating} for '{title}'"
        )
        # Should be linked to the rule
        assert rule_uid is not None, "Expected item to be linked to a filtering rule"

        # Verify non-matching articles are NOT filtered
        non_chocolate = conn.execute(
            "SELECT title, rating FROM item WHERE title NOT LIKE '%chocolate%' AND title NOT LIKE '%Chocolate%'"
        ).fetchall()
        assert len(non_chocolate) > 0, "Expected some non-chocolate articles"
        for title, rating in non_chocolate:
            assert rating != -2, (
                f"Article '{title}' should not be filtered by 'milk chocolate' rule"
            )

    def test_filtered_visible_in_web_ui(self, test_env):
        conn = test_env["conn"]

        # Create rule and refresh
        conn.execute(
            "INSERT INTO rule (type, text) VALUES (?, ?)",
            ("union_all", "milk chocolate"),
        )
        conn.commit()

        result = run_refresh(test_env)
        assert result.returncode == 0, (
            f"refresh failed:\nstdout: {result.stdout}\nstderr: {result.stderr}"
        )

        # Rebuild materialized views so the web UI has correct stats
        rebuild_result = subprocess.run(
            [BINARY, "rebuild"],
            cwd=test_env["work_dir"],
            env=test_env["env"],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert rebuild_result.returncode == 0

        # Start the web server
        proc = start_serve(test_env)
        try:
            base_url = f"http://127.0.0.1:{RTEMBOZ_PORT}"
            session = login_session(base_url)

            # Check filtered view
            resp = session.get(
                f"{base_url}/view",
                params={"show": "filtered"},
            )
            assert resp.status_code == 200
            assert "Dardenne" in resp.text, (
                "Dardenne article should appear in filtered view"
            )

            # Check unread view should NOT contain the Dardenne article
            resp = session.get(
                f"{base_url}/view",
                params={"show": "unread"},
            )
            assert resp.status_code == 200
            assert "Dardenne" not in resp.text, (
                "Dardenne article should NOT appear in unread view"
            )
        finally:
            stop_serve(proc)


class TestRuleAdd:
    """Test adding rules via POST /rule/add for all match_type and target combinations."""

    BASE_URL = f"http://127.0.0.1:{RTEMBOZ_PORT}"

    # match types that derive their text from the `stem` field
    STEM_MATCH_TYPES = ["word", "exactword", "all"]
    # match types that derive their text from the `kw` field
    KW_MATCH_TYPES = ["phrase_lc", "phrase"]
    # match types that ignore target entirely
    GLOBAL_MATCH_TYPES = ["author", "tag"]
    TARGETS = ["title", "union", "content"]

    def _post_rule(self, session, **fields):
        resp = session.post(
            f"{self.BASE_URL}/rule/add",
            data=fields,
            timeout=10,
        )
        assert resp.status_code == 200, (
            f"/rule/add returned {resp.status_code}: {resp.text[:500]}"
        )
        assert resp.json().get("status") == "ok", (
            f"Unexpected response body: {resp.text[:500]}"
        )

    def _rule_exists(self, conn, rule_type, text):
        row = conn.execute(
            "SELECT uid FROM rule WHERE type=? AND text=?", (rule_type, text)
        ).fetchone()
        return row is not None

    @pytest.mark.parametrize("match_type", STEM_MATCH_TYPES)
    @pytest.mark.parametrize("target", TARGETS)
    def test_stem_match_types(self, serve_env, match_type, target):
        """word/exactword/all rules use the stem field and combine with target."""
        conn = serve_env["conn"]
        session = login_session(self.BASE_URL)
        stem = f"pytest {match_type} {target}"
        self._post_rule(
            session,
            kw="ignored kw",
            stem=stem,
            match_type=match_type,
            target=target,
            item_uid=0,
        )
        expected_type = f"{target}_{match_type}"
        assert self._rule_exists(conn, expected_type, stem), (
            f"Rule type={expected_type!r} text={stem!r} not found in DB"
        )

    @pytest.mark.parametrize("match_type", KW_MATCH_TYPES)
    @pytest.mark.parametrize("target", TARGETS)
    def test_kw_match_types(self, serve_env, match_type, target):
        """phrase_lc/phrase rules use the kw field and combine with target."""
        conn = serve_env["conn"]
        session = login_session(self.BASE_URL)
        kw = f"pytest {match_type} {target}"
        self._post_rule(
            session,
            kw=kw,
            stem="ignored stem",
            match_type=match_type,
            target=target,
            item_uid=0,
        )
        expected_type = f"{target}_{match_type}"
        assert self._rule_exists(conn, expected_type, kw), (
            f"Rule type={expected_type!r} text={kw!r} not found in DB"
        )

    @pytest.mark.parametrize("match_type", GLOBAL_MATCH_TYPES)
    def test_global_match_types(self, serve_env, match_type):
        """author/tag rules use the kw field and ignore target."""
        conn = serve_env["conn"]
        session = login_session(self.BASE_URL)
        kw = f"pytest {match_type}"
        self._post_rule(
            session,
            kw=kw,
            stem="ignored stem",
            match_type=match_type,
            target="title",
            item_uid=0,
        )
        assert self._rule_exists(conn, match_type, kw), (
            f"Rule type={match_type!r} text={kw!r} not found in DB"
        )

    def test_feed_only(self, serve_env):
        """feed_only=yes scopes the rule to the feed of the given item."""
        conn = serve_env["conn"]
        session = login_session(self.BASE_URL)

        # Insert a minimal feed and item so item_uid resolves to a feed
        conn.execute(
            "INSERT INTO feed (uid, xml, html, title, status) VALUES (999, 'http://example.com/feed', 'http://example.com', 'Test Feed', 0)"
        )
        conn.execute(
            "INSERT INTO item (uid, feed, title, link, guid, content, rating, loaded) "
            "VALUES (888, 999, 'Test Item', 'http://example.com/1', 'guid-888', 'content', 0, strftime('%s','now'))"
        )
        conn.commit()

        kw = "pytest feed_only"
        self._post_rule(
            session,
            kw=kw,
            stem="",
            match_type="phrase_lc",
            target="title",
            feed_only="yes",
            item_uid=888,
        )

        row = conn.execute(
            "SELECT type, text, feed FROM rule WHERE text=?", (kw,)
        ).fetchone()
        assert row is not None, f"Rule with text={kw!r} not found in DB"
        rule_type, text, feed = row
        assert rule_type == "title_phrase_lc"
        assert feed == 999, f"Expected feed=999 (feed_only), got {feed}"

    def test_feed_only_unchecked(self, serve_env):
        """Omitting feed_only creates a global rule (feed=NULL)."""
        conn = serve_env["conn"]
        session = login_session(self.BASE_URL)

        kw = "pytest feed_only_off"
        self._post_rule(
            session,
            kw=kw,
            stem="",
            match_type="phrase_lc",
            target="title",
            item_uid=0,
        )

        row = conn.execute(
            "SELECT type, text, feed FROM rule WHERE text=?", (kw,)
        ).fetchone()
        assert row is not None, f"Rule with text={kw!r} not found in DB"
        _, _, feed = row
        assert feed is None, f"Expected feed=NULL for global rule, got {feed}"
