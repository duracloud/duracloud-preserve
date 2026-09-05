# Making Content Public

There are two ways to make content public.

## Pre-created public bucket with CloudFront (recommended)

Each stack includes a pre-created `-public` bucket that is served through a CloudFront distribution with a friendly domain. This is the recommended way to make content publicly accessible.

Your administrator will provide the public domain URL, but you can also construct what a public link from this bucket will look like based on this pattern:

```text
https://{$ID}.preserve.duracloud.org/{FOLDER STRUCTURE}{FILENAME}
```

So, for example, a .jpg image found in the `test` account's public bucket → photographs → Cats folder structure would look like:

<https://test.preserve.duracloud.org/photographs/Cats/callie_and_friend.jpg>

### Cyberduck

Navigate to the `duracloud-$ID-public` bucket and upload your files there (see [Uploading Files](./uploading-files.md)). Files uploaded to this bucket will be publicly accessible via the CloudFront domain.

### SFTPGo

Navigate to the `public` folder and upload your content there (see [Uploading Files](./uploading-files.md)). Files placed here will be publicly accessible via the CloudFront domain.

### AWS CLI

```bash
aws s3 cp myfile.jpg s3://duracloud-$ID-public/myfolder/myfile.jpg
```

## Suggestions for public content

Upload smaller file sizes and formats (for example, .jpgs instead of .tiffs) to your `-public` bucket as access copies. Store preservation copies elsewhere in DuraCloud Preserve. Smaller file sizes and formats will download and be viewable more quickly for your users and work with a variety of browser and internet speed limits.

## File delivery behavior

The CloudFront domain is intended for publishing files, not for hosting a website. PDF, HLS/video, audio, subtitle, and common raster-image files use a known browser media type and may display or play inline. Other formats are delivered as downloads.

HTML, SVG, JavaScript, CSS, XML, WebAssembly, executables, archives, files without an extension, and unknown file types do not execute in the browser from the preserve domain. The service determines browser behavior from the filename extension and does not rely on content-type metadata supplied during upload.

The following locations are reserved for service use and cannot be uploaded or replaced by client users:

```text
404.txt
watch/*
```

Place public content somewhere else in the bucket.

## Creating public buckets (not recommended)

You can also make content publicly available by designating a bucket as `-public` - See [How to Create Buckets](creating-buckets.md).

You can construct what a public link will look like in this scenario based on this pattern:

```bash
https://{BUCKET_NAME}.s3.{REGION}.amazonaws.com/{PREFIX}/{FILE}
```

If you have spaces in any of your folder or filenames, replace those with a + sign when forming a URL. The region information is also optional.

So, for example, an image found in the lyrasis account’s bucket public → test-01 → catpics folder structure would look like:

<https://dcp-test-public.s3.us-west-2.amazonaws.com/photographs/Cats/callie_and_friend.jpg>

OR, without the region information:

<https://dcp-test-public.s3.amazonaws.com/photographs/Cats/callie_and_friend.jpg>

**Note this feature is currently available but may be restricted in the future as it goes against AWS guidelines. You may be required to move content in public buckets to the CloudFront bucket or it may be moved for you.**

## Cyberduck sharing options

Cyberduck has some additional ways to share folders and individual objects.

- Navigate to the item you wish to share.
- Right-click on Windows / control+click on a Mac or two-finger click on a touchpad and select "Copy URL" — you can also use the Action (cog) menu and select "Open URL".
  - If you right-click and select "Copy URL," you will have options for how you wish to copy the URL, including HTTPS or HTTP, an expiration on the link (for individual objects only), or the AWS command link.
  - You can now share the item however you wish.
  - The HTTPS and HTTP links may be formed slightly differently (with AWS information before the bucket name), but they should still provide public access to objects in your account.
