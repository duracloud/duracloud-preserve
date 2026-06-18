# Users and Groups

There are three different account types available with DuraCloud Preserve:
- Power users
- Standard users
- Restricted users

## Power users

Power users have the most options available. They can create buckets, upload content, move content around, and delete content. The delete option is unique to power users and should likely be limited to 1-3 staff at your institution because the ability to delete is so powerful.

## Standard users

Standard users can create buckets, copy content, and upload content. They cannot delete. This means if moving content around between folder structures or buckets, standard users can copy the content from one location to another but not remove it from the original location. Only a power user can do that. Standard users have access and visibility to all buckets within your DuraCloud Preserve account.

## Restricted users

Restricted users have the ability to upload content to specific buckets as identified by the power user(s) for the institution. Depending on those restrictions, this may mean that a restricted user can create buckets, if they have access to the `-requested` bucket. If you decide not to give a restricted user access to that bucket, then they will not have the ability to create buckets (but they can still create file structures within the buckets to which they have been designatd access).

Restricted users can see all buckets associated with a DuraCloud Preserve account but won't be able to see or interact with content to which they are not designated. For example, if your account has `-archives` and `-special-collections` buckets, but a restricted user only has access to the `-archives` bucket, they will see that there is a `-special-collections` bucket, but if they try to navigate into the bucket, they will get a permissions error, no matter which option they're using to interact with your DuraCloud Preserve account.

> [!Tip]
When setting up your DuraCloud Preserve account, be sure to tell your hosting provider which users should have power, standard, or restricted accounts. If asking for restricted users, also let your provider know to which buckets these users should have access. You'll also need to update your provider when creating new buckets or needing new restricted user accounts.
