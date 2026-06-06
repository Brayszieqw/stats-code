//! Session lifecycle helpers: archival eligibility and purge scheduling.
//!
//! These pure functions determine when a session should be archived (inactive ≥ 24h)
//! and when an archived session's data should be purged (archived ≥ 24h).

use chrono::{DateTime, Duration, Utc};

use crate::models::{Session, SessionStatus};

/// Returns `true` if the session is eligible for archival.
///
/// A session is archivable when:
/// 1. Its status is `Active`, AND
/// 2. The elapsed time from `session.last_active_at` to `now` is ≥ 24 hours.
///
/// # Requirements
/// - R11.3: 24-hour inactivity timeout → archive
#[must_use]
pub fn is_archivable(session: &Session, now: DateTime<Utc>) -> bool {
    session.status == SessionStatus::Active
        && (now - session.last_active_at) >= Duration::hours(24)
}

/// Returns `true` if an archived session's data should be purged (deleted).
///
/// Purge is due when the elapsed time from `archived_at` to `now` is ≥ 24 hours.
///
/// # Requirements
/// - R13.3: Delete uploaded data within 24h after archival
#[must_use]
pub fn should_purge(archived_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    (now - archived_at) >= Duration::hours(24)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{SessionId, SessionSettings};
    use chrono::Duration;

    /// Helper to create a session with a given status and `last_active_at`.
    fn make_session(status: SessionStatus, last_active_at: DateTime<Utc>) -> Session {
        let now = Utc::now();
        Session {
            id: SessionId::new(),
            status,
            created_at: now,
            last_active_at,
            settings: SessionSettings::default(),
            messages: Vec::new(),
            datasets: Vec::new(),
            skill_runs: Vec::new(),
            uploaded_bytes: 0,
        }
    }

    // --- is_archivable tests ---

    #[test]
    fn active_session_exactly_24h_is_archivable() {
        let now = Utc::now();
        let last_active = now - Duration::hours(24);
        let session = make_session(SessionStatus::Active, last_active);
        assert!(is_archivable(&session, now));
    }

    #[test]
    fn active_session_over_24h_is_archivable() {
        let now = Utc::now();
        let last_active = now - Duration::hours(24) - Duration::minutes(1);
        let session = make_session(SessionStatus::Active, last_active);
        assert!(is_archivable(&session, now));
    }

    #[test]
    fn active_session_under_24h_is_not_archivable() {
        let now = Utc::now();
        let last_active = now - Duration::hours(23) - Duration::minutes(59);
        let session = make_session(SessionStatus::Active, last_active);
        assert!(!is_archivable(&session, now));
    }

    #[test]
    fn archived_session_is_never_archivable() {
        let now = Utc::now();
        let last_active = now - Duration::hours(48);
        let session = make_session(SessionStatus::Archived, last_active);
        assert!(!is_archivable(&session, now));
    }

    #[test]
    fn active_session_just_created_is_not_archivable() {
        let now = Utc::now();
        let session = make_session(SessionStatus::Active, now);
        assert!(!is_archivable(&session, now));
    }

    // --- should_purge tests ---

    #[test]
    fn exactly_24h_after_archive_should_purge() {
        let archived_at = Utc::now() - Duration::hours(24);
        let now = archived_at + Duration::hours(24);
        assert!(should_purge(archived_at, now));
    }

    #[test]
    fn over_24h_after_archive_should_purge() {
        let archived_at = Utc::now() - Duration::hours(25);
        let now = Utc::now();
        assert!(should_purge(archived_at, now));
    }

    #[test]
    fn under_24h_after_archive_should_not_purge() {
        let now = Utc::now();
        let archived_at = now - Duration::hours(23) - Duration::minutes(59);
        assert!(!should_purge(archived_at, now));
    }

    #[test]
    fn just_archived_should_not_purge() {
        let now = Utc::now();
        assert!(!should_purge(now, now));
    }

    #[test]
    fn one_second_before_24h_should_not_purge() {
        let now = Utc::now();
        let archived_at = now - Duration::hours(24) + Duration::seconds(1);
        assert!(!should_purge(archived_at, now));
    }
}
