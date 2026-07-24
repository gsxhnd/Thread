//! Tauri command handlers — thin validation + service orchestration.

use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use crate::db::SharedDb;
use crate::error::AppError;
use crate::models::*;
use crate::services::{self, AiState};

type CmdResult<T> = Result<T, AppError>;

// ---------- Project ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCreatePayload {
    pub title: String,
    pub synopsis: Option<String>,
    pub author: Option<String>,
}

#[tauri::command]
pub fn project_create(db: State<'_, SharedDb>, payload: ProjectCreatePayload) -> CmdResult<Project> {
    db.with_conn(|conn| {
        services::project_create(
            conn,
            &payload.title,
            payload.synopsis.as_deref(),
            payload.author.as_deref(),
        )
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdatePayload {
    pub id: String,
    pub title: Option<String>,
    pub synopsis: Option<Option<String>>,
    pub author: Option<Option<String>>,
}

#[tauri::command]
pub fn project_update(db: State<'_, SharedDb>, payload: ProjectUpdatePayload) -> CmdResult<Project> {
    db.with_conn(|conn| {
        services::project_update(
            conn,
            &payload.id,
            payload.title.as_deref(),
            payload
                .synopsis
                .as_ref()
                .map(|s| s.as_deref()),
            payload.author.as_ref().map(|a| a.as_deref()),
        )
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdPayload {
    pub id: String,
}

#[tauri::command]
pub fn project_delete(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<()> {
    db.with_conn(|conn| services::project_delete(conn, &payload.id))
}

#[tauri::command]
pub fn project_list(db: State<'_, SharedDb>) -> CmdResult<Vec<Project>> {
    db.with_conn(services::project_list)
}

#[tauri::command]
pub fn project_get(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<Project> {
    db.with_conn(|conn| services::project_get(conn, &payload.id))
}

// ---------- Volume / Chapter ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeCreatePayload {
    pub project_id: String,
    pub title: String,
}

#[tauri::command]
pub fn volume_create(db: State<'_, SharedDb>, payload: VolumeCreatePayload) -> CmdResult<Volume> {
    db.with_conn(|conn| services::volume_create(conn, &payload.project_id, &payload.title))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeUpdatePayload {
    pub id: String,
    pub title: Option<String>,
}

#[tauri::command]
pub fn volume_update(db: State<'_, SharedDb>, payload: VolumeUpdatePayload) -> CmdResult<Volume> {
    db.with_conn(|conn| services::volume_update(conn, &payload.id, payload.title.as_deref()))
}

#[tauri::command]
pub fn volume_delete(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<()> {
    db.with_conn(|conn| services::volume_delete(conn, &payload.id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderPayload {
    pub project_id: Option<String>,
    pub volume_id: Option<String>,
    pub ordered_ids: Vec<String>,
}

#[tauri::command]
pub fn volume_reorder(db: State<'_, SharedDb>, payload: ReorderPayload) -> CmdResult<()> {
    let project_id = payload
        .project_id
        .ok_or_else(|| AppError::Validation("projectId required".into()))?;
    db.with_conn(|conn| services::volume_reorder(conn, &project_id, &payload.ordered_ids))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterCreatePayload {
    pub volume_id: String,
    pub title: String,
}

#[tauri::command]
pub fn chapter_create(
    db: State<'_, SharedDb>,
    payload: ChapterCreatePayload,
) -> CmdResult<Chapter> {
    db.with_conn(|conn| services::chapter_create(conn, &payload.volume_id, &payload.title))
}

#[tauri::command]
pub fn chapter_get(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<Chapter> {
    db.with_conn(|conn| services::chapter_get(conn, &payload.id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterSavePayload {
    pub id: String,
    pub content: String,
    pub title: Option<String>,
}

#[tauri::command]
pub fn chapter_save(
    app: AppHandle,
    db: State<'_, SharedDb>,
    payload: ChapterSavePayload,
) -> CmdResult<ChapterSaveResult> {
    let result = db.with_conn(|conn| {
        services::chapter_save(conn, &payload.id, &payload.content, payload.title.as_deref())
    })?;
    let _ = app.emit(
        "chapter://saved",
        serde_json::json!({
            "chapterId": payload.id,
            "savedAt": result.saved_at,
            "wordCount": result.word_count,
        }),
    );
    Ok(result)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterStatusPayload {
    pub id: String,
    pub status: String,
}

#[tauri::command]
pub fn chapter_update_status(
    db: State<'_, SharedDb>,
    payload: ChapterStatusPayload,
) -> CmdResult<Chapter> {
    db.with_conn(|conn| services::chapter_update_status(conn, &payload.id, &payload.status))
}

#[tauri::command]
pub fn chapter_delete(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<()> {
    db.with_conn(|conn| services::chapter_delete(conn, &payload.id))
}

#[tauri::command]
pub fn chapter_reorder(db: State<'_, SharedDb>, payload: ReorderPayload) -> CmdResult<()> {
    let volume_id = payload
        .volume_id
        .ok_or_else(|| AppError::Validation("volumeId required".into()))?;
    db.with_conn(|conn| services::chapter_reorder(conn, &volume_id, &payload.ordered_ids))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIdPayload {
    pub project_id: String,
}

#[tauri::command]
pub fn chapter_tree(
    db: State<'_, SharedDb>,
    payload: ProjectIdPayload,
) -> CmdResult<Vec<VolumeWithChapters>> {
    db.with_conn(|conn| services::chapter_tree(conn, &payload.project_id))
}

// ---------- Plotline ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlotlineCreatePayload {
    pub project_id: String,
    pub name: String,
    pub color: Option<String>,
}

#[tauri::command]
pub fn plotline_create(
    db: State<'_, SharedDb>,
    payload: PlotlineCreatePayload,
) -> CmdResult<Plotline> {
    db.with_conn(|conn| {
        services::plotline_create(
            conn,
            &payload.project_id,
            &payload.name,
            payload.color.as_deref(),
        )
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlotlineUpdatePayload {
    pub id: String,
    pub name: Option<String>,
    pub color: Option<Option<String>>,
    pub description: Option<Option<String>>,
}

#[tauri::command]
pub fn plotline_update(
    db: State<'_, SharedDb>,
    payload: PlotlineUpdatePayload,
) -> CmdResult<Plotline> {
    db.with_conn(|conn| {
        services::plotline_update(
            conn,
            &payload.id,
            payload.name.as_deref(),
            payload.color.as_ref().map(|c| c.as_deref()),
            payload.description.as_ref().map(|d| d.as_deref()),
        )
    })
}

#[tauri::command]
pub fn plotline_delete(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<()> {
    db.with_conn(|conn| services::plotline_delete(conn, &payload.id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlotlineAttachPayload {
    pub chapter_id: String,
    pub plotline_id: String,
}

#[tauri::command]
pub fn plotline_attach(db: State<'_, SharedDb>, payload: PlotlineAttachPayload) -> CmdResult<()> {
    db.with_conn(|conn| {
        services::plotline_attach(conn, &payload.chapter_id, &payload.plotline_id)
    })
}

#[tauri::command]
pub fn plotline_detach(db: State<'_, SharedDb>, payload: PlotlineAttachPayload) -> CmdResult<()> {
    db.with_conn(|conn| {
        services::plotline_detach(conn, &payload.chapter_id, &payload.plotline_id)
    })
}

#[tauri::command]
pub fn outline_list(
    db: State<'_, SharedDb>,
    payload: ProjectIdPayload,
) -> CmdResult<Vec<OutlineNode>> {
    db.with_conn(|conn| services::outline_list(conn, &payload.project_id))
}

// ---------- Character ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterCreatePayload {
    pub project_id: String,
    pub name: String,
    pub alias: Option<String>,
    pub bio: Option<String>,
    pub attributes: Option<String>,
}

#[tauri::command]
pub fn character_create(
    db: State<'_, SharedDb>,
    payload: CharacterCreatePayload,
) -> CmdResult<Character> {
    db.with_conn(|conn| {
        services::character_create(
            conn,
            &payload.project_id,
            &payload.name,
            payload.alias.as_deref(),
            payload.bio.as_deref(),
            payload.attributes.as_deref(),
        )
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterUpdatePayload {
    pub id: String,
    pub name: Option<String>,
    pub alias: Option<Option<String>>,
    pub bio: Option<Option<String>>,
    pub attributes: Option<Option<String>>,
    pub avatar_path: Option<Option<String>>,
}

#[tauri::command]
pub fn character_update(
    db: State<'_, SharedDb>,
    payload: CharacterUpdatePayload,
) -> CmdResult<Character> {
    db.with_conn(|conn| {
        services::character_update(
            conn,
            &payload.id,
            payload.name.as_deref(),
            payload.alias.as_ref().map(|a| a.as_deref()),
            payload.bio.as_ref().map(|b| b.as_deref()),
            payload.attributes.as_ref().map(|a| a.as_deref()),
            payload.avatar_path.as_ref().map(|a| a.as_deref()),
        )
    })
}

#[tauri::command]
pub fn character_delete(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<()> {
    db.with_conn(|conn| services::character_delete(conn, &payload.id))
}

#[tauri::command]
pub fn character_list(
    db: State<'_, SharedDb>,
    payload: ProjectIdPayload,
) -> CmdResult<Vec<Character>> {
    db.with_conn(|conn| services::character_list(conn, &payload.project_id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationCreatePayload {
    pub project_id: String,
    pub from_char_id: String,
    pub to_char_id: String,
    pub relation_type: String,
    pub description: Option<String>,
}

#[tauri::command]
pub fn relation_create(
    db: State<'_, SharedDb>,
    payload: RelationCreatePayload,
) -> CmdResult<CharacterRelation> {
    db.with_conn(|conn| {
        services::relation_create(
            conn,
            &payload.project_id,
            &payload.from_char_id,
            &payload.to_char_id,
            &payload.relation_type,
            payload.description.as_deref(),
        )
    })
}

#[tauri::command]
pub fn relation_delete(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<()> {
    db.with_conn(|conn| services::relation_delete(conn, &payload.id))
}

#[tauri::command]
pub fn relation_graph(
    db: State<'_, SharedDb>,
    payload: ProjectIdPayload,
) -> CmdResult<RelationGraph> {
    db.with_conn(|conn| services::relation_graph(conn, &payload.project_id))
}

// ---------- Worldbuilding ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedCreatePayload {
    pub project_id: String,
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedUpdatePayload {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub parent_id: Option<Option<String>>,
}

#[tauri::command]
pub fn location_create(
    db: State<'_, SharedDb>,
    payload: NamedCreatePayload,
) -> CmdResult<Location> {
    db.with_conn(|conn| {
        services::location_create(
            conn,
            &payload.project_id,
            &payload.name,
            payload.description.as_deref(),
            payload.parent_id.as_deref(),
        )
    })
}

#[tauri::command]
pub fn location_update(
    db: State<'_, SharedDb>,
    payload: NamedUpdatePayload,
) -> CmdResult<Location> {
    db.with_conn(|conn| {
        services::location_update(
            conn,
            &payload.id,
            payload.name.as_deref(),
            payload.description.as_ref().map(|d| d.as_deref()),
            payload.parent_id.as_ref().map(|p| p.as_deref()),
        )
    })
}

#[tauri::command]
pub fn location_delete(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<()> {
    db.with_conn(|conn| services::location_delete(conn, &payload.id))
}

#[tauri::command]
pub fn faction_create(db: State<'_, SharedDb>, payload: NamedCreatePayload) -> CmdResult<Faction> {
    db.with_conn(|conn| {
        services::faction_create(
            conn,
            &payload.project_id,
            &payload.name,
            payload.description.as_deref(),
        )
    })
}

#[tauri::command]
pub fn faction_update(db: State<'_, SharedDb>, payload: NamedUpdatePayload) -> CmdResult<Faction> {
    db.with_conn(|conn| {
        services::faction_update(
            conn,
            &payload.id,
            payload.name.as_deref(),
            payload.description.as_ref().map(|d| d.as_deref()),
        )
    })
}

#[tauri::command]
pub fn faction_delete(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<()> {
    db.with_conn(|conn| services::faction_delete(conn, &payload.id))
}

#[tauri::command]
pub fn item_create(db: State<'_, SharedDb>, payload: NamedCreatePayload) -> CmdResult<Item> {
    db.with_conn(|conn| {
        services::item_create(
            conn,
            &payload.project_id,
            &payload.name,
            payload.description.as_deref(),
        )
    })
}

#[tauri::command]
pub fn item_update(db: State<'_, SharedDb>, payload: NamedUpdatePayload) -> CmdResult<Item> {
    db.with_conn(|conn| {
        services::item_update(
            conn,
            &payload.id,
            payload.name.as_deref(),
            payload.description.as_ref().map(|d| d.as_deref()),
        )
    })
}

#[tauri::command]
pub fn item_delete(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<()> {
    db.with_conn(|conn| services::item_delete(conn, &payload.id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldListPayload {
    pub project_id: String,
    #[serde(rename = "type")]
    pub entry_type: Option<String>,
}

#[tauri::command]
pub fn worldbuilding_list(
    db: State<'_, SharedDb>,
    payload: WorldListPayload,
) -> CmdResult<Vec<WorldEntry>> {
    db.with_conn(|conn| {
        services::worldbuilding_list(conn, &payload.project_id, payload.entry_type.as_deref())
    })
}

// ---------- Timeline ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventCreatePayload {
    pub project_id: String,
    pub title: String,
    pub story_time: Option<String>,
    pub description: Option<String>,
}

#[tauri::command]
pub fn event_create(
    db: State<'_, SharedDb>,
    payload: EventCreatePayload,
) -> CmdResult<TimelineEvent> {
    db.with_conn(|conn| {
        services::event_create(
            conn,
            &payload.project_id,
            &payload.title,
            payload.story_time.as_deref(),
            payload.description.as_deref(),
        )
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventUpdatePayload {
    pub id: String,
    pub title: Option<String>,
    pub story_time: Option<Option<String>>,
    pub description: Option<Option<String>>,
}

#[tauri::command]
pub fn event_update(
    db: State<'_, SharedDb>,
    payload: EventUpdatePayload,
) -> CmdResult<TimelineEvent> {
    db.with_conn(|conn| {
        services::event_update(
            conn,
            &payload.id,
            payload.title.as_deref(),
            payload.story_time.as_ref().map(|s| s.as_deref()),
            payload.description.as_ref().map(|d| d.as_deref()),
        )
    })
}

#[tauri::command]
pub fn event_delete(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<()> {
    db.with_conn(|conn| services::event_delete(conn, &payload.id))
}

#[tauri::command]
pub fn event_reorder(db: State<'_, SharedDb>, payload: ReorderPayload) -> CmdResult<()> {
    let project_id = payload
        .project_id
        .ok_or_else(|| AppError::Validation("projectId required".into()))?;
    db.with_conn(|conn| services::event_reorder(conn, &project_id, &payload.ordered_ids))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventLinkPayload {
    pub event_id: String,
    pub entity_type: String,
    pub entity_id: String,
}

#[tauri::command]
pub fn event_link(db: State<'_, SharedDb>, payload: EventLinkPayload) -> CmdResult<()> {
    db.with_conn(|conn| {
        services::event_link(
            conn,
            &payload.event_id,
            &payload.entity_type,
            &payload.entity_id,
        )
    })
}

#[tauri::command]
pub fn event_unlink(db: State<'_, SharedDb>, payload: EventLinkPayload) -> CmdResult<()> {
    db.with_conn(|conn| {
        services::event_unlink(
            conn,
            &payload.event_id,
            &payload.entity_type,
            &payload.entity_id,
        )
    })
}

#[tauri::command]
pub fn timeline_list(
    db: State<'_, SharedDb>,
    payload: ProjectIdPayload,
) -> CmdResult<Vec<TimelineEvent>> {
    db.with_conn(|conn| services::timeline_list(conn, &payload.project_id))
}

// ---------- Foreshadowing ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeshadowCreatePayload {
    pub project_id: String,
    pub title: String,
    pub description: Option<String>,
    pub importance: Option<i64>,
}

#[tauri::command]
pub fn foreshadowing_create(
    db: State<'_, SharedDb>,
    payload: ForeshadowCreatePayload,
) -> CmdResult<Foreshadowing> {
    db.with_conn(|conn| {
        services::foreshadowing_create(
            conn,
            &payload.project_id,
            &payload.title,
            payload.description.as_deref(),
            payload.importance,
        )
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeshadowUpdatePayload {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub importance: Option<i64>,
}

#[tauri::command]
pub fn foreshadowing_update(
    db: State<'_, SharedDb>,
    payload: ForeshadowUpdatePayload,
) -> CmdResult<Foreshadowing> {
    db.with_conn(|conn| {
        services::foreshadowing_update(
            conn,
            &payload.id,
            payload.title.as_deref(),
            payload.description.as_ref().map(|d| d.as_deref()),
            payload.importance,
        )
    })
}

#[tauri::command]
pub fn foreshadowing_delete(db: State<'_, SharedDb>, payload: IdPayload) -> CmdResult<()> {
    db.with_conn(|conn| services::foreshadowing_delete(conn, &payload.id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeshadowLinkPayload {
    pub id: String,
    pub setup_chapter_id: Option<Option<String>>,
    pub payoff_chapter_id: Option<Option<String>>,
}

#[tauri::command]
pub fn foreshadowing_link(
    db: State<'_, SharedDb>,
    payload: ForeshadowLinkPayload,
) -> CmdResult<ForeshadowLinkResult> {
    db.with_conn(|conn| {
        services::foreshadowing_link(
            conn,
            &payload.id,
            payload.setup_chapter_id.as_ref().map(|s| s.as_deref()),
            payload.payoff_chapter_id.as_ref().map(|p| p.as_deref()),
        )
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeshadowListPayload {
    pub project_id: String,
    pub status: Option<String>,
}

#[tauri::command]
pub fn foreshadowing_list(
    db: State<'_, SharedDb>,
    payload: ForeshadowListPayload,
) -> CmdResult<Vec<Foreshadowing>> {
    db.with_conn(|conn| {
        services::foreshadowing_list(conn, &payload.project_id, payload.status.as_deref())
    })
}

#[tauri::command]
pub fn foreshadowing_unresolved(
    db: State<'_, SharedDb>,
    payload: ProjectIdPayload,
) -> CmdResult<Vec<Foreshadowing>> {
    db.with_conn(|conn| services::foreshadowing_unresolved(conn, &payload.project_id))
}

// ---------- Stats ----------

#[tauri::command]
pub fn stats_overview(
    db: State<'_, SharedDb>,
    payload: ProjectIdPayload,
) -> CmdResult<StatsOverview> {
    db.with_conn(|conn| services::stats_overview(conn, &payload.project_id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsCalendarPayload {
    pub project_id: String,
    pub year: i32,
}

#[tauri::command]
pub fn stats_calendar(
    db: State<'_, SharedDb>,
    payload: StatsCalendarPayload,
) -> CmdResult<Vec<DayCount>> {
    db.with_conn(|conn| services::stats_calendar(conn, &payload.project_id, payload.year))
}

#[tauri::command]
pub fn stats_streak(db: State<'_, SharedDb>, payload: ProjectIdPayload) -> CmdResult<Streak> {
    db.with_conn(|conn| services::stats_streak(conn, &payload.project_id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalSetPayload {
    pub project_id: String,
    pub daily_words: Option<i64>,
    pub total_words: Option<i64>,
}

#[tauri::command]
pub fn goal_set(db: State<'_, SharedDb>, payload: GoalSetPayload) -> CmdResult<Goal> {
    db.with_conn(|conn| {
        services::goal_set(
            conn,
            &payload.project_id,
            payload.daily_words,
            payload.total_words,
        )
    })
}

#[tauri::command]
pub fn goal_get(db: State<'_, SharedDb>, payload: ProjectIdPayload) -> CmdResult<Option<Goal>> {
    db.with_conn(|conn| services::goal_get(conn, &payload.project_id))
}

// ---------- AI ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigSetPayload {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[tauri::command]
pub fn ai_config_set(payload: AiConfigSetPayload) -> CmdResult<()> {
    services::ai_config_set(&payload.base_url, &payload.api_key, &payload.model)
}

#[tauri::command]
pub fn ai_config_get() -> CmdResult<AiConfigPublic> {
    services::ai_config_get()
}

#[tauri::command]
pub async fn ai_complete(
    app: AppHandle,
    ai: State<'_, Arc<AiState>>,
    payload: services::AiCompletePayload,
) -> CmdResult<AiCompleteResult> {
    services::ai_complete(app, ai.inner().clone(), payload).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCancelPayload {
    pub request_id: String,
}

#[tauri::command]
pub fn ai_cancel(ai: State<'_, Arc<AiState>>, payload: AiCancelPayload) -> CmdResult<()> {
    ai.cancel(&payload.request_id);
    Ok(())
}

// ---------- Export ----------

#[tauri::command]
pub fn export_run(
    app: AppHandle,
    db: State<'_, SharedDb>,
    payload: services::ExportPayload,
) -> CmdResult<ExportResult> {
    db.with_conn(|conn| services::export_run(&app, conn, payload))
}
