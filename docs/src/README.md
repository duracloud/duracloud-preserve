# Introduction

DuraCloud Preserve is a project designed to make using [AWS](https://aws.amazon.com/) [S3](https://aws.amazon.com/s3/) as simple as possible for users who only want to care about uploading files, or integrating S3 storage with other applications, and not have to worry about esoteric configuration or infrastructure management. It also supports digital preservation use cases by managing the configuration and features available to S3 to support long term access to and preservation of files.

The goal is to make it easy for users to choose any off the shelf S3 client and interact with S3 gaining more advanced features by default. Advanced features are described in more detail throughout the user and technical documentation but in brief: versioning, inventory, replication, logging etc. is enabled as buckets are created without a user having to do anything in AWS.

Periodically checksum verification is performed to ensure that file integrity is maintained between the primary and replicated (backup) files. This builds upon the already impressive levels of durability that S3 provides by adding a recurring guarantee that files are what they are intended to be.

Additional features include generating manifest (file inventory) and storage reports and user access control via preconstructed groups that are scoped to stacks. When deployed every resource is created "within" a stack. A stack is simply a resource naming prefix and tag applied to all resources managed by the deployed components to exclusively associate them. This makes it possible to have multiple stacks within a single account and makes it so different users can belong to one or more stacks.

[Lyrasis](https://lyrasis.org/) provides a [hosting service](./lyrasis.md) for DuraCloud Preserve, handling the AWS account creation and installation, and which comes with access to a web based ui for S3, using [SFTPGo](https://sftpgo.com/). S3 can then be interacted with using the web ui or via direct AWS access credentials for broader integrations or for usage with tools like the [AWS cli](https://aws.amazon.com/cli/).

AWS resources used:

- [CloudWatch](#)
- [Eventbridge](#)
- [IAM](#)
- [Lambda](#)
- [S3](#)

## Context

DuraCloud Preserve is a continuation of the [DuraCloud](#) project in a form that is intended to be more sustainable for the long term. It does this by focusing on the core mission of DuraCloud but with a significantly smaller technical footprint, made possible by leveraging AWS S3 features directly in contrast to the more abstracted approach that DuraCloud took in being open to support multiple backend storage providers.

But the goals remain the same: in the digital era, ensuring that critically important documents remain safe and available is a continual challenge. Physical computing hardware that is used to create and store documents can fail or become obsolete very quickly, providing a need for tools to ensure that these documents remain available. DuraCloud Preserve aims to address these concerns:

- How do I upload files to the storage service in a simple and reliable way?
- How do I ensure that the storage service that I am using receives a copy of my local files?
- How do I ensure that files remain intact over time?
- How do I retrieve my content once it is stored?
- How do I recover a file if it has been overwritten or corrupted?
- How do I make my content publicly accessible at a stable URL?
- How am I protected against the storage service becoming obsolete or going away?

Answers to these questions are provided throughout the rest of this documentation.
