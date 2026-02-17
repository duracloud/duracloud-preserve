# checksum-report

- Lambda triggered by: cloudtrail eventbridge event (job status `complete` or `failed`)
- Dependencies: compute-checksums

## Overview

1. Processes batch compute checksum reports into a single checksum report csv.
2. Generates checksum verification stats (total mismatches etc.).
