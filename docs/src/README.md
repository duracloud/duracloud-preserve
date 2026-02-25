# Introduction

This project is a set of components built using AWS services centered around digital preservation use cases. It provides configuration and features on top of AWS S3 to support long term access to and preservation of files.

The goal is to make it easy for users to choose any off the shelf S3 client and interact with S3 gaining more advanced features by default. Advanced features are described in more detail throughout the user and technical documentation but in brief: versioning, inventory, replication, logging etc. is enabled as buckets are created without a user having to do anything in AWS.

Periodically checksum verification is performed to ensure that file integrity is maintained between the primary and replicated (backup) files. This builds upon the already impressive levels of durability that S3 provides by adding an additonal, recurring guarantee that files are what they are intended to be.

Additional features include generating manifest (file inventory) and storage reports and user access control via preconstructed groups that are scoped to stacks. When deployed every resource is created "within" a stack. A stack is simply a resource naming prefix and tag applied to all resources managed by the deployed components to exclusively associate them. This makes it possible to have multiple stacks within a single account and makes it so different users can belong to one or more stacks.

AWS resources used:

- [CloudWatch](#)
- [Eventbridge](#)
- [IAM](#)
- [Lambda](#)
- [S3](#)

In the digital era, ensuring that critically important documents remain safe and available is a continual challenge. Physical computing hardware that is used to create and store documents can fail or become obsolete very quickly, providing a need for tools to ensure that these documents remain available. This project aims to address these concerns:

* How do I upload files to the storage service in a simple and reliable way?
* How do I ensure that the storage service that I am using receives a copy of my local files?
* How do I ensure that files remain intact over time?
* How do I retrieve my content once it is stored?
* How do I recover a file if it has been overwritten or corrupted?
* How do I make my content publicly accessible at a stable URL?
* How am I protected against the storage service becoming obsolete or going away?
