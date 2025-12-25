.DEFAULT_GOAL := help
SHELL:=/bin/bash

.PHONY: bucket-request
bucket-request: ## Upload txt to request bucket (make bucket-request f=file s=stack p=profile)
	@AWS_PROFILE=$(p) aws s3 cp $(f) s3://$(s)-bucket-request/buckets.txt

.PHONY: bucket
bucket: ## Perform action on a bucket (make bucket a=action b=bucket p=profile)
	@AWS_PROFILE=$(p) ./scripts/bucket.sh $(a) $(b)

build:
	@cargo lambda build --release --arm64 --output-format zip

.PHONY: help
help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' Makefile | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

.PHONY: invoke
invoke: ## Invoke lambda function locally (make invoke f=function e=event)
	@cargo lambda invoke -p $(f) --data-file $(e)

.PHONY: reset
reset: ## Reset (empty) remote resources (make reset s=stack p=profile)
	@AWS_PROFILE=$(p) ./scripts/reset.sh empty $(s)

.PHONY: setup
setup: ## Create required IAM role and buckets (make setup s=stack p=profile)
	@AWS_PROFILE=$(p) ./scripts/create-replication-role.sh $(s)
	@AWS_PROFILE=$(p) ./scripts/bucket.sh create $(s)-bucket-request
	@AWS_PROFILE=$(p) ./scripts/bucket.sh create $(s)-managed
	@AWS_PROFILE=$(p) ./scripts/set-managed-bucket-policy.sh $(s)

.PHONY: teardown
teardown: reset ## Destroy remote resources (make teardown s=stack p=profile)
	@AWS_PROFILE=$(p) ./scripts/reset.sh delete $(s)

.PHONY: test
test: ## Run local tests with no AWS calls (make test)
	@cargo test

.PHONY: test-integration
test-integration: setup ## Run integration tests (make test-integration s=stack p=profile)
	@AWS_PROFILE=$(p) TEST_STACK=$(s) cargo test --test bucket_creator -- --ignored --test-threads=1

.PHONY: watch
watch: ## Watch function (make watch f=function s=stack p=profile)
	@AWS_PROFILE=$(p) STACK=$(s) cargo lambda watch -p $(f)
