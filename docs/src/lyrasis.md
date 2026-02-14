# Lyrasis hosting and support

Details regarding configuration for hosted deployments:

## Lifecycle transitions

By default files uploaded to S3 move from standard to a lower storage tier after 7 days. It helps us to manage service costs if you can keep the file location stable before the transition occurs.

## Checksum report schedule

By default we run compute checksum jobs and generate checksum reports twice a year.

## Managed files

Files that are uploaded to the managed bucket are retained for 90 days. If you need to retain them for longer than that you must download them. Alternatively, you have the option of transferring files from the managed bucket to one of your user created buckets.

## Public access

The option to create buckets using the `-public` suffix is intended for light / infrequent usage and may be disabled by us if the incoming request load spikes. We strongly recommend using the CloudFront url and bucket for all public file access other than for exceptional cases. Please open a support ticket if you'd like to discuss options and use cases with us.

## Versioning

By default file versions are retained for 14 days.
