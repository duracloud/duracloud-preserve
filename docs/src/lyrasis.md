# Lyrasis hosting and support

DuraCloud is open source and freely available for anyone to deploy into their own AWS account. However, Lyrasis provides a hosted option for individuals or institutions wanting a managed service.

## Benefits

- Lyrasis manages an AWS account for you, which can be transferred to your ownership at any time with 30 days notice of cancelation of your hosting contract.
- Setup, configuration, and monitoring are fully handled by Lyrasis.
- You receive S3 access credentials to interact with DuraCloud using any S3 client.
  - Credentials can provide "full" or more "limited" access per user.
- Technical support is provided by experienced hosting staff.
- A simple web based option for file uploads is made available to you.

For pricing information and other details [...](#).

## Configuration

Here are details regarding configuration for hosted deployments:

### Lifecycle transitions

By default files uploaded to S3 move from standard to a lower storage tier after 7 days. It helps us to manage service costs if you can keep the file location stable before the transition occurs.

### Checksum report schedule

By default we run compute checksum jobs and generate checksum reports twice a year.

### Managed files

Files that are uploaded to the managed bucket are retained for 90 days. If you need to retain them for longer than that you must download them. Alternatively, you have the option of transferring files from the managed bucket to one of your user created buckets.

### Public access

The option to create buckets using the `-public` suffix is intended for light / infrequent usage and may be disabled by us if the incoming request load spikes. We strongly recommend using the CloudFront url and bucket for all public file access other than for exceptional cases. Please open a support ticket if you'd like to discuss options and use cases with us.

### Versioning

By default file versions are retained for 14 days.
