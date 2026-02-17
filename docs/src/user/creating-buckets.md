# Creating Buckets
> [!IMPORTANT]
> **Note:** These instructions apply if you are directly creating buckets (for example, via the command line or Cyberduck). If you are using SFTPGo, you are **not** creating buckets directly. Instead, you are creating folder structures within the `-private` and `-public` buckets that have already been created for you.

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
> [!Tip] Practically, this means your names should be no more than: 63 - 5 - (length of your duracloud-$ID)

### Public Bucket Naming
To create a **publicly accessible bucket**, the name must end with `-public`.
- This subtracts an additional **7 characters** from the maximum length.

## Reserved Prefixes and Suffixes
The following **cannot** be used in bucket names:
- `duracloud-` — already included as the foremost prefix
- `-bucket-requested` — used for bucket request files
- `-logs` — used for access logging buckets
- `-managed` — used for system-managed buckets (reports, logs, and other system data appear here)
- `-repl` — used for replication target buckets (Amazon Glacier replication)

## Steps
1. Open a text editor (such as Notepad or Notepad++) and decide on your bucket names.
2. Upload your text file to the `duracloud-$ID-bucket-requested` bucket.
3. The file will be processed in the background and an attempt will be made to create each bucket.
   - Processing normally takes **0–2 minutes**.
4. A report file will be uploaded to the `logs` folder inside the `-managed` bucket, providing details about the outcome.
5. Review the log when it becomes available.
6. Refresh your client view or reconnect to S3.
   - Successfully created buckets will now be visible.
   - Each new bucket will have an associated replication bucket with a `-repl` suffix.
   - Replication buckets are **list-only** (files cannot be downloaded).
7. The newly created buckets are now usable, and files can be uploaded.
8. To create more buckets:
   - Re-use and re-upload the same file with new bucket names, **or**
   - Create and upload an entirely new file. Both approaches work.

> [!Tip]
> If you're using Cyberduck or another GUI client, the client may ask you to confirm that you wish to overwrite the existing `.txt` file if re-using the original file (option 1 in Step 8).

## Troubleshooting
- If you do not see any new buckets created, check the logs in the `$ID-managed` bucket for error messages.
- If you attempt to create multiple buckets at one time and one bucket has an error (for example, the name is too long or you attempted to create more than five buckets), **none of the buckets will be created**. You must correct the issue and start again for all buckets.
