//! Shared wire vocabulary for exact-file discoveries made during import.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Import boundary that observed an already-known raw file hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExactFileDiscoverySource {
    Upload,
    LibraryScan,
}

/// Error returned when persisted or wire data contains an unknown discovery source.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown exact-file discovery source: {0}")]
pub struct InvalidExactFileDiscoverySource(String);

impl ExactFileDiscoverySource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Upload => "upload",
            Self::LibraryScan => "library_scan",
        }
    }
}

impl FromStr for ExactFileDiscoverySource {
    type Err = InvalidExactFileDiscoverySource;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "upload" => Ok(Self::Upload),
            "library_scan" => Ok(Self::LibraryScan),
            _ => Err(InvalidExactFileDiscoverySource(value.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_file_discovery_sources_have_stable_wire_names() {
        assert_eq!(ExactFileDiscoverySource::Upload.as_str(), "upload");
        assert_eq!(
            serde_json::to_value(ExactFileDiscoverySource::LibraryScan)
                .expect("source should serialize"),
            serde_json::json!("library_scan")
        );
    }

    #[test]
    fn exact_file_discovery_sources_parse_strictly() {
        assert_eq!(
            "upload".parse::<ExactFileDiscoverySource>(),
            Ok(ExactFileDiscoverySource::Upload)
        );
        assert_eq!(
            "library_scan".parse::<ExactFileDiscoverySource>(),
            Ok(ExactFileDiscoverySource::LibraryScan)
        );

        let error = "filesystem"
            .parse::<ExactFileDiscoverySource>()
            .unwrap_err();
        assert_eq!(
            error,
            InvalidExactFileDiscoverySource("filesystem".to_string())
        );
        assert_eq!(
            error.to_string(),
            "unknown exact-file discovery source: filesystem"
        );
    }
}
