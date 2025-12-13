.DEFAULT_GOAL := help
.PHONY: bucket-request bucket help invoke reset setup teardown test-integration watch
SHELL:=/bin/bash

bucket-request: ## Upload txt to request bucket (make bucket-request f=file s=stack p=profile)
	@AWS_PROFILE=$(p) aws s3 cp $(f) s3://$(s)-bucket-request/buckets.txt

bucket: ## Perform action on a bucket (make bucket a=action b=bucket p=profile)
	@AWS_PROFILE=$(p) ./scripts/bucket.sh $(a) $(b)

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' Makefile | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

invoke: ## Invoke lambda function locally (make invoke f=function e=event)
	@cargo lambda invoke -p $(f) --data-file $(e)

reset: ## Reset (empty) remote resources (make reset s=stack p=profile)
	@AWS_PROFILE=$(p) ./scripts/reset.sh empty $(s)

setup: ## Create required IAM role and buckets (make setup s=stack p=profile)
	@AWS_PROFILE=$(p) ./scripts/create-replication-role.sh $(s)
	@AWS_PROFILE=$(p) ./scripts/bucket.sh create $(s)-bucket-request
	@AWS_PROFILE=$(p) ./scripts/bucket.sh create $(s)-managed

teardown: reset ## Destroy remote resources (make teardown s=stack p=profile)
	@AWS_PROFILE=$(p) ./scripts/reset.sh delete $(s)

test-integration: setup # Run integration tests (make test-integration s=stack p=profile)
	@AWS_PROFILE=$(p) TEST_STACK=$(s) cargo test --test bucket_creator -- --ignored --test-threads=1

watch: ## Watch function (make watch f=function s=stack p=profile)
	@AWS_PROFILE=$(p) STACK=$(s) cargo lambda watch -p $(f)
