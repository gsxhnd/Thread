use rusqlite::{params, Connection};

use crate::db::{now_ms, today_local};
use crate::error::{AppError, AppResult};
use crate::models::{DayCount, Goal, StatsOverview, Streak};

fn ensure_project(conn: &Connection, project_id: &str) -> AppResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1 AND deleted_at IS NULL)",
        params![project_id],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(AppError::NotFound(format!("project {project_id}")));
    }
    Ok(())
}

pub fn stats_overview(conn: &Connection, project_id: &str) -> AppResult<StatsOverview> {
    ensure_project(conn, project_id)?;
    let total_words: i64 = conn.query_row(
        "SELECT COALESCE(SUM(word_count), 0) FROM chapters WHERE project_id = ?1 AND deleted_at IS NULL",
        params![project_id],
        |r| r.get(0),
    )?;
    let chapter_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chapters WHERE project_id = ?1 AND deleted_at IS NULL",
        params![project_id],
        |r| r.get(0),
    )?;
    let volume_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM volumes WHERE project_id = ?1 AND deleted_at IS NULL",
        params![project_id],
        |r| r.get(0),
    )?;
    let character_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM characters WHERE project_id = ?1 AND deleted_at IS NULL",
        params![project_id],
        |r| r.get(0),
    )?;
    let unresolved_foreshadowings: i64 = conn.query_row(
        "SELECT COUNT(*) FROM foreshadowings WHERE project_id = ?1 AND deleted_at IS NULL AND status = 'planted'",
        params![project_id],
        |r| r.get(0),
    )?;
    let words_today: i64 = conn.query_row(
        "SELECT COALESCE(SUM(words_added), 0) FROM writing_sessions WHERE project_id = ?1 AND date = ?2",
        params![project_id, today_local()],
        |r| r.get(0),
    )?;

    Ok(StatsOverview {
        total_words,
        chapter_count,
        volume_count,
        character_count,
        unresolved_foreshadowings,
        words_today,
    })
}

pub fn stats_calendar(conn: &Connection, project_id: &str, year: i32) -> AppResult<Vec<DayCount>> {
    ensure_project(conn, project_id)?;
    let prefix = format!("{year}-");
    let mut stmt = conn.prepare(
        "SELECT date, SUM(words_added) FROM writing_sessions
         WHERE project_id = ?1 AND date LIKE ?2
         GROUP BY date ORDER BY date ASC",
    )?;
    let rows = stmt
        .query_map(params![project_id, format!("{prefix}%")], |row| {
            Ok(DayCount {
                date: row.get(0)?,
                words: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn stats_streak(conn: &Connection, project_id: &str) -> AppResult<Streak> {
    ensure_project(conn, project_id)?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT date FROM writing_sessions
         WHERE project_id = ?1 AND words_added > 0
         ORDER BY date DESC",
    )?;
    let dates: Vec<String> = stmt
        .query_map(params![project_id], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    if dates.is_empty() {
        return Ok(Streak {
            current: 0,
            longest: 0,
        });
    }

    let mut longest = 1i64;
    let mut run = 1i64;
    let mut current = 0i64;

    let today = today_local();
    let yesterday = chrono::Local::now()
        .date_naive()
        .pred_opt()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();

    if dates.first().map(|d| d.as_str()) == Some(today.as_str())
        || dates.first().map(|d| d.as_str()) == Some(yesterday.as_str())
    {
        current = 1;
        for window in dates.windows(2) {
            let newer = &window[0];
            let older = &window[1];
            if is_consecutive(older, newer) {
                current += 1;
            } else {
                break;
            }
        }
    }

    for window in dates.windows(2) {
        let newer = &window[0];
        let older = &window[1];
        if is_consecutive(older, newer) {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 1;
        }
    }

    Ok(Streak { current, longest })
}

fn is_consecutive(older: &str, newer: &str) -> bool {
    let Ok(o) = chrono::NaiveDate::parse_from_str(older, "%Y-%m-%d") else {
        return false;
    };
    let Ok(n) = chrono::NaiveDate::parse_from_str(newer, "%Y-%m-%d") else {
        return false;
    };
    n.signed_duration_since(o).num_days() == 1
}

pub fn goal_set(
    conn: &Connection,
    project_id: &str,
    daily_words: Option<i64>,
    total_words: Option<i64>,
) -> AppResult<Goal> {
    ensure_project(conn, project_id)?;
    let now = now_ms();
    conn.execute(
        "INSERT INTO project_goals (project_id, daily_words, total_words, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(project_id) DO UPDATE SET
           daily_words = excluded.daily_words,
           total_words = excluded.total_words,
           updated_at = excluded.updated_at",
        params![project_id, daily_words, total_words, now],
    )?;
    Ok(Goal {
        project_id: project_id.to_string(),
        daily_words,
        total_words,
        updated_at: now,
    })
}

pub fn goal_get(conn: &Connection, project_id: &str) -> AppResult<Option<Goal>> {
    ensure_project(conn, project_id)?;
    let mut stmt = conn.prepare(
        "SELECT project_id, daily_words, total_words, updated_at FROM project_goals WHERE project_id = ?1",
    )?;
    let mut rows = stmt.query(params![project_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(Goal {
            project_id: row.get(0)?,
            daily_words: row.get(1)?,
            total_words: row.get(2)?,
            updated_at: row.get(3)?,
        }))
    } else {
        Ok(None)
    }
}
