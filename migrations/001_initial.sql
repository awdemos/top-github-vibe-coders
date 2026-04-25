CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    github_id INTEGER UNIQUE NOT NULL,
    github_login TEXT UNIQUE NOT NULL,
    avatar_url TEXT,
    is_claw_bot INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS nominees (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    github_login TEXT UNIQUE NOT NULL,
    avatar_url TEXT,
    total_commits_90d INTEGER DEFAULT 0,
    total_loc_90d INTEGER DEFAULT 0,
    repo_count_90d INTEGER DEFAULT 0,
    desloppify_score REAL,
    vibe_score REAL NOT NULL DEFAULT 0,
    nomination_status TEXT DEFAULT 'pending',
    pipeline_report TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS votes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    nominee_id INTEGER NOT NULL REFERENCES nominees(id) ON DELETE CASCADE,
    vote_type TEXT NOT NULL CHECK(vote_type IN ('community', 'claw_bot')),
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_id, nominee_id, vote_type)
);

CREATE TABLE IF NOT EXISTS nomination_prs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pr_number INTEGER UNIQUE NOT NULL,
    github_login TEXT NOT NULL,
    raw_data TEXT NOT NULL,
    pipeline_status TEXT DEFAULT 'pending',
    pipeline_output TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_votes_nominee ON votes(nominee_id, vote_type);
CREATE INDEX IF NOT EXISTS idx_nominees_status ON nominees(nomination_status);
CREATE INDEX IF NOT EXISTS idx_nominees_score ON nominees(vibe_score DESC);
