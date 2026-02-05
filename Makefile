.DEFAULT_GOAL := help
SHELL:=/bin/bash

.PHONY: bucket
bucket: ## Perform action on a bucket (make bucket a=action b=bucket p=profile)
	@AWS_PROFILE=$(p) ./scripts/bucket.sh $(a) $(b)

.PHONY: bucket-request
bucket-request: ## Run bucket-request cli (make bucket-request f=file s=stack p=profile)
	@AWS_PROFILE=$(p) cargo run -p duracloud -- bucket-request --stack=$(s) --names=$(f)

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

.PHONY: checksum-report
checksum-report: ## Run checksum-report cli (make checksum-report b=bucket p=profile)
	@AWS_PROFILE=$(p) cargo run -p duracloud -- checksum-report --bucket=$(b)

.PHONY: ci
ci: test ## Run the ci checks locally
	@cargo fmt -- --check
	@cargo clippy --workspace --all-features -- -D warnings
	@cargo audit

.PHONY: docs
docs: ## Read the docs
	@cd docs && mdbook serve --open

.PHONY: help
help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' Makefile | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

.PHONY: event
event: ## Generate event file from sample (make event f=function s=stack)
	@mkdir -p payloads
	@sed 's/test-stack/$(s)/g' functions/$(f)/events/sample.json > payloads/$(f).json

.PHONY: compute-checksums
compute-checksums: ## Run compute-checksums cli (make compute-checksums b=bucket p=profile)
	@AWS_PROFILE=$(p) cargo run -p duracloud -- compute-checksums --bucket=$(b)

.PHONY: inventory-report
inventory-report: ## Run inventory-report cli (make inventory-report b=bucket p=profile)
	@AWS_PROFILE=$(p) cargo run -p duracloud -- inventory-report --bucket=$(b)

.PHONY: invoke
invoke: ## Invoke lambda function locally (make invoke f=function e=event)
	@cargo lambda invoke -p $(f) --data-file $(e)

.PHONY: invoke-bucket-request
invoke-bucket-request: ## Invoke bucket request function locally (make invoke-bucket-request s=stack p=profile)
	@$(MAKE) event f=bucket-request s=$(s)
	@$(MAKE) upload b=bucket-request f=files/buckets.txt s=$(s) p=$(p)
	@cargo lambda invoke -p bucket-request --data-file payloads/bucket-request.json

.PHONY: invoke-inventory-report
invoke-inventory-report: ## Invoke inventory report function locally (make invoke-inventory-report s=stack p=profile)
	@$(MAKE) event f=inventory-report s=$(s)
	@sed 's/test-stack/$(s)/g' files/inventory-manifest.json > payloads/manifest.json
	@$(MAKE) upload b=managed f=files/example.parquet s=$(s) p=$(p)
	@$(MAKE) upload b=managed f=payloads/manifest.json s=$(s) p=$(p)
	@cargo lambda invoke -p inventory-report --data-file payloads/inventory-report.json

.PHONY: reset
reset: ## Reset (empty) stack buckets (make reset s=stack p=profile)
	@AWS_PROFILE=$(p) cargo run -p duracloud -- reset --stack=$(s)

.PHONY: setup
setup: ## Create base infrastructure (make setup s=stack p=profile)
	@AWS_PROFILE=$(p) TF_VAR_stack=$(s) terraform init -upgrade
	@AWS_PROFILE=$(p) TF_VAR_stack=$(s) terraform apply

.PHONY: teardown
teardown: ## Destroy base infrastructure (make teardown s=stack p=profile)
	@AWS_PROFILE=$(p) TF_VAR_stack=$(s) TF_VAR_deploy=true terraform destroy

.PHONY: test
test: ## Run local tests with no AWS calls (make test)
	@cargo test

.PHONY: test-integration
test-integration: setup ## Run integration tests, makes AWS calls (make test-integration s=stack p=profile)
	@AWS_PROFILE=$(p) TEST_STACK=$(s) cargo test --test "*" -- --ignored --test-threads=1

.PHONY: upload
upload: ## Upload a file to a bucket (make upload b=bucket f=file s=stack p=profile)
	@AWS_PROFILE=$(p) aws s3 cp $(f) s3://$(s)-$(b)/$(notdir $(realpath $(f)))

.PHONY: watch
watch: ## Watch function (make watch f=function s=stack p=profile)
	@AWS_PROFILE=$(p) STACK=$(s) cargo lambda watch -p $(f)
