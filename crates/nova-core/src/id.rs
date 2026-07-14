use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// A strongly-typed identifier wrapper using UUIDv7 (time-ordered).
/// Provides type safety by parameterizing over the entity type.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id<T> {
    value: Uuid,
    #[serde(skip)]
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Id<T> {
    /// Create a new time-ordered ID (UUIDv7).
    #[must_use]
    pub fn new() -> Self {
        Self {
            value: Uuid::now_v7(),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Create an ID from an existing UUID.
    #[must_use]
    pub fn from_uuid(value: Uuid) -> Self {
        Self {
            value,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get the inner UUID value.
    #[must_use]
    pub fn into_inner(self) -> Uuid {
        self.value
    }

    /// Alias for `into_inner()` — returns the underlying UUID.
    #[must_use]
    pub fn into_uuid(self) -> Uuid {
        self.value
    }

    /// Parse from a string representation.
    pub fn parse(s: &str) -> std::result::Result<Self, uuid::Error> {
        let value = Uuid::parse_str(s)?;
        Ok(Self {
            value,
            _phantom: std::marker::PhantomData,
        })
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.value)
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl<T> From<Uuid> for Id<T> {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl<T> From<Id<T>> for Uuid {
    fn from(id: Id<T>) -> Self {
        id.value
    }
}

// SQLx integration
impl<T: Send + Sync> sqlx::Type<sqlx::Postgres> for Id<T> {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <Uuid as sqlx::Type<sqlx::Postgres>>::type_info()
    }
}

impl<'r, T: Send + Sync> sqlx::Decode<'r, sqlx::Postgres> for Id<T> {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> std::result::Result<Self, sqlx::error::BoxDynError> {
        let uuid = <Uuid as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(Self::from_uuid(uuid))
    }
}

impl<T: Send + Sync> sqlx::Encode<'_, sqlx::Postgres> for Id<T> {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <Uuid as sqlx::Encode<sqlx::Postgres>>::encode_by_ref(&self.value, buf)
    }
}
