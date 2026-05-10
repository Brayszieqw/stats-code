use crate::schema::{AnalysisCheckItem, AnalysisCheckLevel};

pub(super) fn push_check(
    items: &mut Vec<AnalysisCheckItem>,
    level: AnalysisCheckLevel,
    code: &str,
    message: impl Into<String>,
) {
    items.push(AnalysisCheckItem {
        level,
        code: code.to_string(),
        message: message.into(),
    });
}
