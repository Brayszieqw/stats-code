#[derive(Debug, thiserror::Error)]
pub enum StatsCodeError {
    #[error("{0}")]
    Message(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type StatsCodeResult<T> = Result<T, StatsCodeError>;

impl StatsCodeError {
    #[cfg(test)]
    pub(crate) fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl From<String> for StatsCodeError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for StatsCodeError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::StatsCodeError;

    #[test]
    fn string_errors_preserve_display_message() {
        let error = StatsCodeError::from("analysis contract is missing");

        assert_eq!(error.to_string(), "analysis contract is missing");
    }

    #[test]
    fn json_errors_keep_source() {
        let json_error = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let error = StatsCodeError::from(json_error);

        assert!(matches!(error, StatsCodeError::Json(_)));
    }
}
