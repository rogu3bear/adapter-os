    #[error("{context}: {source}")]
    WithContext {
        context: String,
        #[source]
        source: Box<AosError>,
    },
}

// Rusqlite conversions removed to avoid conflicts with sqlx
// If needed, implement these conversions in aos-registry directly
