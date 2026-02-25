# Getting started

Whoever is responsible for deployment will provide access
credentials to users.  If you are intending to connect directly to S3
using a GUI or CLI tool then you should receive an **access key** and
**secret**, which serve as a username and password for interacting with
S3. **It is important to treat this as sensitively as you would any
username and password.**

If you are intending to use the web client then you should receive
a **username** (your email address), **password** and the **url** to
login. It's completely fine to use both approaches if you'd like access
to both.

You should also receive a **stack name**.
  
This will typically be in the form `duracloud-$ID` where `$ID` is an
identifier assigned by those handling the deployment. It may be based on
or similar to a sitecode used by your institution for its domain (e.g.
INSTITUTION.edu).

It is important to know this because your user will only be able to
interact with a subset of buckets in an AWS account that are prefixed
with that stack name. You will also see references to *stack name*
throughout the documentation.

> [!IMPORTANT]
> Before proceeding confirm you have received:
> - Access key (**username)** and secret (**password**) for direct s3
  access if requested
> - Stack prefix (`duracloud-$ID`)
> - Web client **username**, **password** and **url** if requested

### Lyrasis Hosting clients permissions

Hosting clients will start with identifying one user who will have power
user permissions. This user will be able to upload, download, and
delete. The initial power user will need to provide the Hosting team the
names of other users for whom they wish to have accounts and indicate
whether those users should be power users or standard users who can only
upload files. The Hosting team recommends limiting the number of power
users per institution to 1 or 2 individuals because of the power to
delete.
