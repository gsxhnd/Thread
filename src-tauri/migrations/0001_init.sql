-- Thread / 灰线 — initial schema

PRAGMA foreign_keys = ON;

CREATE TABLE projects (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    synopsis    TEXT,
    author      TEXT,
    cover_path  TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);

CREATE TABLE volumes (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);

CREATE TABLE chapters (
    id          TEXT PRIMARY KEY,
    volume_id   TEXT NOT NULL REFERENCES volumes(id) ON DELETE CASCADE,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    content     TEXT NOT NULL DEFAULT '',
    summary     TEXT,
    word_count  INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL DEFAULT 'draft',
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);

CREATE TABLE plotlines (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    color       TEXT,
    description TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);

CREATE TABLE chapter_plotlines (
    chapter_id  TEXT NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    plotline_id TEXT NOT NULL REFERENCES plotlines(id) ON DELETE CASCADE,
    PRIMARY KEY (chapter_id, plotline_id)
);

CREATE TABLE characters (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    alias       TEXT,
    avatar_path TEXT,
    bio         TEXT,
    attributes  TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);

CREATE TABLE character_relations (
    id            TEXT PRIMARY KEY,
    project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    from_char_id  TEXT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    to_char_id    TEXT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    description   TEXT,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    deleted_at    INTEGER
);

CREATE TABLE locations (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT,
    parent_id   TEXT REFERENCES locations(id),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);

CREATE TABLE factions (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);

CREATE TABLE items (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);

CREATE TABLE timeline_events (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    description  TEXT,
    story_time   TEXT,
    sort_key     INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    deleted_at   INTEGER
);

CREATE TABLE event_links (
    id          TEXT PRIMARY KEY,
    event_id    TEXT NOT NULL REFERENCES timeline_events(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    entity_id   TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE TABLE foreshadowings (
    id                 TEXT PRIMARY KEY,
    project_id         TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title              TEXT NOT NULL,
    description        TEXT,
    status             TEXT NOT NULL DEFAULT 'planted',
    setup_chapter_id   TEXT REFERENCES chapters(id),
    payoff_chapter_id  TEXT REFERENCES chapters(id),
    importance         INTEGER NOT NULL DEFAULT 0,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    deleted_at         INTEGER
);

CREATE TABLE writing_sessions (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    chapter_id   TEXT REFERENCES chapters(id) ON DELETE SET NULL,
    date         TEXT NOT NULL,
    words_added  INTEGER NOT NULL DEFAULT 0,
    duration_ms  INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL
);

CREATE TABLE tags (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    color       TEXT,
    created_at  INTEGER NOT NULL,
    UNIQUE (project_id, name)
);

CREATE TABLE entity_tags (
    tag_id      TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    entity_id   TEXT NOT NULL,
    PRIMARY KEY (tag_id, entity_type, entity_id)
);

CREATE TABLE project_goals (
    project_id   TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    daily_words  INTEGER,
    total_words  INTEGER,
    updated_at   INTEGER NOT NULL
);

CREATE INDEX idx_volumes_project     ON volumes(project_id);
CREATE INDEX idx_chapters_volume     ON chapters(volume_id);
CREATE INDEX idx_chapters_project    ON chapters(project_id);
CREATE INDEX idx_plotlines_project   ON plotlines(project_id);
CREATE INDEX idx_characters_project  ON characters(project_id);
CREATE INDEX idx_relations_from      ON character_relations(from_char_id);
CREATE INDEX idx_relations_to        ON character_relations(to_char_id);
CREATE INDEX idx_locations_project   ON locations(project_id);
CREATE INDEX idx_factions_project    ON factions(project_id);
CREATE INDEX idx_items_project       ON items(project_id);
CREATE INDEX idx_events_project      ON timeline_events(project_id);
CREATE INDEX idx_event_links_event   ON event_links(event_id);
CREATE INDEX idx_event_links_entity  ON event_links(entity_type, entity_id);
CREATE INDEX idx_foreshadow_project  ON foreshadowings(project_id);
CREATE INDEX idx_foreshadow_status   ON foreshadowings(status);
CREATE INDEX idx_sessions_project    ON writing_sessions(project_id);
CREATE INDEX idx_sessions_date       ON writing_sessions(date);
CREATE INDEX idx_entity_tags_entity  ON entity_tags(entity_type, entity_id);
