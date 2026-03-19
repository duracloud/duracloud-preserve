.DEFAULT_GOAL := help
SHELL:=/bin/bash

.PHONY: bucket
bucket: ## Perform action on a bucket (make bucket a=action b=bucket p=profile)
	@AWS_PROFILE=$(p) ./scripts/bucket.sh $(a) $(b)

build-cli: ## Build cli with debug profile (make build-cli)
	@cargo build -p duracloud

build-cli-release: ## Build cli with release profile (make build-cli-release)
	@cargo build -p duracloud --release

build-lambda: ## Build lambda functions with debug profile (make build-lambda)
	@cargo lambda build --workspace --exclude duracloud
	@rm -rf target/lambda/duracloud

build-lambda-release: ## Build lambda functions with release profile (make build-lambda-release)
	@cargo lambda build --workspace --exclude duracloud --release --arm64 --output-format zip
	@rm -rf target/lambda/duracloud
	@for f in functions/*/Cargo.toml; do \
		name=$$(basename $$(dirname $$f)); \
		version=$$(grep '^version' $$f | head -1 | sed 's/.*"\(.*\)"/\1/'); \
		cp target/lambda/$$name/bootstrap.zip target/lambda/$$name/$$name-$$version.zip; \
	done

.PHONY: ci
ci: test ## Run the ci checks locally
	@cargo fmt -- --check
	@cargo clippy --workspace --all-features -- -D warnings
	@cargo audit

.PHONY: deploy
deploy: build-lambda-release ## Deploy all resources including functions (make deploy s=stack p=profile)
	@AWS_PROFILE=$(p) TF_VAR_stack=$(s) TF_VAR_deploy=true terraform apply

.PHONY: docs
docs: ## Read the docs
	@cd docs && mdbook serve --open

.PHONY: event
event: ## Generate event file from sample (make event f=function s=stack)
	@mkdir -p payloads
	@sed 's/test-stack/$(s)/g' functions/$(f)/events/sample.json > payloads/$(f).json

.PHONY: help
help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' Makefile | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

.PHONY: invoke
invoke: ## Invoke lambda function (make invoke f=function e=event)
	@cargo lambda invoke --remote -p $(f) --data-file $(e)

.PHONY: job
job: ## Lookup job by id (make job i=id p=profile)
	@AWS_PROFILE=$(p) aws s3control describe-job \
	    --account-id $$(AWS_PROFILE=$(p) aws sts get-caller-identity --query 'Account' --output text) \
	    --job-id $(i)

.PHONY: reset
reset: ## Reset (empty) stack buckets (make reset s=stack p=profile)
	@AWS_PROFILE=$(p) cargo run -p duracloud -- reset --stack=$(s)

.PHONY: run-bucket-request
run-bucket-request: ## Run run-bucket-request cli (make run-bucket-request f=file s=stack p=profile)
	@AWS_PROFILE=$(p) cargo run -p duracloud -- bucket-request --stack=$(s) --file=$(f)

.PHONY: run-checksum-report
run-checksum-report: ## Run run-checksum-report cli (make run-checksum-report b=bucket p=profile)
	@AWS_PROFILE=$(p) cargo run -p duracloud -- checksum-report --bucket=$(b)

.PHONY: run-compute-checksums
run-compute-checksums: ## Run run-compute-checksums cli (make run-compute-checksums b=bucket p=profile)
	@AWS_PROFILE=$(p) cargo run -p duracloud -- compute-checksums --bucket=$(b)

.PHONY: run-inventory-report
run-inventory-report: ## Run run-inventory-report cli (make run-inventory-report b=bucket p=profile)
	@AWS_PROFILE=$(p) cargo run -p duracloud -- inventory-report --bucket=$(b)

.PHONY: run-storage-report
run-storage-report: ## Run run-storage-report cli (make run-storage-report s=stack p=profile)
	@AWS_PROFILE=$(p) cargo run -p duracloud -- storage-report --stack=$(s)

.PHONY: setup
setup: ## Create base infrastructure (make setup s=stack p=profile)
	@AWS_PROFILE=$(p) TF_VAR_stack=$(s) terraform init -upgrade
	@AWS_PROFILE=$(p) TF_VAR_stack=$(s) terraform apply

.PHONY: teardown
teardown: reset ## Destroy all infrastructure (make teardown s=stack p=profile)
	@AWS_PROFILE=$(p) TF_VAR_stack=$(s) TF_VAR_deploy=true terraform destroy

.PHONY: trigger
trigger: ## Trigger a lambda function remotely (make trigger f=function s=stack p=profile)
	@./scripts/trigger-function.sh $(f) $(s) $(p)

.PHONY: test
test: ## Run local tests with no AWS calls (make test)
	@cargo test

.PHONY: test-integration
test-integration: ## Run integration tests, makes AWS calls (make test-integration s=stack p=profile)
	@AWS_PROFILE=$(p) TEST_STACK=$(s) cargo test --test "*" -- --ignored --test-threads=1

.PHONY: upload
upload: ## Upload a file to a bucket (make upload b=bucket f=file p=profile)
	@AWS_PROFILE=$(p) aws s3 cp $(f) s3://$(b)/$(notdir $(realpath $(f)))
