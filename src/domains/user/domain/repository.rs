//! This module defines the `UserRepository` trait, which abstracts
//! the database operations related to user entities.

use crate::domains::user::dto::user_dto::{CreateUserMultipartDto, SearchUserDto, UpdateUserDto};

use super::model::User;

use async_trait::async_trait;
use sea_orm::{DatabaseConnection, DatabaseTransaction, DbErr};

#[async_trait]
/// Trait representing repository-level operations for user entities.
/// Provides methods for creating, retrieving, updating, and deleting users in the database.
pub trait UserRepository: Send + Sync {
    /// Retrieves all users from the database.
    async fn find_all(&self, db: &DatabaseConnection) -> Result<Vec<User>, DbErr>;

    /// Finds a user by their unique identifier.
    async fn find_by_id(&self, db: &DatabaseConnection, id: String) -> Result<Option<User>, DbErr>;

    /// Finds user list by condition
    async fn find_list(
        &self,
        db: &DatabaseConnection,
        search_user_dto: SearchUserDto,
    ) -> Result<Vec<User>, DbErr>;

    /// Creates a new user record using the provided data within an active transaction.
    async fn create(
        &self,
        tx: &DatabaseTransaction,
        user: CreateUserMultipartDto,
    ) -> Result<String, DbErr>;

    /// Updates an existing user record using the provided data.
    async fn update(
        &self,
        tx: &DatabaseTransaction,
        id: String,
        user: UpdateUserDto,
    ) -> Result<Option<User>, DbErr>;

    /// Deletes a user by their unique identifier within an active transaction.
    async fn delete(&self, tx: &DatabaseTransaction, id: String) -> Result<bool, DbErr>;
}
