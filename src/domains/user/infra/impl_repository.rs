use crate::domains::user::{
    domain::{model::User, repository::UserRepository},
    dto::user_dto::{CreateUserMultipartDto, SearchUserDto, UpdateUserDto},
};
use crate::infra::entities::{uploaded_files, users};
use async_trait::async_trait;

use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait, IntoCondition};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait,
    FromQueryResult, JoinType, QueryFilter, QuerySelect, RelationTrait, Select, Set,
};
use uuid::Uuid;

pub struct UserRepo;

/// Flat projection of `users LEFT JOIN uploaded_files (profile_picture)`.
#[derive(Debug, FromQueryResult)]
struct UserRow {
    id: String,
    username: String,
    email: Option<String>,
    created_by: Option<String>,
    created_at: Option<DateTime<Utc>>,
    modified_by: Option<String>,
    modified_at: Option<DateTime<Utc>>,
    file_id: Option<String>,
    origin_file_name: Option<String>,
}

impl From<UserRow> for User {
    fn from(r: UserRow) -> Self {
        Self {
            id: r.id,
            username: r.username,
            email: r.email,
            created_by: r.created_by,
            created_at: r.created_at,
            modified_by: r.modified_by,
            modified_at: r.modified_at,
            file_id: r.file_id,
            origin_file_name: r.origin_file_name,
        }
    }
}

/// SELECT u.*, uf.id AS file_id, uf.origin_file_name
///   FROM users u
///   LEFT JOIN uploaded_files uf ON uf.user_id = u.id AND uf.file_type = 'profile_picture'
fn user_select() -> Select<users::Entity> {
    users::Entity::find()
        .select_only()
        .columns([
            users::Column::Id,
            users::Column::Username,
            users::Column::Email,
            users::Column::CreatedBy,
            users::Column::CreatedAt,
            users::Column::ModifiedBy,
            users::Column::ModifiedAt,
        ])
        .column_as(uploaded_files::Column::Id, "file_id")
        .column(uploaded_files::Column::OriginFileName)
        .join(
            JoinType::LeftJoin,
            users::Relation::UploadedFiles
                .def()
                .on_condition(|_users, files| {
                    Expr::col((files, uploaded_files::Column::FileType))
                        .eq("profile_picture")
                        .into_condition()
                }),
        )
}

async fn find_user_info<C: ConnectionTrait>(conn: &C, id: String) -> Result<Option<User>, DbErr> {
    Ok(user_select()
        .filter(users::Column::Id.eq(id))
        .into_model::<UserRow>()
        .one(conn)
        .await?
        .map(User::from))
}

#[async_trait]
impl UserRepository for UserRepo {
    async fn find_all(&self, db: &DatabaseConnection) -> Result<Vec<User>, DbErr> {
        let users = user_select().into_model::<UserRow>().all(db).await?;
        Ok(users.into_iter().map(User::from).collect())
    }

    async fn find_list(
        &self,
        db: &DatabaseConnection,
        search_user_dto: SearchUserDto,
    ) -> Result<Vec<User>, DbErr> {
        let mut query = user_select();

        if let Some(s) = search_user_dto
            .id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            query = query.filter(users::Column::Id.eq(s));
        }

        if let Some(s) = search_user_dto
            .username
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            query = query.filter(users::Column::Username.like(format!("%{}%", s)));
        }

        let users = query.into_model::<UserRow>().all(db).await?;
        Ok(users.into_iter().map(User::from).collect())
    }

    async fn find_by_id(&self, db: &DatabaseConnection, id: String) -> Result<Option<User>, DbErr> {
        find_user_info(db, id).await
    }

    async fn create(
        &self,
        tx: &DatabaseTransaction,
        user: CreateUserMultipartDto,
    ) -> Result<String, DbErr> {
        let id = Uuid::new_v4().to_string();

        // created_at / modified_at use the column DEFAULT CURRENT_TIMESTAMP.
        let model = users::ActiveModel {
            id: Set(id.clone()),
            username: Set(user.username),
            email: Set(user.email),
            created_by: Set(Some(user.modified_by.clone())),
            modified_by: Set(Some(user.modified_by)),
            ..Default::default()
        };
        users::Entity::insert(model)
            .exec_without_returning(tx)
            .await?;

        Ok(id)
    }

    async fn update(
        &self,
        tx: &DatabaseTransaction,
        id: String,
        user: UpdateUserDto,
    ) -> Result<Option<User>, DbErr> {
        if find_user_info(tx, id.clone()).await?.is_none() {
            return Ok(None);
        }

        users::Entity::update_many()
            .col_expr(users::Column::Username, Expr::value(user.username))
            .col_expr(users::Column::Email, Expr::value(user.email))
            .col_expr(users::Column::ModifiedBy, Expr::value(user.modified_by))
            .col_expr(users::Column::ModifiedAt, Expr::current_timestamp())
            .filter(users::Column::Id.eq(id.clone()))
            .exec(tx)
            .await?;

        let updated = find_user_info(tx, id)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("updated user".into()))?;
        Ok(Some(updated))
    }

    async fn delete(&self, tx: &DatabaseTransaction, id: String) -> Result<bool, DbErr> {
        let res = users::Entity::delete_by_id(id).exec(tx).await?;
        Ok(res.rows_affected > 0)
    }
}

#[cfg(test)]
mod tests {
    //! TDD: SeaORM-backed UserRepo contract tests. Writes run inside a rolled-back transaction.
    use super::*;
    use crate::infra::db::test_support::test_db;
    use crate::infra::entities::uploaded_files;
    use sea_orm::{ActiveModelTrait, Set, TransactionTrait};

    fn create_dto(username: &str) -> CreateUserMultipartDto {
        CreateUserMultipartDto {
            username: username.to_string(),
            email: format!("{username}@example.com"),
            modified_by: "tdd".to_string(),
            profile_picture: None,
        }
    }

    #[tokio::test]
    async fn find_by_id_find_all_find_list() {
        let db = test_db().await;
        let u = UserRepo
            .find_by_id(&db, "00000000-0000-0000-0000-000000000001".into())
            .await
            .unwrap()
            .expect("user01 exists");
        assert_eq!(u.username, "user01");
        assert_eq!(u.email.as_deref(), Some("user01@example.com"));
        assert!(u.created_at.is_some());
        assert!(UserRepo
            .find_by_id(&db, "nope".into())
            .await
            .unwrap()
            .is_none());
        assert!(UserRepo.find_all(&db).await.unwrap().len() >= 21);

        let by_name = UserRepo
            .find_list(
                &db,
                SearchUserDto {
                    id: None,
                    username: Some("user0".into()),
                    email: None,
                },
            )
            .await
            .unwrap();
        assert!(by_name.iter().any(|u| u.username == "user01"));
        assert!(by_name.iter().all(|u| u.username.contains("user0")));

        let by_id = UserRepo
            .find_list(
                &db,
                SearchUserDto {
                    id: Some("00000000-0000-0000-0000-000000000002".into()),
                    username: Some("  ".into()), // blank filters are ignored
                    email: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(by_id.len(), 1);
        assert_eq!(by_id[0].username, "user02");
    }

    #[tokio::test]
    async fn create_update_delete_with_profile_picture_join() {
        let db = test_db().await;
        let tx = db.begin().await.unwrap();
        let name = format!("tdd_{}", &uuid::Uuid::new_v4().to_string()[..8]);

        let id = UserRepo.create(&tx, create_dto(&name)).await.unwrap();
        assert!(!id.is_empty());

        // attach a profile picture and a non-profile file; only the former must be joined
        for (fid, ftype) in [("pp", "profile_picture"), ("doc", "document")] {
            uploaded_files::ActiveModel {
                id: Set(format!("{fid}-{id}")[..36].to_string()),
                user_id: Set(id.clone()),
                file_name: Set(format!("{fid}.png")),
                origin_file_name: Set(format!("orig-{fid}.png")),
                file_relative_path: Set(format!("x/{fid}.png")),
                file_url: Set(format!("/x/{fid}.png")),
                content_type: Set("image/png".into()),
                file_size: Set(1),
                file_type: Set(ftype.into()),
                ..Default::default()
            }
            .insert(&tx)
            .await
            .unwrap();
        }

        let updated = UserRepo
            .update(
                &tx,
                id.clone(),
                UpdateUserDto {
                    username: format!("{name}_u"),
                    email: "changed@example.com".into(),
                    modified_by: "editor".into(),
                },
            )
            .await
            .unwrap()
            .expect("user exists");
        assert_eq!(updated.id, id);
        assert_eq!(updated.username, format!("{name}_u"));
        assert_eq!(updated.email.as_deref(), Some("changed@example.com"));
        assert_eq!(updated.created_by.as_deref(), Some("tdd"));
        assert_eq!(updated.modified_by.as_deref(), Some("editor"));
        assert_eq!(updated.origin_file_name.as_deref(), Some("orig-pp.png"));
        assert!(updated.file_id.unwrap().starts_with("pp-"));

        assert!(UserRepo
            .update(
                &tx,
                "nope".into(),
                UpdateUserDto {
                    username: "x".into(),
                    email: "x@example.com".into(),
                    modified_by: "x".into(),
                },
            )
            .await
            .unwrap()
            .is_none());

        assert!(!UserRepo.delete(&tx, "nope".into()).await.unwrap());
        tx.rollback().await.unwrap();

        // separate tx: delete a fresh user without dependants
        let tx = db.begin().await.unwrap();
        let id = UserRepo
            .create(&tx, create_dto(&format!("{name}_d")))
            .await
            .unwrap();
        assert!(UserRepo.delete(&tx, id).await.unwrap());
        tx.rollback().await.unwrap();
    }
}
