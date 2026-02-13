# Development

## Creating a function

```bash
cd functions
cargo lambda new bucket-request
```

Add the function path to the project root `Cargo.toml`.

From the project root dir:

```bash
# Build all or specified pkg (using -p)
cargo lambda build [-p $pkg]

# Run local
cargo lambda watch -p $pkg

# Invoke local with a sample payload
cargo lambda invoke -p $pkg --data-example s3-event

# Invoke local using a json file as payload
cargo lambda invoke -p $pkg --data-file functions/$pkg/events/event.json
```

- [Event payloads](https://github.com/aws/aws-lambda-rust-runtime/tree/main/lambda-events/src/fixtures)
