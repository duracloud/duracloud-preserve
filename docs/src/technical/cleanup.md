# Cleanup

```bash
# empties buckets only, resources are not destroyed
mise run reset --stack digipres-dev1 --profile default

# teardown: empties buckets and deletes everything
mise run teardown --stack digipres-dev1 --profile default
```
