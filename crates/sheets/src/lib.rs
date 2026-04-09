//! Google Sheets availability sync. Populated in milestone 6.
//!
//! Intended shape: `AvailabilitySync::pull(hub, sheet_id, range, &Db)`
//! reads the shared club spreadsheet and upserts rows into `availability`.
//! Unknown rower names are logged, not fatal — scullers' attendance comes
//! through the same sheet and is stored as `AvailabilityStatus::ScullingOnly`.
