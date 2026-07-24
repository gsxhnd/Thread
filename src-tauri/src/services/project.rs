use rusqlite::{params, Connection, OptionalExtension};

use crate::db::{new_id, now_ms, require_non_empty};
use crate::error::{AppError, AppResult};
use crate::models::Project;

pub fn project_create(
    conn: &Connection,
    title: &str,
    synopsis: Option<&str>,
    author: Option<&str>,
) -> AppResult<Project> {
    require_non_empty("title", title)?;
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO projects (id, title, synopsis, author, cover_path, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5, NULL)",
        params![id, title.trim(), synopsis, author, now],
    )?;
    // Default first volume for new projects
    let volume_id = new_id();
    conn.execute(
        "INSERT INTO volumes (id, project_id, title, sort_order, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, '第一卷', 0, ?3, ?3, NULL)",
        params![volume_id, id, now],
    )?;
    let chapter_id = new_id();
    conn.execute(
        "INSERT INTO chapters (id, volume_id, project_id, title, content, summary, word_count, status, sort_order, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, '第一章', '', NULL, 0, 'draft', 0, ?4, ?4, NULL)",
        params![chapter_id, volume_id, id, now],
    )?;
    project_get(conn, &id)
}

pub fn project_update(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
    synopsis: Option<Option<&str>>,
    author: Option<Option<&str>>,
) -> AppResult<Project> {
    let mut project = project_get(conn, id)?;
    if let Some(t) = title {
        require_non_empty("title", t)?;
        project.title = t.trim().to_string();
    }
    if let Some(s) = synopsis {
        project.synopsis = s.map(|v| v.to_string());
    }
    if let Some(a) = author {
        project.author = a.map(|v| v.to_string());
    }
    project.updated_at = now_ms();
    conn.execute(
        "UPDATE projects SET title = ?1, synopsis = ?2, author = ?3, updated_at = ?4 WHERE id = ?5 AND deleted_at IS NULL",
        params![
            project.title,
            project.synopsis,
            project.author,
            project.updated_at,
            id
        ],
    )?;
    Ok(project)
}

pub fn project_delete(conn: &Connection, id: &str) -> AppResult<()> {
    let now = now_ms();
    let changed = conn.execute(
        "UPDATE projects SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
        params![now, id],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("project {id}")));
    }
    Ok(())
}

pub fn project_list(conn: &Connection) -> AppResult<Vec<Project>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, synopsis, author, cover_path, created_at, updated_at
         FROM projects WHERE deleted_at IS NULL ORDER BY updated_at DESC",
    )?;
    let rows = stmt
        .query_map([], map_project)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn project_get(conn: &Connection, id: &str) -> AppResult<Project> {
    conn.query_row(
        "SELECT id, title, synopsis, author, cover_path, created_at, updated_at
         FROM projects WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        map_project,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("project {id}")))
}

fn map_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        title: row.get(1)?,
        synopsis: row.get(2)?,
        author: row.get(3)?,
        cover_path: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}
