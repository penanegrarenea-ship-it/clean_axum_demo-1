// This module defines the `DeviceRepository` trait, which abstracts
// the database operations related to device management.

use crate::domains::device::dto::device_dto::{
    CreateDeviceDto, UpdateDeviceDto, UpdateManyDevicesDto,
};

use super::model::Device;

use async_trait::async_trait;
use sea_orm::{DatabaseConnection, DatabaseTransaction, DbErr};

#[async_trait]
/// Trait representing repository-level operations for device entities.
/// Provides an interface for data persistence and retrieval of device records.
pub trait DeviceRepository: Send + Sync {
    /// Retrieves all devices from the database.
    async fn find_all(&self, db: &DatabaseConnection) -> Result<Vec<Device>, DbErr>;

    /// Finds a device by its unique identifier.
    async fn find_by_id(
        &self,
        db: &DatabaseConnection,
        id: String,
    ) -> Result<Option<Device>, DbErr>;

    /// Creates a new device record in the database within the given transaction.
    async fn create(
        &self,
        tx: &DatabaseTransaction,
        device: CreateDeviceDto,
    ) -> Result<Device, DbErr>;

    /// Updates an existing device record with new data.
    async fn update(
        &self,
        tx: &DatabaseTransaction,
        id: String,
        device: UpdateDeviceDto,
    ) -> Result<Option<Device>, DbErr>;

    /// Updates multiple devices for a given user with the specified changes.
    async fn update_many(
        &self,
        tx: &DatabaseTransaction,
        user_id: String,
        modified_by: String,
        update_devices: UpdateManyDevicesDto,
    ) -> Result<(), DbErr>;

    /// Deletes a device record by its ID.
    async fn delete(&self, tx: &DatabaseTransaction, id: String) -> Result<bool, DbErr>;
}
