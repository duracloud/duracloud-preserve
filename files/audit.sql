-- Parse S3 server access logs under data/audit/ into a queryable view.
-- Run with: duckdb -init audit.sql
CREATE OR REPLACE VIEW audit AS
WITH raw AS (
  SELECT line
  FROM read_csv('data/audit/**/*', columns={'line': 'VARCHAR'}, delim='\x01', header=false)
),
parsed AS (
  SELECT regexp_extract(
    line,
    '^(\S+) (\S+) \[([^\]]+)\] (\S+) (\S+) (\S+) (\S+) (\S+) "([^"]*)" (\S+) (\S+) (\S+) (\S+) (\S+) (\S+) "([^"]*)" "([^"]*)" (\S+) (\S+) (\S+) (\S+) (\S+) (\S+) (\S+) (\S+)',
    ['bucket_owner','bucket','time_str','remote_ip','requester','request_id',
     'operation','key','request_uri','http_status','error_code','bytes_sent',
     'object_size','total_time','turn_around_time','referer','user_agent',
     'version_id','host_id','sig_v','cipher_suite','auth_type','host_header',
     'tls_version','access_point_arn']
  ) AS f
  FROM raw
)
SELECT
  f.bucket_owner,
  f.bucket,
  strptime(f.time_str, '%d/%b/%Y:%H:%M:%S %z')         AS event_time,
  f.remote_ip,
  f.requester,
  f.request_id,
  f.operation,
  nullif(f.key, '-')                                   AS key,
  f.request_uri,
  TRY_CAST(f.http_status AS INTEGER)                   AS http_status,
  nullif(f.error_code, '-')                            AS error_code,
  TRY_CAST(nullif(f.bytes_sent, '-') AS BIGINT)        AS bytes_sent,
  TRY_CAST(nullif(f.object_size, '-') AS BIGINT)       AS object_size,
  TRY_CAST(nullif(f.total_time, '-') AS INTEGER)       AS total_time_ms,
  TRY_CAST(nullif(f.turn_around_time, '-') AS INTEGER) AS turn_around_time_ms,
  nullif(f.referer, '-')                               AS referer,
  f.user_agent,
  nullif(f.version_id, '-')                            AS version_id,
  f.host_id,
  f.sig_v,
  f.cipher_suite,
  f.auth_type,
  f.host_header,
  f.tls_version,
  nullif(f.access_point_arn, '-')                      AS access_point_arn
FROM parsed;
