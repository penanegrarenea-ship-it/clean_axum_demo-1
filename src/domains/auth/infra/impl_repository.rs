use async_trait::async_trait;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait, JoinType,
    QueryFilter, QuerySelect, RelationTrait, Set,
};

use crate::domains::auth::domain::model::UserAuth;
use crate::domains::auth::domain::repository::UserAuthRepository;
use crate::infra::entities::{user_auth, users};

pub struct UserAuthRepo;

impl From<user_auth::Model> for UserAuth {
    fn from(m: user_auth::Model) -> Self {
        Self {
            user_id: m.user_id,
            password_hash: m.password_hash,
        }
    }
}

#[async_trait]
impl UserAuthRepository for UserAuthRepo {
    async fn find_by_user_name(
        &self,
        db: &DatabaseConnection,
        user_name: String,
    ) -> Result<Option<UserAuth>, DbErr> {
        // SELECT ua.* FROM user_auth ua JOIN users u ON ua.user_id = u.id WHERE u.username = $1
        let result = user_auth::Entity::find()
            .join(JoinType::InnerJoin, user_auth::Relation::Users.def())
            .filter(users::Column::Username.eq(user_name))
            .one(db)
            .await?;

        Ok(result.map(UserAuth::from))
    }

    async fn create(&self, tx: &DatabaseTransaction, user_auth: UserAuth) -> Result<(), DbErr> {
        // created_at / modified_at fall back to the column DEFAULT CURRENT_TIMESTAMP.
        let model = user_auth::ActiveModel {
            user_id: Set(user_auth.user_id),
            password_hash: Set(user_auth.password_hash),
            ..Default::default()
        };
        user_auth::Entity::insert(model)
            .exec_without_returning(tx)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! TDD: SeaORM-backed UserAuthRepo contract tests (run against `.env.test` DB).
    use super::*;
    use crate::infra::db::test_support::test_db;
    use crate::infra::entities::{user_auth, users};
    use sea_orm::{ActiveModelTrait, EntityTrait, Set, TransactionTrait};

    #[tokio::test]
    async fn find_by_user_name_returns_seeded_auth() {
        let db = test_db().await;
        let found = UserAuthRepo
            .find_by_user_name(&db, "apitest01".to_string())
            .await
            .unwrap()
            .expect("apitest01 has an auth row");
        assert_eq!(found.user_id, "00000000-0000-0000-0000-000000000021");
        assert!(found.password_hash.starts_with("$argon2"));
    }

    #[tokio::test]
    async fn find_by_user_name_unknown_is_none() {
        let db = test_db().await;
        let found = UserAuthRepo
            .find_by_user_name(&db, "no_such_user_xyz".to_string())
            .await
            .unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn create_inserts_within_transaction() {
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

        UserAuthRepo
            .create(
                &tx,
                UserAuth {
                    user_id: uid.clone(),
                    password_hash: "hash".into(),
                },
            )
            .await
            .unwrap();

        let row = user_auth::Entity::find_by_id(uid).one(&tx).await.unwrap();
        assert_eq!(row.unwrap().password_hash, "hash");
        tx.rollback().await.unwrap();
    }
}
