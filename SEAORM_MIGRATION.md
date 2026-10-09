# sqlx → SeaORM migration (branch `seaorm-rewrite`)

Base: `deps-upgrade-latest-stable` (commit `41170be`, baseline **24/24** with `SQLX_OFFLINE=true cargo test`).
Result: application has **no direct sqlx dependency**; all repositories + DB wiring use **SeaORM 2.0.4**.
Local only: nothing pushed.

## What changed

| Area | Before (sqlx 0.9) | After (SeaORM 2.0.4) |
|---|---|---|
| Cargo.toml | `sqlx` (postgres, runtime-tokio, macros, uuid, chrono, json) | `sea-orm = 2.0.4`, `default-features = false`, features `macros, sqlx-postgres, runtime-tokio, with-chrono, with-json, with-uuid` (same no-TLS runtime as before). sqlx 0.9 is now only a transitive dep of sea-orm. |
| Entities | none (raw SQL + `FromRow`) | `src/infra/entities/{users,devices,uploaded_files,user_auth}.rs`, hand-written from `db-seed/01-tables.sql` (TIMESTAMPTZ → `DateTimeUtc`, FK relations users↔devices/uploaded_files/user_auth) |
| Connection | `PgPoolOptions` in `common/config.rs` + `tests/test_helpers.rs` | `src/infra/db.rs::connect(&Config)` (`ConnectOptions` with same max/min pool sizes); `config::setup_database` keeps its 3× retry loop and returns `DatabaseConnection` |
| Wiring | `build_app_state(PgPool, Config)` | `build_app_state(DatabaseConnection, Config)`; `main.rs` and test helper updated |
| Repo traits (domain) | `PgPool` by value, `&mut Transaction<'_, Postgres>`, `sqlx::Error` | `&DatabaseConnection`, `&DatabaseTransaction`, `DbErr` |
| Service traits | `create_service(pool: PgPool, …)`; `FileServiceTrait::process_profile_picture_upload(tx: &mut Transaction…)` | `create_service(db: DatabaseConnection, …)`; `process_profile_picture_upload(tx: &DatabaseTransaction, …)` (user-create + profile-picture still share one transaction) |
| Domain models | `#[derive(FromRow)]` + manual `sqlx::Decode/Type` impls for `DeviceStatus`, `DeviceOS`, `FileType` | plain structs/enums (no ORM types in models); entity → domain mapping lives in infra (`TryFrom<devices::Model> for Device`, `TryFrom<uploaded_files::Model> for UploadedFile`, `From<user_auth::Model>`; bad enum strings → `DbErr::Type`) |
| Errors | `AppError::DatabaseError(#[from] sqlx::Error)` | `AppError::DatabaseError(#[from] sea_orm::DbErr)` (same message/500 status) |
| Offline data | `.sqlx/` (11 query files), `SQLX_OFFLINE=true` in Dockerfile | removed; `cargo test` needs no `SQLX_OFFLINE` |

Query mapping notes:
- **auth** `find_by_user_name`: `user_auth INNER JOIN users` via relation + `users.username = ?`.
- **user** list/get: one `user_select()` builder = `users LEFT JOIN uploaded_files ON uf.user_id = u.id AND uf.file_type = 'profile_picture'` (relation `on_condition`), projected into a private `UserRow` (`FromQueryResult`, `file_id` alias). Reused inside the transaction for `update` (generic over `ConnectionTrait`). `find_list` keeps `id = ?` / `username LIKE '%…%'` with blank filters ignored.
- **device** `update`: `update_many().col_expr(...)` only for provided fields + `modified_at = CURRENT_TIMESTAMP`; `update_many` (batch): `insert_many(...).on_conflict(OnConflict::column(id).update_columns([name,status,device_os,modified_by,modified_at]))` — same upsert semantics as the raw SQL.
- Inserts leave `created_at/modified_at` NotSet so the column `DEFAULT CURRENT_TIMESTAMP` applies (equivalent to the old `now()`/defaults).
- Transactions: `db.begin()` / `tx.commit()` / `tx.rollback()`; SeaORM's `DatabaseTransaction` rolls back on drop, same as sqlx.

Intentional, minor behaviour deltas (not covered by any HTTP test):
1. `POST /device` with `registered_at: null` now falls back to the column default (`now()`); the raw SQL inserted `NULL` into a NOT NULL column → 500.
2. `PUT /device/batch/{user_id}` with an empty `devices` list now returns success (no-op); the raw SQL emitted an invalid `INSERT … VALUES ON CONFLICT` → 500.
3. Batch upsert timestamps are bound as `DateTime<Utc>` (old code bound `naive_utc()` into TIMESTAMPTZ, which depended on the session time zone).

## TDD steps (each step: red → green → full suite → commit)

| # | Commit | Red (new failing tests) | Green |
|---|---|---|---|
| 0 | `84b1e36` | — baseline on new DB: **24/24** (`SQLX_OFFLINE=true cargo test`) | env only: `.env`/`.env.test` → `testdb_seaorm` |
| 1 | scaffold | `src/infra/tests.rs` (4 tests: ping + each entity reads seed) — failed to compile (no `infra::db`/`entities`) | add sea-orm, `infra::db::connect`, 4 entities → lib 6/6, suite 28 |
| 2 | auth | 3 tests in `auth/infra/impl_repository.rs` (lookup by username, unknown → None, create in tx) — type mismatch | UserAuthRepo on SeaORM; **dual-run**: bootstrap derived a SeaORM `DatabaseConnection` from the same sqlx pool (`SqlxPostgresConnector::from_sqlx_postgres_pool`) so un-migrated domains kept working → suite 31/31 |
| 3 | device | 3 tests (find/all; create→partial update→delete; batch upsert insert+update) | DeviceRepo/DeviceService on SeaORM → suite 34/34 |
| 4 | user + file | user: 2 tests (find_by_id/all/list filters; create → attach profile_picture + document → update returns joined profile file only → delete); file: 1 test (create/find/delete in tx, pool can't see uncommitted row) | migrated together because user-create passes its transaction into `FileService::process_profile_picture_upload` → suite 37/37 |
| 5 | remove sqlx | — | drop `sqlx` from Cargo.toml, `.sqlx/`, Dockerfile `SQLX_OFFLINE`, sqlx pool in config/test helper, temporary dual error variant; suite **37/37 without `SQLX_OFFLINE`** |

All write-path repo tests run inside a transaction that is rolled back, so they don't pollute the DB.

## Final verification

```bash
cd /workspace/clean_axum_demo_seaorm
cargo test            # SQLX_OFFLINE no longer needed
```
Result: **37 passed, 0 failed** = the original **24/24** (2 `hash_util` unit + 22 route integration tests: assets 3, auth 3, device 7, user 9) **+ 13 new SeaORM tests** (infra 4, auth 3, device 3, user 2, file 1).
Also verified on a freshly dropped/re-seeded DB (37/37), `cargo build --features opentelemetry` OK, `cargo clippy --all-targets` shows only the same 3 warnings as the baseline.

## DB setup used

- Postgres 17 on localhost:5432, independent DB **`testdb_seaorm`** (owner `testuser`/`pass`); `.env` and `.env.test` in this tree point to it. `testdb` / `testdb_upgrade` untouched.
- Re-create / re-seed:
  ```bash
  sudo -u postgres dropdb --if-exists testdb_seaorm
  sudo -u postgres createdb -O testuser testdb_seaorm
  for f in db-seed/01-tables.sql db-seed/02-seed.sql; do
    PGPASSWORD=pass psql -v ON_ERROR_STOP=1 -h localhost -U testuser -d testdb_seaorm -f "$f"; done
  ```
- `docker-compose.yml` still overrides `DATABASE_URL` to its own `testdb` container (unchanged).

## Remaining gaps / follow-ups

- Domain repository/service traits now name SeaORM types (`DatabaseConnection`, `DatabaseTransaction`, `DbErr`) — the same kind of leak as the old sqlx types. A unit-of-work abstraction would remove it but changes more API than this migration warranted.
- No `sea-orm-migration` crate: schema is still owned by `db-seed/*.sql` (entities hand-written to match).
- Route tests upload `cat.png` into `assets/private/profile_picture/` on every run (`cat(N).png` untracked files) — pre-existing behaviour, not cleaned up.
- README body still describes SQLx; only a pointer note was added at the top.
