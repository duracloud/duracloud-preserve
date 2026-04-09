-- Expose S3 inventory parquet files under data/inventory/ as a queryable view.
-- Run with: duckdb -init inventory.sql
--
-- S3 inventory emits one snapshot per day and each snapshot's manifest
-- references a parquet file under inventory/<bucket>/inventory/data/.
-- Every sync'd snapshot shows up in the glob, so an object that hasn't
-- changed will be reported once per snapshot. SELECT DISTINCT collapses
-- those repeats so basic queries see one row per unique observed state.
CREATE OR REPLACE VIEW inventory AS
SELECT DISTINCT
  bucket,
  key,
  size,
  last_modified_date,
  storage_class,
  replication_status
FROM read_parquet('data/inventory/**/data/*.parquet');
