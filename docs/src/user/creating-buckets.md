# Creating Buckets

> [!IMPORTANT]
> These instructions apply to all users, whether using Cyberduck, SFTPGo, the AWS CLI, or another S3-compatible client. The process is the same for everyone: upload a text file containing your bucket names to the `duracloud-$ID-request` bucket under the `buckets` folder. Instructions for each client are provided in the [Steps](#steps) section below.

## Create a Bucket

To create a bucket, you must create a text file (`.txt`) containing the names of up to **five** buckets you want to create.

## Naming Rules

- Bucket names are automatically prefixed with the stack name — **do not include the stack name in the file**.
- Each bucket name must be entered on its own line.
- Bucket names may contain only **alphanumeric characters** and `-`.
- Bucket names must **not** begin or end with `-`.
- Bucket names must be **no more than 63 characters total**, including:
  - The stack name prefix (`duracloud-$ID`)
  - 5 reserved characters for the `-repl` suffix

> [!Tip]
> Practically, this means your names should be no more than: 63 - 5 - (length of your duracloud-$ID)

### Public Bucket Naming

To create a **publicly accessible bucket**, the name must end with `-public`.

- This subtracts an additional **7 characters** from the maximum length.

## Reserved Prefixes and Suffixes

The following **cannot** be used in bucket names:

- `duracloud-` — already included as the foremost prefix
- `-logs` — used for access logging buckets
- `-managed` — used for system-managed buckets (reports, logs, and other system data appear here)
- `-repl` — used for replication target buckets (Amazon Glacier replication)
- `-request` — used for bucket request files

## Steps

1. Open a text editor (such as Notepad or Notepad++) and create a file containing your bucket names, one per line. Save it as a `.txt` file.

2. Upload the file to the `duracloud-$ID-request` bucket, inside the `buckets` folder.
   - If the `buckets` folder does not exist then create it first.
   - Buckets can only be created from files uploaded to the `buckets` folder in the request bucket.

### Cyberduck

1. Connect to your S3 account (see [Connecting to S3](./connecting-to-s3.md)).
2. Navigate to the `duracloud-$ID-request` bucket.
3. If a `buckets` folder does not exist, create one: **Action → New Folder**.
4. Open the `buckets` folder and drag your `.txt` file into the Cyberduck window, or click **Upload** to browse for it.
5. Cyberduck will show a transfer log confirming the upload.

> [!Tip]
> When re-using the same file with updated bucket names (Step 8 below), Cyberduck may ask you to confirm overwriting the existing file. Confirm to proceed.

### SFTPGo

1. Log in to the SFTPGo web interface (see [Connecting to S3](./connecting-to-s3.md)).
2. Navigate to your home folder. You will see `managed` and `public` folders — **do not** upload to these. Instead, navigate back to the root or look for a `request` folder corresponding to `duracloud-$ID-request`.
3. If a `buckets` folder does not exist inside the request area, click **New Folder** to create it.
4. Open the `buckets` folder, then click **Upload Files** or drag your `.txt` file into the upload area.
5. Click **Save** to complete the upload.

### AWS CLI

```bash
aws s3 cp mybuckets.txt s3://duracloud-$ID-request/buckets/mybuckets.txt
```

1. The file will be processed in the background and an attempt will be made to create each bucket.
   - Processing normally takes **0–2 minutes**.
2. A report file will be uploaded to the `feedback` folder inside the `-managed` bucket, providing details about the outcome.
3. Review the log when it becomes available.
4. Refresh your client view or reconnect to S3.
   - Successfully created buckets will now be visible.
   - Each new bucket will have an associated replication bucket with a `-repl` suffix.
   - Replication buckets are **list-only** (files cannot be downloaded).
5. The newly created buckets are now usable, and files can be uploaded.
6. To create more buckets:
   - Re-use and re-upload the same file with new bucket names, **or**
   - Create and upload an entirely new file. Both approaches work.

## Troubleshooting

- If you do not see any new buckets created, check the logs in the `$ID-managed` bucket `feedback` folder for error messages.
- If you attempt to create multiple buckets at one time and one bucket has an error (for example, the name is too long or you attempted to create more than five buckets), **none of the buckets will be created**. You must correct the issue and start again for all buckets.
