use chrono::{DateTime, Utc};
/// Domain model representing a user in the application.
#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub created_by: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_by: Option<String>,
    pub modified_at: Option<DateTime<Utc>>,
    pub file_id: Option<String>,
    pub origin_file_name: Option<String>,
}
