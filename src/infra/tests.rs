//! TDD scaffold tests: SeaORM connection + entity mapping against the seeded test DB
//! (`db-seed/01-tables.sql` + `db-seed/02-seed.sql`).

use sea_orm::{EntityTrait, PaginatorTrait};

use crate::infra::db::test_support::test_db;
use crate::infra::entities::{devices, uploaded_files, user_auth, users};

#[tokio::test]
async fn connects_and_pings() {
    let db = test_db().await;
    db.ping().await.expect("ping should succeed");
}

#[tokio::test]
async fn users_entity_reads_seed() {
    let db = test_db().await;
    let u = users::Entity::find_by_id("00000000-0000-0000-0000-000000000001")
        .one(&db)
        .await
        .unwrap()
        .expect("seed user01 must exist");
    assert_eq!(u.username, "user01");
    assert_eq!(u.email, "user01@example.com");
    assert!(users::Entity::find().count(&db).await.unwrap() >= 21);
}

#[tokio::test]
async fn devices_entity_reads_seed() {
    let db = test_db().await;
    // Seed device ids are fixed; some tests may delete/modify others, so look at device ...0005.
    let d = devices::Entity::find_by_id("00000000-0000-0000-0000-000000000005")
        .one(&db)
        .await
        .unwrap()
        .expect("seed device ...0005 must exist");
    assert!(!d.name.is_empty());
    assert!(!d.user_id.is_empty());
}

#[tokio::test]
async fn user_auth_and_uploaded_files_entities_query() {
    let db = test_db().await;
    let ua = user_auth::Entity::find_by_id("00000000-0000-0000-0000-000000000021")
        .one(&db)
        .await
        .unwrap()
        .expect("apitest01 auth row must exist");
    assert!(ua.password_hash.starts_with("$argon2"));
    // uploaded_files may be empty on a fresh seed; just make sure the mapping/query works.
    uploaded_files::Entity::find().all(&db).await.unwrap();
}
