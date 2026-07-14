use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryAccess {
    Read,
    Write,
    Manage,
}

impl LibraryAccess {
    pub(crate) fn permission_predicate(self, alias: &str) -> String {
        match self {
            Self::Read => format!("({alias}.can_read OR {alias}.can_write OR {alias}.can_manage)"),
            Self::Write => format!("({alias}.can_write OR {alias}.can_manage)"),
            Self::Manage => format!("{alias}.can_manage"),
        }
    }
}

pub(crate) fn auth_user_id(auth: &AuthUser) -> ApiResult<Uuid> {
    Uuid::parse_str(&auth.id).map_err(|_| ApiError::unauthorized())
}

async fn user_role(state: &AppState, user_id: Uuid) -> ApiResult<Option<String>> {
    sqlx::query_scalar("SELECT role::text FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)
}

pub(crate) async fn is_admin(state: &AppState, auth: &AuthUser) -> ApiResult<bool> {
    let user_id = auth_user_id(auth)?;
    Ok(user_role(state, user_id).await?.as_deref() == Some("admin"))
}

pub(crate) async fn ensure_library_access(
    state: &AppState,
    auth: &AuthUser,
    library_id: Uuid,
    access: LibraryAccess,
) -> ApiResult<()> {
    let user_id = auth_user_id(auth)?;
    let role = user_role(state, user_id).await?;

    if role.as_deref() == Some("admin") {
        return Ok(());
    }

    let (sql, bind_guest) = library_access_sql(access);

    let mut query = sqlx::query_scalar(&sql).bind(library_id).bind(user_id);
    if bind_guest {
        query = query.bind(role.as_deref() == Some("guest"));
    }

    let allowed: bool = query.fetch_one(&state.db).await.map_err(ApiError::from)?;

    if allowed {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}

/// Returns `None` for admins (unrestricted), or a concrete set of visible
/// library IDs for non-admin users.
pub(crate) async fn visible_library_ids(
    state: &AppState,
    auth: &AuthUser,
    access: LibraryAccess,
) -> ApiResult<Option<Vec<Uuid>>> {
    let user_id = auth_user_id(auth)?;
    let role = user_role(state, user_id).await?;

    if role.as_deref() == Some("admin") {
        return Ok(None);
    }

    let (sql, bind_guest) = visible_libraries_sql(access);

    let mut query = sqlx::query_scalar::<_, Uuid>(&sql).bind(user_id);
    if bind_guest {
        query = query.bind(role.as_deref() == Some("guest"));
    }

    let ids = query.fetch_all(&state.db).await.map_err(ApiError::from)?;

    Ok(Some(ids))
}

pub(crate) async fn book_library_id(state: &AppState, book_id: Uuid) -> ApiResult<Option<Uuid>> {
    sqlx::query_scalar("SELECT library_id FROM books WHERE id = $1")
        .bind(book_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound("Book not found".to_string()))
}

pub(crate) async fn ensure_book_access(
    state: &AppState,
    auth: &AuthUser,
    book_id: Uuid,
    access: LibraryAccess,
) -> ApiResult<()> {
    match book_library_id(state, book_id).await? {
        Some(library_id) => ensure_library_access(state, auth, library_id, access).await,
        None if is_admin(state, auth).await? => Ok(()),
        None => Err(ApiError::forbidden()),
    }
}

pub(crate) async fn default_library_id(state: &AppState) -> ApiResult<Option<Uuid>> {
    sqlx::query_scalar("SELECT id FROM libraries WHERE is_default = true LIMIT 1")
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)
}

fn library_access_sql(access: LibraryAccess) -> (String, bool) {
    let direct_predicate = access.permission_predicate("lp");
    let group_predicate = access.permission_predicate("lgp");
    let bind_guest = access == LibraryAccess::Read;
    let open_read_clause = if bind_guest {
        "OR (
            NOT EXISTS (SELECT 1 FROM library_permissions lp WHERE lp.library_id = l.id)
            AND NOT EXISTS (SELECT 1 FROM library_group_permissions lgp WHERE lgp.library_id = l.id)
        )
        OR ($3 = true AND COALESCE((l.features->>'allow_guests')::boolean, false) = true)"
    } else {
        ""
    };

    (
        format!(
            "SELECT EXISTS (
                SELECT 1 FROM libraries l
                WHERE l.id = $1 AND (
                    EXISTS (
                        SELECT 1 FROM library_permissions lp
                        WHERE lp.library_id = l.id
                          AND lp.user_id = $2
                          AND {direct_predicate}
                    )
                    OR EXISTS (
                        SELECT 1
                        FROM library_group_permissions lgp
                        JOIN group_members gm ON gm.group_id = lgp.group_id
                        WHERE lgp.library_id = l.id
                          AND gm.user_id = $2
                          AND {group_predicate}
                    )
                    {open_read_clause}
                )
            )"
        ),
        bind_guest,
    )
}

fn visible_libraries_sql(access: LibraryAccess) -> (String, bool) {
    let direct_predicate = access.permission_predicate("lp");
    let group_predicate = access.permission_predicate("lgp");
    let bind_guest = access == LibraryAccess::Read;
    let open_read_clause = if bind_guest {
        "OR (
            NOT EXISTS (SELECT 1 FROM library_permissions lp WHERE lp.library_id = l.id)
            AND NOT EXISTS (SELECT 1 FROM library_group_permissions lgp WHERE lgp.library_id = l.id)
        )
        OR ($2 = true AND COALESCE((l.features->>'allow_guests')::boolean, false) = true)"
    } else {
        ""
    };

    (
        format!(
            "SELECT l.id
             FROM libraries l
             WHERE
                EXISTS (
                    SELECT 1 FROM library_permissions lp
                    WHERE lp.library_id = l.id
                      AND lp.user_id = $1
                      AND {direct_predicate}
                )
                OR EXISTS (
                    SELECT 1
                    FROM library_group_permissions lgp
                    JOIN group_members gm ON gm.group_id = lgp.group_id
                    WHERE lgp.library_id = l.id
                      AND gm.user_id = $1
                      AND {group_predicate}
                )
                {open_read_clause}"
        ),
        bind_guest,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_access_sql_does_not_bind_missing_guest_placeholder() {
        let (sql, bind_guest) = library_access_sql(LibraryAccess::Write);

        assert!(!sql.contains("$3"));
        assert!(!bind_guest);
    }

    #[test]
    fn read_access_sql_keeps_guest_placeholder() {
        let (sql, bind_guest) = library_access_sql(LibraryAccess::Read);

        assert!(sql.contains("$3"));
        assert!(bind_guest);
    }

    #[test]
    fn visible_write_sql_does_not_bind_missing_guest_placeholder() {
        let (sql, bind_guest) = visible_libraries_sql(LibraryAccess::Write);

        assert!(!sql.contains("$2 = true"));
        assert!(!bind_guest);
    }
}
