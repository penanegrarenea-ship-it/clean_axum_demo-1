//! Infrastructure layer shared by all domains: SeaORM connection setup and entities.

pub mod db;
pub mod entities;

#[cfg(test)]
mod tests;
