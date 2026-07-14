use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        // Friend system
        .route("/social/friends", get(list_friends).post(send_friend_request))
        .route("/social/friends/{id}/accept", post(accept_friend))
        .route("/social/friends/{id}/reject", post(reject_friend))
        .route("/social/friends/{id}", delete(remove_friend))
        // Activity feed
        .route("/social/feed", get(activity_feed))
        // Shared shelves
        .route("/social/shared-shelves", get(list_shared_shelves).post(create_shared_shelf))
        .route("/social/shared-shelves/{id}", get(get_shared_shelf))
        .route("/social/shared-shelves/{id}/books", post(add_to_shared_shelf))
        // Reading challenges
        .route("/social/challenges", get(list_challenges).post(create_challenge))
        .route("/social/challenges/{id}/join", post(join_challenge))
}

// ─── Friends ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct FriendRequest {
    target_user_id: Uuid,
    message: Option<String>,
}

async fn list_friends(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let friends = sqlx::query!(
        r#"SELECT f.id, f.friend_id, u.username, u.avatar_url, f.status, f.created_at
           FROM friendships f
           JOIN users u ON u.id = f.friend_id
           WHERE f.user_id = $1 AND f.status = 'accepted'
           ORDER BY u.username"#,
        Uuid::nil() // TODO: extract from auth context
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "friends": friends, "total": friends.len() })))
}

async fn send_friend_request(
    State(state): State<Arc<AppState>>,
    Json(body): Json<FriendRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let id = Uuid::new_v4();
    sqlx::query!(
        r#"INSERT INTO friendships (id, user_id, friend_id, status, message)
           VALUES ($1, $2, $3, 'pending', $4)"#,
        id,
        Uuid::nil(), // TODO: from auth
        body.target_user_id,
        body.message,
    )
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "id": id,
        "status": "pending",
        "target_user_id": body.target_user_id,
    })))
}

async fn accept_friend(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!(
        "UPDATE friendships SET status = 'accepted' WHERE id = $1",
        id
    )
    .execute(&state.db)
    .await?;
    Ok(Json(serde_json::json!({ "id": id, "status": "accepted" })))
}

async fn reject_friend(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!(
        "UPDATE friendships SET status = 'rejected' WHERE id = $1",
        id
    )
    .execute(&state.db)
    .await?;
    Ok(Json(serde_json::json!({ "id": id, "status": "rejected" })))
}

async fn remove_friend(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!("DELETE FROM friendships WHERE id = $1", id)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

// ─── Activity Feed ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct FeedQuery {
    #[serde(default = "default_feed_limit")]
    limit: i64,
    before: Option<String>,
}

fn default_feed_limit() -> i64 { 20 }

async fn activity_feed(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FeedQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let limit = params.limit.min(50);

    // Get activity from friends
    let activities = sqlx::query!(
        r#"SELECT a.id, a.user_id, u.username, a.activity_type, a.book_id, 
           b.title as book_title, a.details, a.created_at
           FROM user_activities a
           JOIN users u ON u.id = a.user_id
           LEFT JOIN books b ON b.id = a.book_id
           WHERE a.user_id IN (
               SELECT friend_id FROM friendships 
               WHERE user_id = $1 AND status = 'accepted'
           )
           ORDER BY a.created_at DESC
           LIMIT $2"#,
        Uuid::nil(), // TODO: from auth
        limit
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "feed": activities,
        "total": activities.len(),
    })))
}

// ─── Shared Shelves ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateSharedShelfRequest {
    name: String,
    description: Option<String>,
    /// User IDs to share with
    shared_with: Vec<Uuid>,
    is_public: Option<bool>,
}

async fn list_shared_shelves(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let shelves = sqlx::query!(
        r#"SELECT s.id, s.name, s.description, s.owner_id, s.is_public,
           COUNT(sb.book_id) as book_count, s.created_at
           FROM shared_shelves s
           LEFT JOIN shared_shelf_books sb ON sb.shelf_id = s.id
           WHERE s.owner_id = $1 OR s.id IN (
               SELECT shelf_id FROM shared_shelf_members WHERE user_id = $1
           )
           GROUP BY s.id
           ORDER BY s.created_at DESC"#,
        Uuid::nil()
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "shelves": shelves, "total": shelves.len() })))
}

async fn create_shared_shelf(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSharedShelfRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let id = Uuid::new_v4();
    let owner_id = Uuid::nil(); // TODO: from auth

    sqlx::query!(
        r#"INSERT INTO shared_shelves (id, name, description, owner_id, is_public)
           VALUES ($1, $2, $3, $4, $5)"#,
        id,
        body.name,
        body.description,
        owner_id,
        body.is_public.unwrap_or(false)
    )
    .execute(&state.db)
    .await?;

    // Add members
    for user_id in &body.shared_with {
        sqlx::query!(
            "INSERT INTO shared_shelf_members (shelf_id, user_id) VALUES ($1, $2)",
            id, user_id
        )
        .execute(&state.db)
        .await?;
    }

    Ok(Json(serde_json::json!({
        "id": id,
        "name": body.name,
        "shared_with": body.shared_with.len(),
    })))
}

async fn get_shared_shelf(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let shelf = sqlx::query!(
        r#"SELECT s.id, s.name, s.description, s.owner_id, s.is_public, s.created_at
           FROM shared_shelves s WHERE s.id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?;

    let books = sqlx::query!(
        r#"SELECT b.id, b.title, b.author, b.cover_url
           FROM shared_shelf_books sb
           JOIN books b ON b.id = sb.book_id
           WHERE sb.shelf_id = $1"#,
        id
    )
    .fetch_all(&state.db)
    .await?;

    match shelf {
        Some(s) => Ok(Json(serde_json::json!({
            "shelf": s,
            "books": books,
        }))),
        None => Err(crate::error::ApiError::not_found("Shared shelf not found")),
    }
}

#[derive(Debug, Deserialize)]
struct AddBookToShelfRequest {
    book_id: Uuid,
    note: Option<String>,
}

async fn add_to_shared_shelf(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddBookToShelfRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!(
        r#"INSERT INTO shared_shelf_books (shelf_id, book_id, added_by, note)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (shelf_id, book_id) DO NOTHING"#,
        id,
        body.book_id,
        Uuid::nil(), // TODO: from auth
        body.note,
    )
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "added": true, "shelf_id": id, "book_id": body.book_id })))
}

// ─── Reading Challenges ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateChallengeRequest {
    name: String,
    description: Option<String>,
    /// Target number of books
    target_books: i32,
    /// Duration in days
    duration_days: i32,
}

async fn list_challenges(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let challenges = sqlx::query!(
        r#"SELECT c.id, c.name, c.description, c.target_books, c.duration_days,
           c.start_date, c.owner_id, COUNT(cp.user_id) as participants
           FROM reading_challenges c
           LEFT JOIN challenge_participants cp ON cp.challenge_id = c.id
           GROUP BY c.id
           ORDER BY c.start_date DESC"#
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "challenges": challenges })))
}

async fn create_challenge(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateChallengeRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let id = Uuid::new_v4();
    sqlx::query!(
        r#"INSERT INTO reading_challenges (id, name, description, target_books, duration_days, owner_id, start_date)
           VALUES ($1, $2, $3, $4, $5, $6, now())"#,
        id,
        body.name,
        body.description,
        body.target_books,
        body.duration_days,
        Uuid::nil(),
    )
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "id": id,
        "name": body.name,
        "target_books": body.target_books,
        "duration_days": body.duration_days,
    })))
}

async fn join_challenge(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!(
        r#"INSERT INTO challenge_participants (challenge_id, user_id, books_read)
           VALUES ($1, $2, 0)
           ON CONFLICT (challenge_id, user_id) DO NOTHING"#,
        id,
        Uuid::nil(),
    )
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "joined": true, "challenge_id": id })))
}
