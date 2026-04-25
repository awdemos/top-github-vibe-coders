use libsql::Connection;
use crate::models::{Nominee, User};

pub async fn init_db(conn: &Connection) -> anyhow::Result<()> {
    let sql = include_str!("../migrations/001_initial.sql");
    conn.execute(sql, ()).await?;
    Ok(())
}

pub async fn get_nominees(conn: &Connection, status: &str) -> anyhow::Result<Vec<Nominee>> {
    let mut rows = conn
        .query(
            "SELECT id, github_login, avatar_url, total_commits_90d, total_loc_90d, 
                    repo_count_90d, desloppify_score, vibe_score, nomination_status, 
                    pipeline_report, created_at 
             FROM nominees 
             WHERE nomination_status = ?
             ORDER BY vibe_score DESC",
            [status],
        )
        .await?;

    let mut nominees = Vec::new();
    while let Some(row) = rows.next().await? {
        nominees.push(Nominee {
            id: row.get(0)?,
            github_login: row.get(1)?,
            avatar_url: row.get(2)?,
            total_commits_90d: row.get(3)?,
            total_loc_90d: row.get(4)?,
            repo_count_90d: row.get(5)?,
            desloppify_score: row.get(6)?,
            vibe_score: row.get(7)?,
            nomination_status: row.get(8)?,
            pipeline_report: row.get(9)?,
            created_at: row.get(10)?,
        });
    }
    Ok(nominees)
}

pub async fn get_or_create_user(
    conn: &Connection,
    github_id: i64,
    github_login: &str,
    avatar_url: Option<&str>,
    is_claw_bot: bool,
) -> anyhow::Result<User> {
    let mut rows = conn
        .query(
            "SELECT id, github_id, github_login, avatar_url, is_claw_bot, created_at 
             FROM users WHERE github_id = ?",
            [github_id],
        )
        .await?;

    if let Some(row) = rows.next().await? {
        return Ok(User {
            id: row.get(0)?,
            github_id: row.get(1)?,
            github_login: row.get(2)?,
            avatar_url: row.get(3)?,
            is_claw_bot: row.get::<i64>(4)? != 0,
            created_at: row.get(5)?,
        });
    }

    conn.execute(
        "INSERT OR IGNORE INTO users (github_id, github_login, avatar_url, is_claw_bot) VALUES (?, ?, ?, ?)",
        (github_id, github_login, avatar_url, is_claw_bot as i64),
    )
    .await?;

    let mut rows = conn
        .query(
            "SELECT id, github_id, github_login, avatar_url, is_claw_bot, created_at 
             FROM users WHERE github_id = ?",
            [github_id],
        )
        .await?;

    let row = rows.next().await?.unwrap();
    Ok(User {
        id: row.get(0)?,
        github_id: row.get(1)?,
        github_login: row.get(2)?,
        avatar_url: row.get(3)?,
        is_claw_bot: row.get::<i64>(4)? != 0,
        created_at: row.get(5)?,
    })
}

pub async fn get_vote_counts(conn: &Connection, nominee_id: i64) -> anyhow::Result<(i64, i64)> {
    let mut rows = conn
        .query(
            "SELECT vote_type, COUNT(*) FROM votes WHERE nominee_id = ? GROUP BY vote_type",
            [nominee_id],
        )
        .await?;

    let mut community = 0i64;
    let mut claw = 0i64;

    while let Some(row) = rows.next().await? {
        let vote_type: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        match vote_type.as_str() {
            "community" => community = count,
            "claw_bot" => claw = count,
            _ => {}
        }
    }

    Ok((community, claw))
}

pub async fn get_user_voted(
    conn: &Connection,
    user_id: i64,
    nominee_id: i64,
) -> anyhow::Result<(bool, bool)> {
    let mut rows = conn
        .query(
            "SELECT vote_type FROM votes WHERE user_id = ? AND nominee_id = ?",
            (user_id, nominee_id),
        )
        .await?;

    let mut community = false;
    let mut claw = false;

    while let Some(row) = rows.next().await? {
        let vote_type: String = row.get(0)?;
        match vote_type.as_str() {
            "community" => community = true,
            "claw_bot" => claw = true,
            _ => {}
        }
    }

    Ok((community, claw))
}

pub async fn get_nominee_by_id(conn: &Connection, id: i64) -> anyhow::Result<Option<Nominee>> {
    let mut rows = conn
        .query(
            "SELECT id, github_login, avatar_url, total_commits_90d, total_loc_90d, 
                    repo_count_90d, desloppify_score, vibe_score, nomination_status, 
                    pipeline_report, created_at 
             FROM nominees WHERE id = ?",
            [id],
        )
        .await?;

    if let Some(row) = rows.next().await? {
        Ok(Some(Nominee {
            id: row.get(0)?,
            github_login: row.get(1)?,
            avatar_url: row.get(2)?,
            total_commits_90d: row.get(3)?,
            total_loc_90d: row.get(4)?,
            repo_count_90d: row.get(5)?,
            desloppify_score: row.get(6)?,
            vibe_score: row.get(7)?,
            nomination_status: row.get(8)?,
            pipeline_report: row.get(9)?,
            created_at: row.get(10)?,
        }))
    } else {
        Ok(None)
    }
}

pub async fn cast_vote(
    conn: &Connection,
    user_id: i64,
    nominee_id: i64,
    vote_type: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO votes (user_id, nominee_id, vote_type) VALUES (?, ?, ?)",
        (user_id, nominee_id, vote_type),
    )
    .await?;
    Ok(())
}

pub async fn remove_vote(
    conn: &Connection,
    user_id: i64,
    nominee_id: i64,
    vote_type: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM votes WHERE user_id = ? AND nominee_id = ? AND vote_type = ?",
        (user_id, nominee_id, vote_type),
    )
    .await?;
    Ok(())
}
