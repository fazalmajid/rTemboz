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


class TestAddFeed:
    """Test adding a feed via the /add endpoint."""

    def test_add_feed_via_web(self, serve_env):
        conn = serve_env["conn"]
        feed_url = f"http://127.0.0.1:{FEED_SERVER_PORT}/fazal"

        # POST to /add with the feed URL
        resp = requests.post(
            f"http://127.0.0.1:{RTEMBOZ_PORT}/add",
            data={"feed_xml": feed_url},
            timeout=60,
        )
        assert resp.status_code == 200, (
            f"/add returned status {resp.status_code}: {resp.text[:500]}"
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
            # Check filtered view
            resp = requests.get(
                f"http://127.0.0.1:{RTEMBOZ_PORT}/view",
                params={"show": "filtered"},
            )
            assert resp.status_code == 200
            assert "Dardenne" in resp.text, (
                "Dardenne article should appear in filtered view"
            )

            # Check unread view should NOT contain the Dardenne article
            resp = requests.get(
                f"http://127.0.0.1:{RTEMBOZ_PORT}/view",
                params={"show": "unread"},
            )
            assert resp.status_code == 200
            assert "Dardenne" not in resp.text, (
                "Dardenne article should NOT appear in unread view"
            )
        finally:
            stop_serve(proc)
