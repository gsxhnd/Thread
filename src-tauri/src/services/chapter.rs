use rusqlite::{params, Connection, OptionalExtension};

use crate::db::{count_words, new_id, now_ms, require_non_empty, today_local};
use crate::error::{AppError, AppResult};
use crate::models::{
    Chapter, ChapterSaveResult, ChapterSummary, Volume, VolumeWithChapters,
};

pub fn volume_create(conn: &Connection, project_id: &str, title: &str) -> AppResult<Volume> {
    require_non_empty("title", title)?;
    ensure_project(conn, project_id)?;
    let sort_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM volumes WHERE project_id = ?1 AND deleted_at IS NULL",
            params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO volumes (id, project_id, title, sort_order, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, NULL)",
        params![id, project_id, title.trim(), sort_order, now],
    )?;
    volume_get(conn, &id)
}

pub fn volume_update(conn: &Connection, id: &str, title: Option<&str>) -> AppResult<Volume> {
    let mut volume = volume_get(conn, id)?;
    if let Some(t) = title {
        require_non_empty("title", t)?;
        volume.title = t.trim().to_string();
    }
    volume.updated_at = now_ms();
    conn.execute(
        "UPDATE volumes SET title = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
        params![volume.title, volume.updated_at, id],
    )?;
    Ok(volume)
}

pub fn volume_delete(conn: &Connection, id: &str) -> AppResult<()> {
    let now = now_ms();
    let changed = conn.execute(
        "UPDATE volumes SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
        params![now, id],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("volume {id}")));
    }
    conn.execute(
        "UPDATE chapters SET deleted_at = ?1, updated_at = ?1 WHERE volume_id = ?2 AND deleted_at IS NULL",
        params![now, id],
    )?;
    Ok(())
}

pub fn volume_reorder(conn: &Connection, project_id: &str, ordered_ids: &[String]) -> AppResult<()> {
    ensure_project(conn, project_id)?;
    let now = now_ms();
    for (idx, id) in ordered_ids.iter().enumerate() {
        conn.execute(
            "UPDATE volumes SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND project_id = ?4 AND deleted_at IS NULL",
            params![idx as i64, now, id, project_id],
        )?;
    }
    Ok(())
}

pub fn chapter_create(conn: &Connection, volume_id: &str, title: &str) -> AppResult<Chapter> {
    require_non_empty("title", title)?;
    let volume = volume_get(conn, volume_id)?;
    let sort_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM chapters WHERE volume_id = ?1 AND deleted_at IS NULL",
            params![volume_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO chapters (id, volume_id, project_id, title, content, summary, word_count, status, sort_order, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, '', NULL, 0, 'draft', ?5, ?6, ?6, NULL)",
        params![id, volume_id, volume.project_id, title.trim(), sort_order, now],
    )?;
    chapter_get(conn, &id)
}

pub fn chapter_get(conn: &Connection, id: &str) -> AppResult<Chapter> {
    conn.query_row(
        "SELECT id, volume_id, project_id, title, content, summary, word_count, status, sort_order, created_at, updated_at
         FROM chapters WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        map_chapter,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("chapter {id}")))
}

pub fn chapter_save(
    conn: &Connection,
    id: &str,
    content: &str,
    title: Option<&str>,
) -> AppResult<ChapterSaveResult> {
    let chapter = chapter_get(conn, id)?;
    let word_count = count_words(content);
    let words_added = word_count - chapter.word_count;
    let now = now_ms();
    let new_title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or(chapter.title);

    conn.execute(
        "UPDATE chapters SET content = ?1, title = ?2, word_count = ?3, updated_at = ?4
         WHERE id = ?5 AND deleted_at IS NULL",
        params![content, new_title, word_count, now, id],
    )?;

    if words_added != 0 {
        let session_id = new_id();
        conn.execute(
            "INSERT INTO writing_sessions (id, project_id, chapter_id, date, words_added, duration_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
            params![
                session_id,
                chapter.project_id,
                id,
                today_local(),
                words_added,
                now
            ],
        )?;
    }

    // Touch project updated_at
    conn.execute(
        "UPDATE projects SET updated_at = ?1 WHERE id = ?2",
        params![now, chapter.project_id],
    )?;

    Ok(ChapterSaveResult {
        saved_at: now,
        word_count,
    })
}

pub fn chapter_update_status(conn: &Connection, id: &str, status: &str) -> AppResult<Chapter> {
    if status != "draft" && status != "done" {
        return Err(AppError::Validation(
            "status must be draft or done".into(),
        ));
    }
    let now = now_ms();
    let changed = conn.execute(
        "UPDATE chapters SET status = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
        params![status, now, id],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("chapter {id}")));
    }
    chapter_get(conn, id)
}

pub fn chapter_delete(conn: &Connection, id: &str) -> AppResult<()> {
    let now = now_ms();
    let changed = conn.execute(
        "UPDATE chapters SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
        params![now, id],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("chapter {id}")));
    }
    Ok(())
}

pub fn chapter_reorder(
    conn: &Connection,
    volume_id: &str,
    ordered_ids: &[String],
) -> AppResult<()> {
    volume_get(conn, volume_id)?;
    let now = now_ms();
    for (idx, id) in ordered_ids.iter().enumerate() {
        conn.execute(
            "UPDATE chapters SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND volume_id = ?4 AND deleted_at IS NULL",
            params![idx as i64, now, id, volume_id],
        )?;
    }
    Ok(())
}

pub fn chapter_tree(conn: &Connection, project_id: &str) -> AppResult<Vec<VolumeWithChapters>> {
    ensure_project(conn, project_id)?;
    let mut vol_stmt = conn.prepare(
        "SELECT id, project_id, title, sort_order, created_at, updated_at
         FROM volumes WHERE project_id = ?1 AND deleted_at IS NULL ORDER BY sort_order ASC",
    )?;
    let volumes = vol_stmt
        .query_map(params![project_id], map_volume)?
        .collect::<Result<Vec<_>, _>>()?;

    let mut result = Vec::with_capacity(volumes.len());
    for volume in volumes {
        let mut ch_stmt = conn.prepare(
            "SELECT id, volume_id, project_id, title, summary, word_count, status, sort_order, updated_at
             FROM chapters WHERE volume_id = ?1 AND deleted_at IS NULL ORDER BY sort_order ASC",
        )?;
        let chapters = ch_stmt
            .query_map(params![volume.id], map_chapter_summary)?
            .collect::<Result<Vec<_>, _>>()?;
        result.push(VolumeWithChapters { volume, chapters });
    }
    Ok(result)
}

fn volume_get(conn: &Connection, id: &str) -> AppResult<Volume> {
    conn.query_row(
        "SELECT id, project_id, title, sort_order, created_at, updated_at
         FROM volumes WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        map_volume,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("volume {id}")))
}

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

fn map_volume(row: &rusqlite::Row<'_>) -> rusqlite::Result<Volume> {
    Ok(Volume {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        sort_order: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn map_chapter(row: &rusqlite::Row<'_>) -> rusqlite::Result<Chapter> {
    Ok(Chapter {
        id: row.get(0)?,
        volume_id: row.get(1)?,
        project_id: row.get(2)?,
        title: row.get(3)?,
        content: row.get(4)?,
        summary: row.get(5)?,
        word_count: row.get(6)?,
        status: row.get(7)?,
        sort_order: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn map_chapter_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChapterSummary> {
    Ok(ChapterSummary {
        id: row.get(0)?,
        volume_id: row.get(1)?,
        project_id: row.get(2)?,
        title: row.get(3)?,
        summary: row.get(4)?,
        word_count: row.get(5)?,
        status: row.get(6)?,
        sort_order: row.get(7)?,
        updated_at: row.get(8)?,
    })
}
