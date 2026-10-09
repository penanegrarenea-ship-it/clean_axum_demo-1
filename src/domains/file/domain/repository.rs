//! This module defines the `FileRepository` trait, which provides
//! an abstraction over database operations for managing uploaded files.

use crate::domains::file::dto::file_dto::CreateFileDto;

use super::model::UploadedFile;

use async_trait::async_trait;
use sea_orm::{DatabaseConnection, DatabaseTransaction, DbErr};

#[async_trait]
/// Trait representing repository-level operations for uploaded file metadata.
/// Enables persistence, retrieval, and deletion of file records through database interactions.
pub trait FileRepository {
    /// Inserts a new file record into the database using a transaction.
    async fn create_file(
        &self,
        tx: &DatabaseTransaction,
        file: CreateFileDto,
    ) -> Result<UploadedFile, DbErr>;

    /// Finds a file record by its unique identifier.
    async fn find_by_id(
        &self,
        db: &DatabaseConnection,
        id: String,
    ) -> Result<Option<UploadedFile>, DbErr>;

    /// Finds a file record associated with a specific user ID.
    async fn find_by_user_id(
        &self,
        db: &DatabaseConnection,
        user_id: String,
    ) -> Result<Option<UploadedFile>, DbErr>;

    /// Deletes a file record by its unique identifier using a transaction.
    async fn delete(&self, tx: &DatabaseTransaction, id: String) -> Result<bool, DbErr>;
}
