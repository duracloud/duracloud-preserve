.DEFAULT_GOAL := help
.PHONY: bucket-request buckets help teardown watch
SHELL:=/bin/bash

bucket-request: ## Upload txt to request bucket (make bucket-request f=file p=profile s=stack)
	@AWS_PROFILE=$(p) aws s3 cp $(f) s3://$(s)-bucket-request/buckets.txt

buckets: ## Perform action on required buckets (make buckets a=action p=profile s=stack)
	@AWS_PROFILE=$(p) ./scripts/buckets.sh $(a) $(s)-bucket-request
	@AWS_PROFILE=$(p) ./scripts/buckets.sh $(a) $(s)-managed

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' Makefile | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-45s\033[0m %s\n", $$1, $$2}'

invoke:
	@cargo lambda invoke -p $(f) --data-file $(e)

teardown: ## Destroy remote resource (make teardown p=profile s=stack)
	@AWS_PROFILE=$(p) ./scripts/buckets.sh empty $(s)-bucket-request
	@AWS_PROFILE=$(p) ./scripts/buckets.sh empty $(s)-managed

watch: ## Watch function (make watch f=bucket-request p=profile s=stack)
	@AWS_PROFILE=$(p) STACK=$(s) cargo lambda watch -p $(f)
