use rusqlite::{params, Connection, OptionalExtension};

use crate::db::{new_id, now_ms, require_non_empty};
use crate::error::{AppError, AppResult};
use crate::models::{
    Character, CharacterRelation, EventLink, Faction, Item, Location, OutlineNode, Plotline,
    RelationGraph, TimelineEvent, WorldEntry,
};

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

// --- Plotlines ---

pub fn plotline_create(
    conn: &Connection,
    project_id: &str,
    name: &str,
    color: Option<&str>,
) -> AppResult<Plotline> {
    require_non_empty("name", name)?;
    ensure_project(conn, project_id)?;
    let sort_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM plotlines WHERE project_id = ?1 AND deleted_at IS NULL",
            params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO plotlines (id, project_id, name, color, description, sort_order, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?6, NULL)",
        params![id, project_id, name.trim(), color, sort_order, now],
    )?;
    plotline_get(conn, &id)
}

pub fn plotline_update(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    color: Option<Option<&str>>,
    description: Option<Option<&str>>,
) -> AppResult<Plotline> {
    let mut p = plotline_get(conn, id)?;
    if let Some(n) = name {
        require_non_empty("name", n)?;
        p.name = n.trim().to_string();
    }
    if let Some(c) = color {
        p.color = c.map(|v| v.to_string());
    }
    if let Some(d) = description {
        p.description = d.map(|v| v.to_string());
    }
    p.updated_at = now_ms();
    conn.execute(
        "UPDATE plotlines SET name = ?1, color = ?2, description = ?3, updated_at = ?4 WHERE id = ?5 AND deleted_at IS NULL",
        params![p.name, p.color, p.description, p.updated_at, id],
    )?;
    Ok(p)
}

pub fn plotline_delete(conn: &Connection, id: &str) -> AppResult<()> {
    let now = now_ms();
    let changed = conn.execute(
        "UPDATE plotlines SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
        params![now, id],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("plotline {id}")));
    }
    conn.execute(
        "DELETE FROM chapter_plotlines WHERE plotline_id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn plotline_attach(conn: &Connection, chapter_id: &str, plotline_id: &str) -> AppResult<()> {
    plotline_get(conn, plotline_id)?;
    conn.execute(
        "INSERT OR IGNORE INTO chapter_plotlines (chapter_id, plotline_id) VALUES (?1, ?2)",
        params![chapter_id, plotline_id],
    )?;
    Ok(())
}

pub fn plotline_detach(conn: &Connection, chapter_id: &str, plotline_id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM chapter_plotlines WHERE chapter_id = ?1 AND plotline_id = ?2",
        params![chapter_id, plotline_id],
    )?;
    Ok(())
}

pub fn outline_list(conn: &Connection, project_id: &str) -> AppResult<Vec<OutlineNode>> {
    ensure_project(conn, project_id)?;
    let mut stmt = conn.prepare(
        "SELECT c.id, c.title, c.volume_id, v.title, c.summary, c.sort_order
         FROM chapters c
         JOIN volumes v ON v.id = c.volume_id
         WHERE c.project_id = ?1 AND c.deleted_at IS NULL AND v.deleted_at IS NULL
         ORDER BY v.sort_order ASC, c.sort_order ASC",
    )?;
    let mut nodes = Vec::new();
    let mut rows = stmt.query(params![project_id])?;
    while let Some(row) = rows.next()? {
        let chapter_id: String = row.get(0)?;
        let plotlines = plotlines_for_chapter(conn, &chapter_id)?;
        nodes.push(OutlineNode {
            chapter_id,
            chapter_title: row.get(1)?,
            volume_id: row.get(2)?,
            volume_title: row.get(3)?,
            summary: row.get(4)?,
            sort_order: row.get(5)?,
            plotlines,
        });
    }
    Ok(nodes)
}

fn plotline_get(conn: &Connection, id: &str) -> AppResult<Plotline> {
    conn.query_row(
        "SELECT id, project_id, name, color, description, sort_order, created_at, updated_at
         FROM plotlines WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        map_plotline,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("plotline {id}")))
}

fn plotlines_for_chapter(conn: &Connection, chapter_id: &str) -> AppResult<Vec<Plotline>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.project_id, p.name, p.color, p.description, p.sort_order, p.created_at, p.updated_at
         FROM plotlines p
         JOIN chapter_plotlines cp ON cp.plotline_id = p.id
         WHERE cp.chapter_id = ?1 AND p.deleted_at IS NULL
         ORDER BY p.sort_order ASC",
    )?;
    let rows = stmt
        .query_map(params![chapter_id], map_plotline)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn map_plotline(row: &rusqlite::Row<'_>) -> rusqlite::Result<Plotline> {
    Ok(Plotline {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        color: row.get(3)?,
        description: row.get(4)?,
        sort_order: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

// --- Characters ---

pub fn character_create(
    conn: &Connection,
    project_id: &str,
    name: &str,
    alias: Option<&str>,
    bio: Option<&str>,
    attributes: Option<&str>,
) -> AppResult<Character> {
    require_non_empty("name", name)?;
    ensure_project(conn, project_id)?;
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO characters (id, project_id, name, alias, avatar_path, bio, attributes, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?7, NULL)",
        params![id, project_id, name.trim(), alias, bio, attributes, now],
    )?;
    character_get(conn, &id)
}

pub fn character_update(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    alias: Option<Option<&str>>,
    bio: Option<Option<&str>>,
    attributes: Option<Option<&str>>,
    avatar_path: Option<Option<&str>>,
) -> AppResult<Character> {
    let mut c = character_get(conn, id)?;
    if let Some(n) = name {
        require_non_empty("name", n)?;
        c.name = n.trim().to_string();
    }
    if let Some(a) = alias {
        c.alias = a.map(|v| v.to_string());
    }
    if let Some(b) = bio {
        c.bio = b.map(|v| v.to_string());
    }
    if let Some(attr) = attributes {
        c.attributes = attr.map(|v| v.to_string());
    }
    if let Some(av) = avatar_path {
        c.avatar_path = av.map(|v| v.to_string());
    }
    c.updated_at = now_ms();
    conn.execute(
        "UPDATE characters SET name = ?1, alias = ?2, bio = ?3, attributes = ?4, avatar_path = ?5, updated_at = ?6
         WHERE id = ?7 AND deleted_at IS NULL",
        params![
            c.name,
            c.alias,
            c.bio,
            c.attributes,
            c.avatar_path,
            c.updated_at,
            id
        ],
    )?;
    Ok(c)
}

pub fn character_delete(conn: &Connection, id: &str) -> AppResult<()> {
    let now = now_ms();
    let changed = conn.execute(
        "UPDATE characters SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
        params![now, id],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("character {id}")));
    }
    conn.execute(
        "UPDATE character_relations SET deleted_at = ?1, updated_at = ?1
         WHERE (from_char_id = ?2 OR to_char_id = ?2) AND deleted_at IS NULL",
        params![now, id],
    )?;
    Ok(())
}

pub fn character_list(conn: &Connection, project_id: &str) -> AppResult<Vec<Character>> {
    ensure_project(conn, project_id)?;
    let mut stmt = conn.prepare(
        "SELECT id, project_id, name, alias, avatar_path, bio, attributes, created_at, updated_at
         FROM characters WHERE project_id = ?1 AND deleted_at IS NULL ORDER BY name ASC",
    )?;
    let rows = stmt
        .query_map(params![project_id], map_character)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn relation_create(
    conn: &Connection,
    project_id: &str,
    from_char_id: &str,
    to_char_id: &str,
    relation_type: &str,
    description: Option<&str>,
) -> AppResult<CharacterRelation> {
    require_non_empty("relationType", relation_type)?;
    ensure_project(conn, project_id)?;
    character_get(conn, from_char_id)?;
    character_get(conn, to_char_id)?;
    if from_char_id == to_char_id {
        return Err(AppError::Validation(
            "cannot relate a character to itself".into(),
        ));
    }
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO character_relations (id, project_id, from_char_id, to_char_id, relation_type, description, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, NULL)",
        params![
            id,
            project_id,
            from_char_id,
            to_char_id,
            relation_type.trim(),
            description,
            now
        ],
    )?;
    relation_get(conn, &id)
}

pub fn relation_delete(conn: &Connection, id: &str) -> AppResult<()> {
    let now = now_ms();
    let changed = conn.execute(
        "UPDATE character_relations SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
        params![now, id],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("relation {id}")));
    }
    Ok(())
}

pub fn relation_graph(conn: &Connection, project_id: &str) -> AppResult<RelationGraph> {
    let nodes = character_list(conn, project_id)?;
    let mut stmt = conn.prepare(
        "SELECT id, project_id, from_char_id, to_char_id, relation_type, description, created_at, updated_at
         FROM character_relations WHERE project_id = ?1 AND deleted_at IS NULL",
    )?;
    let edges = stmt
        .query_map(params![project_id], map_relation)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RelationGraph { nodes, edges })
}

fn character_get(conn: &Connection, id: &str) -> AppResult<Character> {
    conn.query_row(
        "SELECT id, project_id, name, alias, avatar_path, bio, attributes, created_at, updated_at
         FROM characters WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        map_character,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("character {id}")))
}

fn relation_get(conn: &Connection, id: &str) -> AppResult<CharacterRelation> {
    conn.query_row(
        "SELECT id, project_id, from_char_id, to_char_id, relation_type, description, created_at, updated_at
         FROM character_relations WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        map_relation,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("relation {id}")))
}

fn map_character(row: &rusqlite::Row<'_>) -> rusqlite::Result<Character> {
    Ok(Character {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        alias: row.get(3)?,
        avatar_path: row.get(4)?,
        bio: row.get(5)?,
        attributes: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_relation(row: &rusqlite::Row<'_>) -> rusqlite::Result<CharacterRelation> {
    Ok(CharacterRelation {
        id: row.get(0)?,
        project_id: row.get(1)?,
        from_char_id: row.get(2)?,
        to_char_id: row.get(3)?,
        relation_type: row.get(4)?,
        description: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

// --- Worldbuilding ---

pub fn location_create(
    conn: &Connection,
    project_id: &str,
    name: &str,
    description: Option<&str>,
    parent_id: Option<&str>,
) -> AppResult<Location> {
    require_non_empty("name", name)?;
    ensure_project(conn, project_id)?;
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO locations (id, project_id, name, description, parent_id, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, NULL)",
        params![id, project_id, name.trim(), description, parent_id, now],
    )?;
    location_get(conn, &id)
}

pub fn location_update(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    description: Option<Option<&str>>,
    parent_id: Option<Option<&str>>,
) -> AppResult<Location> {
    let mut loc = location_get(conn, id)?;
    if let Some(n) = name {
        require_non_empty("name", n)?;
        loc.name = n.trim().to_string();
    }
    if let Some(d) = description {
        loc.description = d.map(|v| v.to_string());
    }
    if let Some(p) = parent_id {
        loc.parent_id = p.map(|v| v.to_string());
    }
    loc.updated_at = now_ms();
    conn.execute(
        "UPDATE locations SET name = ?1, description = ?2, parent_id = ?3, updated_at = ?4 WHERE id = ?5 AND deleted_at IS NULL",
        params![loc.name, loc.description, loc.parent_id, loc.updated_at, id],
    )?;
    Ok(loc)
}

pub fn location_delete(conn: &Connection, id: &str) -> AppResult<()> {
    soft_delete(conn, "locations", id)
}

pub fn faction_create(
    conn: &Connection,
    project_id: &str,
    name: &str,
    description: Option<&str>,
) -> AppResult<Faction> {
    require_non_empty("name", name)?;
    ensure_project(conn, project_id)?;
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO factions (id, project_id, name, description, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, NULL)",
        params![id, project_id, name.trim(), description, now],
    )?;
    faction_get(conn, &id)
}

pub fn faction_update(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    description: Option<Option<&str>>,
) -> AppResult<Faction> {
    let mut f = faction_get(conn, id)?;
    if let Some(n) = name {
        require_non_empty("name", n)?;
        f.name = n.trim().to_string();
    }
    if let Some(d) = description {
        f.description = d.map(|v| v.to_string());
    }
    f.updated_at = now_ms();
    conn.execute(
        "UPDATE factions SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4 AND deleted_at IS NULL",
        params![f.name, f.description, f.updated_at, id],
    )?;
    Ok(f)
}

pub fn faction_delete(conn: &Connection, id: &str) -> AppResult<()> {
    soft_delete(conn, "factions", id)
}

pub fn item_create(
    conn: &Connection,
    project_id: &str,
    name: &str,
    description: Option<&str>,
) -> AppResult<Item> {
    require_non_empty("name", name)?;
    ensure_project(conn, project_id)?;
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO items (id, project_id, name, description, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, NULL)",
        params![id, project_id, name.trim(), description, now],
    )?;
    item_get(conn, &id)
}

pub fn item_update(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    description: Option<Option<&str>>,
) -> AppResult<Item> {
    let mut item = item_get(conn, id)?;
    if let Some(n) = name {
        require_non_empty("name", n)?;
        item.name = n.trim().to_string();
    }
    if let Some(d) = description {
        item.description = d.map(|v| v.to_string());
    }
    item.updated_at = now_ms();
    conn.execute(
        "UPDATE items SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4 AND deleted_at IS NULL",
        params![item.name, item.description, item.updated_at, id],
    )?;
    Ok(item)
}

pub fn item_delete(conn: &Connection, id: &str) -> AppResult<()> {
    soft_delete(conn, "items", id)
}

pub fn worldbuilding_list(
    conn: &Connection,
    project_id: &str,
    entry_type: Option<&str>,
) -> AppResult<Vec<WorldEntry>> {
    ensure_project(conn, project_id)?;
    let mut entries = Vec::new();
    let want = |t: &str| entry_type.is_none_or(|e| e == t);

    if want("location") {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, parent_id, updated_at FROM locations
             WHERE project_id = ?1 AND deleted_at IS NULL ORDER BY name ASC",
        )?;
        let mut rows = stmt.query(params![project_id])?;
        while let Some(row) = rows.next()? {
            entries.push(WorldEntry {
                id: row.get(0)?,
                entry_type: "location".into(),
                name: row.get(1)?,
                description: row.get(2)?,
                parent_id: row.get(3)?,
                updated_at: row.get(4)?,
            });
        }
    }
    if want("faction") {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, updated_at FROM factions
             WHERE project_id = ?1 AND deleted_at IS NULL ORDER BY name ASC",
        )?;
        let mut rows = stmt.query(params![project_id])?;
        while let Some(row) = rows.next()? {
            entries.push(WorldEntry {
                id: row.get(0)?,
                entry_type: "faction".into(),
                name: row.get(1)?,
                description: row.get(2)?,
                parent_id: None,
                updated_at: row.get(3)?,
            });
        }
    }
    if want("item") {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, updated_at FROM items
             WHERE project_id = ?1 AND deleted_at IS NULL ORDER BY name ASC",
        )?;
        let mut rows = stmt.query(params![project_id])?;
        while let Some(row) = rows.next()? {
            entries.push(WorldEntry {
                id: row.get(0)?,
                entry_type: "item".into(),
                name: row.get(1)?,
                description: row.get(2)?,
                parent_id: None,
                updated_at: row.get(3)?,
            });
        }
    }
    Ok(entries)
}

fn location_get(conn: &Connection, id: &str) -> AppResult<Location> {
    conn.query_row(
        "SELECT id, project_id, name, description, parent_id, created_at, updated_at
         FROM locations WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        |row| {
            Ok(Location {
                id: row.get(0)?,
                project_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                parent_id: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("location {id}")))
}

fn faction_get(conn: &Connection, id: &str) -> AppResult<Faction> {
    conn.query_row(
        "SELECT id, project_id, name, description, created_at, updated_at
         FROM factions WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        |row| {
            Ok(Faction {
                id: row.get(0)?,
                project_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("faction {id}")))
}

fn item_get(conn: &Connection, id: &str) -> AppResult<Item> {
    conn.query_row(
        "SELECT id, project_id, name, description, created_at, updated_at
         FROM items WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        |row| {
            Ok(Item {
                id: row.get(0)?,
                project_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("item {id}")))
}

fn soft_delete(conn: &Connection, table: &str, id: &str) -> AppResult<()> {
    let now = now_ms();
    let sql = format!(
        "UPDATE {table} SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL"
    );
    let changed = conn.execute(&sql, params![now, id])?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("{table} {id}")));
    }
    Ok(())
}

// --- Timeline ---

pub fn event_create(
    conn: &Connection,
    project_id: &str,
    title: &str,
    story_time: Option<&str>,
    description: Option<&str>,
) -> AppResult<TimelineEvent> {
    require_non_empty("title", title)?;
    ensure_project(conn, project_id)?;
    let sort_key: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_key), -1) + 1 FROM timeline_events WHERE project_id = ?1 AND deleted_at IS NULL",
            params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO timeline_events (id, project_id, title, description, story_time, sort_key, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, NULL)",
        params![id, project_id, title.trim(), description, story_time, sort_key, now],
    )?;
    event_get(conn, &id)
}

pub fn event_update(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
    story_time: Option<Option<&str>>,
    description: Option<Option<&str>>,
) -> AppResult<TimelineEvent> {
    let mut event = event_get(conn, id)?;
    if let Some(t) = title {
        require_non_empty("title", t)?;
        event.title = t.trim().to_string();
    }
    if let Some(st) = story_time {
        event.story_time = st.map(|v| v.to_string());
    }
    if let Some(d) = description {
        event.description = d.map(|v| v.to_string());
    }
    event.updated_at = now_ms();
    conn.execute(
        "UPDATE timeline_events SET title = ?1, story_time = ?2, description = ?3, updated_at = ?4
         WHERE id = ?5 AND deleted_at IS NULL",
        params![
            event.title,
            event.story_time,
            event.description,
            event.updated_at,
            id
        ],
    )?;
    event_get(conn, id)
}

pub fn event_delete(conn: &Connection, id: &str) -> AppResult<()> {
    soft_delete(conn, "timeline_events", id)
}

pub fn event_reorder(
    conn: &Connection,
    project_id: &str,
    ordered_ids: &[String],
) -> AppResult<()> {
    ensure_project(conn, project_id)?;
    let now = now_ms();
    for (idx, id) in ordered_ids.iter().enumerate() {
        conn.execute(
            "UPDATE timeline_events SET sort_key = ?1, updated_at = ?2 WHERE id = ?3 AND project_id = ?4 AND deleted_at IS NULL",
            params![idx as i64, now, id, project_id],
        )?;
    }
    Ok(())
}

pub fn event_link(
    conn: &Connection,
    event_id: &str,
    entity_type: &str,
    entity_id: &str,
) -> AppResult<()> {
    event_get(conn, event_id)?;
    if !matches!(entity_type, "chapter" | "character" | "location") {
        return Err(AppError::Validation(
            "entityType must be chapter, character, or location".into(),
        ));
    }
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO event_links (id, event_id, entity_type, entity_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, event_id, entity_type, entity_id, now],
    )?;
    Ok(())
}

pub fn event_unlink(
    conn: &Connection,
    event_id: &str,
    entity_type: &str,
    entity_id: &str,
) -> AppResult<()> {
    conn.execute(
        "DELETE FROM event_links WHERE event_id = ?1 AND entity_type = ?2 AND entity_id = ?3",
        params![event_id, entity_type, entity_id],
    )?;
    Ok(())
}

pub fn timeline_list(conn: &Connection, project_id: &str) -> AppResult<Vec<TimelineEvent>> {
    ensure_project(conn, project_id)?;
    let mut stmt = conn.prepare(
        "SELECT id FROM timeline_events WHERE project_id = ?1 AND deleted_at IS NULL ORDER BY sort_key ASC",
    )?;
    let ids = stmt
        .query_map(params![project_id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut events = Vec::with_capacity(ids.len());
    for id in ids {
        events.push(event_get(conn, &id)?);
    }
    Ok(events)
}

fn event_get(conn: &Connection, id: &str) -> AppResult<TimelineEvent> {
    let mut event = conn
        .query_row(
            "SELECT id, project_id, title, description, story_time, sort_key, created_at, updated_at
             FROM timeline_events WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
            |row| {
                Ok(TimelineEvent {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    story_time: row.get(4)?,
                    sort_key: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    links: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("event {id}")))?;

    let mut stmt = conn.prepare(
        "SELECT id, event_id, entity_type, entity_id, created_at FROM event_links WHERE event_id = ?1",
    )?;
    event.links = stmt
        .query_map(params![id], |row| {
            Ok(EventLink {
                id: row.get(0)?,
                event_id: row.get(1)?,
                entity_type: row.get(2)?,
                entity_id: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(event)
}
