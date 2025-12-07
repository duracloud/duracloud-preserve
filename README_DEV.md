# Developer docs

Requirements:

- [rust](#)
- [cargo-lambda](#)

## Creating a function

```bash
cargo lambda new bucket-request
```

```bash
# Build all or specified pkg (using -p)
cargo lambda build [-p $pkg]

# Run local
cargo lambda watch -p $pkg

# Invoke local with a sample payload
cargo lambda invoke -p $pkg --data-example s3-event

# Invoke local using a json file as payload
cargo lambda invoke -p $pkg --data-file $pkg/events/event.json
```

- [Event payloads](https://github.com/aws/aws-lambda-rust-runtime/tree/main/lambda-events/src/fixtures)
