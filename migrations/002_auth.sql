-- -*- mode: sql; sql-product: sqlite -*-

CREATE TABLE session (
        uuid            text PRIMARY KEY,
        login           text NOT NULL,
        user_agent      text NOT NULL,
        created         timestamp default (julianday('now')),
        expires         timestamp default (julianday('now') + 14)
);
