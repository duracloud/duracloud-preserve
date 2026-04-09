# Query audit and inventory data

S3 audit logs and inventory can be synced locally for ad-hoc querying with [DuckDB](https://duckdb.org).

## Pre-reqs

- [AWS cli](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html)
- [DuckDB](https://duckdb.org/install)

## Sync the files

Download audit and / or inventory data to a local `./data` folder. For example:

```bash
mkdir -p data/audit/
mkdir -p data/inventory/

aws s3 sync s3://${stack_name}-managed/audit/ data/audit/
aws s3 sync s3://${stack_name}-managed/manifests/ data/inventory/

# also download the query setup sql files
curl -O https://artifacts.preserve.duracloud.org/query/audit.sql
curl -O https://artifacts.preserve.duracloud.org/query/inventory.sql

```

## Query audit data with DuckDB

The log files are in the [S3 server access log format][1]: one request per
line, space-delimited, with a bracketed timestamp and quoted `request_uri`,
`referer`, and `user_agent`. DuckDB's CSV reader can't handle the mixed
quoting, so `audit.sql` reads each line as a single string and pulls fields
out with a regex, exposing them as the `audit` view.

Launch the DuckDB CLI with the view preloaded:

```bash
duckdb -init audit.sql
```

Then query away. For example, every request ordered by time:

```sql
SELECT event_time, bucket, remote_ip, operation, key, http_status, bytes_sent
FROM audit
ORDER BY event_time;
```

### Standard object operations by users

The `requester` field is an IAM ARN. Most traffic is programmatic (for
example SDK sessions named `aws-go-sdk-…`, service roles doing replication
or batch work etc.) but when a user assumes a role via a named profile, the
session name at the end of the ARN is usually the IAM username. To see
just the standard object-level operations (`GET`, `PUT`, `DELETE`) performed
by assumed-role sessions, with the obvious programmatic sessions filtered out:

```sql
SELECT
  event_time,
  regexp_extract(requester, 'assumed-role/[^/]+/(.+)$', 1) AS who,
  bucket,
  operation,
  key,
  http_status
FROM audit
WHERE operation IN ('REST.PUT.OBJECT', 'REST.GET.OBJECT', 'REST.DELETE.OBJECT')
  AND requester LIKE '%:assumed-role/%'
  AND requester NOT LIKE '%aws-go-sdk-%'
  AND requester NOT LIKE '%assume-role-from-profile-%'
ORDER BY event_time;
```

Service roles (e.g. replication, batch jobs) may still appear in the
results. Inspect the `who` column and add further `NOT LIKE` clauses
for any session names that aren't people of interest.

## Query inventory data with DuckDB

S3 inventory reports ship as Parquet, which DuckDB reads natively.
`inventory.sql` globs every parquet file under `data/inventory/` and
exposes them as the `inventory` view. Because each daily snapshot
re-reports objects that haven't changed, the view uses `SELECT DISTINCT`
to collapse identical rows so basic queries see one row per unique
observed state.

Launch the DuckDB CLI with the view preloaded:

```bash
duckdb -init inventory.sql
```

List every object across all buckets:

```sql
SELECT bucket, key, size, last_modified_date, storage_class
FROM inventory
ORDER BY bucket, key;
```

Object count and total bytes per bucket:

```sql
SELECT bucket, COUNT(*) AS objects, SUM(size) AS total_bytes
FROM inventory
GROUP BY bucket
ORDER BY bucket;
```

To work with both views in the same session, pass both scripts:

```bash
duckdb -init audit.sql -cmd ".read inventory.sql"
```

[1]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/LogFormat.html
