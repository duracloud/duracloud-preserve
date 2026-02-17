# Multiple Stacks Deployment

If you need to partition access to files by user, or require a very
large number of buckets, you will need to use **multiple stacks**.

Depending on the deployment strategy, multiple stacks may be deployed
to:

a)  a single account, or\
b)  separate AWS accounts.

## Single Account

In this case, each user can be assigned to access one or more stacks
within the same account. You will use one set of credentials for all the stacks you have access
to.

## Separate Accounts

In this case, you will receive one set of credentials for each account
you have access to. Within each account, you will have access to one or more stacks.

> [!TIP]
> These are the standard deployment paths, but consult with your IT support or service provider on the best approach for your use case and which options they provide.
