# Making Content Public
You can make content publicly available by designating a bucket as `-public` - See [How to Create Buckets](creating-buckets.md) or by uploading content to your `public` folder if you're using the SFTPGo web-based application.

You can construct what a public link will look like based on this pattern:

```text
https://{BUCKET_NAME}.s3.{REGION}.amazonaws.com/{PREFIX}/{FILE}
```

If you have spaces in any of your folder or filenames, replace those with a `+` sign when forming a URL. The region information is also optional.

So, for example, an image found in the `lyrasis` account's bucket public → test-01 → catpics folder structure would look like:

https://duracloud-lyrasis-public.s3.us-west-2.amazonaws.com/test-01/catpics/callie_and_friend.jpg

OR, without the region information:

https://duracloud-lyrasis-public.s3.amazonaws.com/test-01/catpics/callie_and_friend.jpg


## Cyberduck sharing options
Cyberduck has some additional ways to share folders and individual objects.
- Navigate to the item you wish to share.
- Right-click on Windows / control+click on a Mac or two-finger click on a touchpad and select "Copy URL" — you can also use the Action (cog) menu and select "Open URL".
  - If you right-click and select "Copy URL," you will have options for how you wish to copy the URL, including HTTPS or HTTP, an expiration on the link (for individual objects only), or the AWS command link.
  - You can now share the item however you wish.
  - The HTTPS and HTTP links may be formed slightly differently (with AWS information before the bucket name), but they should still provide public access to objects in your account.