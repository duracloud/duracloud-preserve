# Functions

The core service functionality is ecapsulated by these
"functions" which are deployed to AWS Lambda but can also
be invoked via the provided cli:

- [bucket-request](#)
- [inventory-report](#)
- [compute-checksums](#)
- [checksum-report](#)
- [storage-report](#)

The cli additionally has functionality for:

- computing a checksum
- emptying content from buckets (careful!)
