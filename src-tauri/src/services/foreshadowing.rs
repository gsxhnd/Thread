use rusqlite::{params, Connection, OptionalExtension};

use crate::db::{new_id, now_ms, require_non_empty};
use crate::error::{AppError, AppResult};
use crate::models::{ForeshadowLinkResult, Foreshadowing};

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

pub fn foreshadowing_create(
    conn: &Connection,
    project_id: &str,
    title: &str,
    description: Option<&str>,
    importance: Option<i64>,
) -> AppResult<Foreshadowing> {
    require_non_empty("title", title)?;
    ensure_project(conn, project_id)?;
    let importance = importance.unwrap_or(0).clamp(0, 3);
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO foreshadowings (id, project_id, title, description, status, setup_chapter_id, payoff_chapter_id, importance, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, 'planted', NULL, NULL, ?5, ?6, ?6, NULL)",
        params![id, project_id, title.trim(), description, importance, now],
    )?;
    foreshadowing_get(conn, &id)
}

pub fn foreshadowing_update(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
    description: Option<Option<&str>>,
    importance: Option<i64>,
) -> AppResult<Foreshadowing> {
    let mut f = foreshadowing_get(conn, id)?;
    if let Some(t) = title {
        require_non_empty("title", t)?;
        f.title = t.trim().to_string();
    }
    if let Some(d) = description {
        f.description = d.map(|v| v.to_string());
    }
    if let Some(i) = importance {
        f.importance = i.clamp(0, 3);
    }
    f.updated_at = now_ms();
    conn.execute(
        "UPDATE foreshadowings SET title = ?1, description = ?2, importance = ?3, updated_at = ?4
         WHERE id = ?5 AND deleted_at IS NULL",
        params![f.title, f.description, f.importance, f.updated_at, id],
    )?;
    Ok(f)
}

pub fn foreshadowing_delete(conn: &Connection, id: &str) -> AppResult<()> {
    let now = now_ms();
    let changed = conn.execute(
        "UPDATE foreshadowings SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
        params![now, id],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("foreshadowing {id}")));
    }
    Ok(())
}

pub fn foreshadowing_link(
    conn: &Connection,
    id: &str,
    setup_chapter_id: Option<Option<&str>>,
    payoff_chapter_id: Option<Option<&str>>,
) -> AppResult<ForeshadowLinkResult> {
    let mut f = foreshadowing_get(conn, id)?;
    if let Some(setup) = setup_chapter_id {
        if let Some(cid) = setup {
            ensure_chapter(conn, cid)?;
            f.setup_chapter_id = Some(cid.to_string());
        } else {
            f.setup_chapter_id = None;
        }
    }
    if let Some(payoff) = payoff_chapter_id {
        if let Some(cid) = payoff {
            ensure_chapter(conn, cid)?;
            f.payoff_chapter_id = Some(cid.to_string());
        } else {
            f.payoff_chapter_id = None;
        }
    }

    f.status = if f.setup_chapter_id.is_some() && f.payoff_chapter_id.is_some() {
        "resolved".into()
    } else {
        "planted".into()
    };
    f.updated_at = now_ms();

    conn.execute(
        "UPDATE foreshadowings SET setup_chapter_id = ?1, payoff_chapter_id = ?2, status = ?3, updated_at = ?4
         WHERE id = ?5 AND deleted_at IS NULL",
        params![
            f.setup_chapter_id,
            f.payoff_chapter_id,
            f.status,
            f.updated_at,
            id
        ],
    )?;

    Ok(ForeshadowLinkResult { status: f.status })
}

pub fn foreshadowing_list(
    conn: &Connection,
    project_id: &str,
    status: Option<&str>,
) -> AppResult<Vec<Foreshadowing>> {
    ensure_project(conn, project_id)?;
    if let Some(status) = status {
        let mut stmt = conn.prepare(
            "SELECT id, project_id, title, description, status, setup_chapter_id, payoff_chapter_id, importance, created_at, updated_at
             FROM foreshadowings WHERE project_id = ?1 AND deleted_at IS NULL AND status = ?2
             ORDER BY importance DESC, updated_at DESC",
        )?;
        let rows = stmt
            .query_map(params![project_id, status], map_foreshadowing)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, project_id, title, description, status, setup_chapter_id, payoff_chapter_id, importance, created_at, updated_at
             FROM foreshadowings WHERE project_id = ?1 AND deleted_at IS NULL
             ORDER BY importance DESC, updated_at DESC",
        )?;
        let rows = stmt
            .query_map(params![project_id], map_foreshadowing)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

pub fn foreshadowing_unresolved(
    conn: &Connection,
    project_id: &str,
) -> AppResult<Vec<Foreshadowing>> {
    foreshadowing_list(conn, project_id, Some("planted"))
}

fn foreshadowing_get(conn: &Connection, id: &str) -> AppResult<Foreshadowing> {
    conn.query_row(
        "SELECT id, project_id, title, description, status, setup_chapter_id, payoff_chapter_id, importance, created_at, updated_at
         FROM foreshadowings WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        map_foreshadowing,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("foreshadowing {id}")))
}

fn ensure_chapter(conn: &Connection, id: &str) -> AppResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM chapters WHERE id = ?1 AND deleted_at IS NULL)",
        params![id],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(AppError::NotFound(format!("chapter {id}")));
    }
    Ok(())
}

fn map_foreshadowing(row: &rusqlite::Row<'_>) -> rusqlite::Result<Foreshadowing> {
    Ok(Foreshadowing {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        status: row.get(4)?,
        setup_chapter_id: row.get(5)?,
        payoff_chapter_id: row.get(6)?,
        importance: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}
