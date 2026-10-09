use crate::domains::file::{
    domain::{
        model::{FileType, UploadedFile},
        repository::FileRepository,
    },
    dto::file_dto::CreateFileDto,
};
use crate::infra::entities::uploaded_files;
use async_trait::async_trait;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait, QueryFilter, Set,
};
use std::str::FromStr;
use uuid::Uuid;

pub struct FileRepo;

/// Maps a SeaORM row to the domain model; an unknown `file_type` surfaces as `DbErr::Type`.
impl TryFrom<uploaded_files::Model> for UploadedFile {
    type Error = DbErr;

    fn try_from(m: uploaded_files::Model) -> Result<Self, Self::Error> {
        Ok(Self {
            file_type: FileType::from_str(&m.file_type).map_err(|e| DbErr::Type(e.to_string()))?,
            id: m.id,
            user_id: m.user_id,
            file_name: m.file_name,
            origin_file_name: m.origin_file_name,
            file_relative_path: m.file_relative_path,
            file_url: m.file_url,
            content_type: m.content_type,
            file_size: m.file_size,
            created_by: m.created_by,
            created_at: m.created_at,
            modified_by: m.modified_by,
            modified_at: m.modified_at,
        })
    }
}

#[async_trait]
impl FileRepository for FileRepo {
    async fn create_file(
        &self,
        tx: &DatabaseTransaction,
        file: CreateFileDto,
    ) -> Result<UploadedFile, DbErr> {
        let id = Uuid::new_v4().to_string();

        // user_id is NOT NULL in the schema; a missing id is rejected by the DB as before.
        let model = uploaded_files::ActiveModel {
            id: Set(id.clone()),
            user_id: match file.user_id {
                Some(user_id) => Set(user_id),
                None => sea_orm::NotSet,
            },
            file_name: Set(file.file_name),
            origin_file_name: Set(file.origin_file_name),
            file_relative_path: Set(file.file_relative_path),
            file_url: Set(file.file_url),
            content_type: Set(file.content_type),
            file_size: Set(file.file_size as i64),
            file_type: Set(file.file_type.to_string()),
            created_by: Set(Some(file.modified_by.clone())),
            modified_by: Set(Some(file.modified_by)),
            ..Default::default()
        };
        uploaded_files::Entity::insert(model)
            .exec_without_returning(tx)
            .await?;

        let inserted = uploaded_files::Entity::find_by_id(id)
            .one(tx)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("inserted file".into()))?;
        UploadedFile::try_from(inserted)
    }

    async fn find_by_user_id(
        &self,
        db: &DatabaseConnection,
        user_id: String,
    ) -> Result<Option<UploadedFile>, DbErr> {
        uploaded_files::Entity::find()
            .filter(uploaded_files::Column::UserId.eq(user_id))
            .one(db)
            .await?
            .map(UploadedFile::try_from)
            .transpose()
    }

    async fn find_by_id(
        &self,
        db: &DatabaseConnection,
        id: String,
    ) -> Result<Option<UploadedFile>, DbErr> {
        uploaded_files::Entity::find_by_id(id)
            .one(db)
            .await?
            .map(UploadedFile::try_from)
            .transpose()
    }

    async fn delete(&self, tx: &DatabaseTransaction, id: String) -> Result<bool, DbErr> {
        let result = uploaded_files::Entity::delete_by_id(id).exec(tx).await?;
        Ok(result.rows_affected > 0)
    }
}

#[cfg(test)]
mod tests {
    //! TDD: SeaORM-backed FileRepo contract tests. Writes run inside a rolled-back transaction.
    use super::*;
    use crate::domains::file::domain::model::FileType;
    use crate::infra::db::test_support::test_db;
    use crate::infra::entities::users;
    use sea_orm::{ActiveModelTrait, Set, TransactionTrait};

    #[tokio::test]
    async fn create_find_delete_in_transaction() {
        let db = test_db().await;
        let tx = db.begin().await.unwrap();
        let uid = uuid::Uuid::new_v4().to_string();
        users::ActiveModel {
            id: Set(uid.clone()),
            username: Set(format!("tdd_{}", &uid[..8])),
            email: Set("tdd@example.com".into()),
            ..Default::default()
        }
        .insert(&tx)
        .await
        .unwrap();

        let created = FileRepo
            .create_file(
                &tx,
                CreateFileDto {
                    user_id: Some(uid.clone()),
                    file_name: "a.png".into(),
                    origin_file_name: "orig.png".into(),
                    file_relative_path: "profile_picture/a.png".into(),
                    file_url: "/assets/private/profile/a.png".into(),
                    content_type: "image/png".into(),
                    file_size: 1234,
                    file_type: FileType::ProfilePicture,
                    modified_by: "tdd".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(created.user_id, uid);
        assert_eq!(created.file_size, 1234);
        assert!(matches!(created.file_type, FileType::ProfilePicture));
        assert_eq!(created.created_by.as_deref(), Some("tdd"));

        // reads through the pool don't see the uncommitted row
        assert!(FileRepo
            .find_by_id(&db, created.id.clone())
            .await
            .unwrap()
            .is_none());
        assert!(FileRepo
            .find_by_user_id(&db, uid.clone())
            .await
            .unwrap()
            .is_none());

        assert!(FileRepo.delete(&tx, created.id.clone()).await.unwrap());
        assert!(!FileRepo.delete(&tx, created.id).await.unwrap());
        tx.rollback().await.unwrap();
    }
}
