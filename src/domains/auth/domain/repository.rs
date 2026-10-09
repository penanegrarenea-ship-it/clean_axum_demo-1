//! This module defines the `UserAuthRepository` trait, which provides an abstraction
//! over database operations related to user authentication records.

use super::model::UserAuth;

use async_trait::async_trait;
use sea_orm::{DatabaseConnection, DatabaseTransaction, DbErr};

#[async_trait]
/// Trait representing the repository contract for user authentication data.
/// Enables decoupling of business logic from direct database interaction.
pub trait UserAuthRepository: Send + Sync {
    /// Finds a user authentication record by the user's username.
    /// Returns `Ok(Some(UserAuth))` if found, or `Ok(None)` if not found.
    async fn find_by_user_name(
        &self,
        db: &DatabaseConnection,
        user_name: String,
    ) -> Result<Option<UserAuth>, DbErr>;

    /// Inserts a new user authentication record into the database using a transaction.
    async fn create(&self, tx: &DatabaseTransaction, user_auth: UserAuth) -> Result<(), DbErr>;
}
