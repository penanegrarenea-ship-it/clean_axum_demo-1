use async_trait::async_trait;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait, NotSet, QueryFilter,
    Set,
};
use std::str::FromStr;
use uuid::Uuid;

use crate::domains::device::domain::model::{Device, DeviceOS, DeviceStatus};
use crate::domains::device::domain::repository::DeviceRepository;
use crate::domains::device::dto::device_dto::{
    CreateDeviceDto, UpdateDeviceDto, UpdateManyDevicesDto,
};
use crate::infra::entities::devices;

pub struct DeviceRepo;

/// Maps a SeaORM row to the domain model; invalid enum strings surface as `DbErr::Type`
/// (mirrors the old sqlx `Decode` impls, which failed the same way).
impl TryFrom<devices::Model> for Device {
    type Error = DbErr;

    fn try_from(m: devices::Model) -> Result<Self, Self::Error> {
        Ok(Self {
            device_os: DeviceOS::from_str(&m.device_os).map_err(|e| DbErr::Type(e.to_string()))?,
            status: DeviceStatus::from_str(&m.status).map_err(|e| DbErr::Type(e.to_string()))?,
            id: m.id,
            user_id: m.user_id,
            name: m.name,
            registered_at: Some(m.registered_at),
            created_by: m.created_by,
            created_at: Some(m.created_at),
            modified_by: m.modified_by,
            modified_at: Some(m.modified_at),
        })
    }
}

#[async_trait]
impl DeviceRepository for DeviceRepo {
    async fn find_all(&self, db: &DatabaseConnection) -> Result<Vec<Device>, DbErr> {
        devices::Entity::find()
            .all(db)
            .await?
            .into_iter()
            .map(Device::try_from)
            .collect()
    }

    async fn find_by_id(
        &self,
        db: &DatabaseConnection,
        id: String,
    ) -> Result<Option<Device>, DbErr> {
        devices::Entity::find_by_id(id)
            .one(db)
            .await?
            .map(Device::try_from)
            .transpose()
    }

    async fn create(
        &self,
        tx: &DatabaseTransaction,
        device: CreateDeviceDto,
    ) -> Result<Device, DbErr> {
        let id = Uuid::new_v4().to_string();

        // created_at / modified_at use the column DEFAULT (CURRENT_TIMESTAMP == now()).
        let model = devices::ActiveModel {
            id: Set(id.clone()),
            user_id: Set(device.user_id),
            name: Set(device.name),
            status: Set(device.status.to_string()),
            device_os: Set(device.device_os.to_string()),
            registered_at: match device.registered_at {
                Some(ts) => Set(ts),
                None => NotSet,
            },
            created_by: Set(Some(device.modified_by.clone())),
            modified_by: Set(Some(device.modified_by)),
            ..Default::default()
        };
        devices::Entity::insert(model)
            .exec_without_returning(tx)
            .await?;

        let inserted = devices::Entity::find_by_id(id)
            .one(tx)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("inserted device".into()))?;
        Device::try_from(inserted)
    }

    async fn update(
        &self,
        tx: &DatabaseTransaction,
        id: String,
        device: UpdateDeviceDto,
    ) -> Result<Option<Device>, DbErr> {
        if devices::Entity::find_by_id(id.clone())
            .one(tx)
            .await?
            .is_none()
        {
            return Ok(None);
        }

        let mut update = devices::Entity::update_many()
            .col_expr(devices::Column::ModifiedAt, Expr::current_timestamp());

        if let Some(value) = device.user_id {
            update = update.col_expr(devices::Column::UserId, Expr::value(value));
        }
        if let Some(value) = device.name {
            update = update.col_expr(devices::Column::Name, Expr::value(value));
        }
        if let Some(value) = device.status {
            update = update.col_expr(devices::Column::Status, Expr::value(value.to_string()));
        }
        if let Some(value) = device.device_os {
            update = update.col_expr(devices::Column::DeviceOs, Expr::value(value.to_string()));
        }
        if let Some(value) = device.registered_at {
            update = update.col_expr(devices::Column::RegisteredAt, Expr::value(value));
        }

        update
            .col_expr(devices::Column::ModifiedBy, Expr::value(device.modified_by))
            .filter(devices::Column::Id.eq(id.clone()))
            .exec(tx)
            .await?;

        let updated = devices::Entity::find_by_id(id)
            .one(tx)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("updated device".into()))?;
        Ok(Some(Device::try_from(updated)?))
    }

    async fn update_many(
        &self,
        tx: &DatabaseTransaction,
        user_id: String,
        modified_by: String,
        update_devices: UpdateManyDevicesDto,
    ) -> Result<(), DbErr> {
        if update_devices.devices.is_empty() {
            // Nothing to upsert (the raw-SQL version emitted invalid SQL here).
            return Ok(());
        }

        let now = chrono::Utc::now();
        let models = update_devices
            .devices
            .into_iter()
            .map(|device| devices::ActiveModel {
                id: Set(device.id.unwrap_or_else(|| Uuid::new_v4().to_string())),
                user_id: Set(user_id.clone()),
                name: Set(device.name),
                status: Set(device.status.to_string()),
                device_os: Set(device.device_os.to_string()),
                registered_at: Set(now),
                created_by: Set(Some(modified_by.clone())),
                created_at: Set(now),
                modified_by: Set(Some(modified_by.clone())),
                modified_at: Set(now),
            });

        // INSERT ... ON CONFLICT (id) DO UPDATE SET name, status, device_os, modified_by, modified_at
        devices::Entity::insert_many(models)
            .on_conflict(
                OnConflict::column(devices::Column::Id)
                    .update_columns([
                        devices::Column::Name,
                        devices::Column::Status,
                        devices::Column::DeviceOs,
                        devices::Column::ModifiedBy,
                        devices::Column::ModifiedAt,
                    ])
                    .to_owned(),
            )
            .exec_without_returning(tx)
            .await?;

        Ok(())
    }

    async fn delete(&self, tx: &DatabaseTransaction, id: String) -> Result<bool, DbErr> {
        let res = devices::Entity::delete_by_id(id).exec(tx).await?;
        Ok(res.rows_affected > 0)
    }
}

#[cfg(test)]
mod tests {
    //! TDD: SeaORM-backed DeviceRepo contract tests. Writes run inside a rolled-back transaction.
    use super::*;
    use crate::domains::device::domain::model::{DeviceOS, DeviceStatus};
    use crate::domains::device::dto::device_dto::UpdateDeviceDtoWithIdDto;
    use crate::infra::db::test_support::test_db;
    use crate::infra::entities::devices;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};

    const USER01: &str = "00000000-0000-0000-0000-000000000001";

    fn new_device(name: &str) -> CreateDeviceDto {
        CreateDeviceDto {
            name: name.to_string(),
            user_id: USER01.to_string(),
            device_os: DeviceOS::IOS,
            status: DeviceStatus::Active,
            registered_at: Some(chrono::Utc::now()),
            modified_by: USER01.to_string(),
        }
    }

    #[tokio::test]
    async fn find_by_id_and_find_all() {
        let db = test_db().await;
        let d = DeviceRepo
            .find_by_id(&db, "00000000-0000-0000-0000-000000000005".into())
            .await
            .unwrap()
            .expect("seed device exists");
        assert_eq!(d.id, "00000000-0000-0000-0000-000000000005");
        assert!(d.registered_at.is_some());
        assert!(DeviceRepo
            .find_by_id(&db, "nope".into())
            .await
            .unwrap()
            .is_none());
        assert!(!DeviceRepo.find_all(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_update_delete_in_transaction() {
        let db = test_db().await;
        let tx = db.begin().await.unwrap();
        let name = format!("tdd-{}", uuid::Uuid::new_v4());

        let created = DeviceRepo.create(&tx, new_device(&name)).await.unwrap();
        assert_eq!(created.name, name);
        assert_eq!(created.user_id, USER01);
        assert_eq!(created.device_os, DeviceOS::IOS);
        assert_eq!(created.status, DeviceStatus::Active);
        assert_eq!(created.created_by.as_deref(), Some(USER01));

        // partial update: only status changes, name stays
        let updated = DeviceRepo
            .update(
                &tx,
                created.id.clone(),
                UpdateDeviceDto {
                    name: None,
                    user_id: None,
                    device_os: None,
                    status: Some(DeviceStatus::Blocked),
                    registered_at: None,
                    modified_by: "someone".into(),
                },
            )
            .await
            .unwrap()
            .expect("device exists");
        assert_eq!(updated.name, name);
        assert_eq!(updated.status, DeviceStatus::Blocked);
        assert_eq!(updated.modified_by.as_deref(), Some("someone"));

        assert!(DeviceRepo
            .update(
                &tx,
                "nope".into(),
                UpdateDeviceDto {
                    name: Some("x".into()),
                    user_id: None,
                    device_os: None,
                    status: None,
                    registered_at: None,
                    modified_by: "x".into(),
                },
            )
            .await
            .unwrap()
            .is_none());

        assert!(DeviceRepo.delete(&tx, created.id.clone()).await.unwrap());
        assert!(!DeviceRepo.delete(&tx, created.id).await.unwrap());
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn update_many_upserts() {
        let db = test_db().await;
        let tx = db.begin().await.unwrap();
        let existing = DeviceRepo
            .create(&tx, new_device(&format!("tdd-{}", uuid::Uuid::new_v4())))
            .await
            .unwrap();
        let new_name = format!("tdd-new-{}", uuid::Uuid::new_v4());

        DeviceRepo
            .update_many(
                &tx,
                USER01.into(),
                "batcher".into(),
                UpdateManyDevicesDto {
                    devices: vec![
                        UpdateDeviceDtoWithIdDto {
                            id: Some(existing.id.clone()),
                            name: "renamed".into(),
                            device_os: DeviceOS::Android,
                            status: DeviceStatus::Inactive,
                        },
                        UpdateDeviceDtoWithIdDto {
                            id: None,
                            name: new_name.clone(),
                            device_os: DeviceOS::IOS,
                            status: DeviceStatus::Pending,
                        },
                    ],
                },
            )
            .await
            .unwrap();

        let renamed = devices::Entity::find_by_id(existing.id.clone())
            .one(&tx)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(renamed.name, "renamed");
        assert_eq!(renamed.device_os, "Android");
        assert_eq!(renamed.status, "inactive");
        assert_eq!(renamed.modified_by.as_deref(), Some("batcher"));

        let inserted = devices::Entity::find()
            .filter(devices::Column::Name.eq(new_name))
            .one(&tx)
            .await
            .unwrap()
            .expect("new device inserted");
        assert_eq!(inserted.user_id, USER01);
        assert_eq!(inserted.status, "pending");
        tx.rollback().await.unwrap();
    }
}
