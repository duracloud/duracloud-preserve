.DEFAULT_GOAL := help
SHELL:=/bin/bash

ARTIFACT_REGIONS := us-east-1 us-east-2 us-west-2

export SFTPGO_HOST ?= http://localhost:8080
export SFTPGO_USERNAME ?= admin
export SFTPGO_PASSWORD ?= admin

.PHONY: bucket
bucket: ## Perform action on a bucket (make bucket a=action b=bucket p=profile)
	@AWS_PROFILE=$(p) ./scripts/bucket.sh $(a) $(b)

build: ## Build lambda functions with release profile (make build)
	@cargo lambda build --workspace --exclude dcp --release --arm64 --output-format zip
	@rm -rf target/lambda/dcp

.PHONY: cli
cli: ## Build dcp arm64 Docker image tagged latest (make cli)
	docker buildx build --platform linux/arm64 \
		-t duracloud/dcp:latest \
		--load .

.PHONY: ci
ci: test ## Run the ci checks locally
	@cargo fmt -- --check
	@cargo clippy --workspace --all-features -- -D warnings
	@cargo audit
	@terraform fmt .

.PHONY: deploy
deploy: locals ## Deploy all resources including functions (make deploy s=stack p=profile)
	@AWS_PROFILE=$(p) TF_VAR_stack=$(s) TF_VAR_deploy=true terraform apply

.PHONY: docs
docs: ## Read the docs
	@cd docs && mdbook serve --open

.PHONY: docs-pdf
docs-pdf: ## Build docs PDF with WeasyPrint (make docs-pdf [o=duracloud-preserve.pdf])
	@command -v weasyprint >/dev/null || { echo "WeasyPrint is required: https://weasyprint.org/"; exit 1; }
	@cd docs && mdbook build
	@weasyprint docs/book/print.html $(or $(o),duracloud-preserve.pdf)

.PHONY: event
event: ## Generate event file from sample (make event f=function s=stack)
	@mkdir -p payloads
	@sed 's/test-stack/$(s)/g' functions/$(f)/events/sample.json > payloads/$(f).json

.PHONY: locals
locals: ## Generate _locals.tf from Rust constants (make locals)
	@./scripts/gen-locals.sh

.PHONY: invoke
invoke: ## Invoke lambda function (make invoke f=function e=event)
	@cargo lambda invoke --remote -p $(f) --data-file $(e)

.PHONY: job-status
job-status: ## Lookup job status by id (make job-status i=id p=profile)
	@AWS_PROFILE=$(p) aws s3control describe-job \
	    --account-id $$(AWS_PROFILE=$(p) aws sts get-caller-identity --query 'Account' --output text) \
	    --job-id $(i)

.PHONY: job-status-by-receipt
job-status-by-receipt: ## Lookup job status by checksum receipt (make job-status-by-receipt b=bucket p=profile)
	@stack="$(b)"; stack="$${stack%-*}"; \
	    $(MAKE) job i=$$(AWS_PROFILE=$(p) aws s3 cp s3://$${stack}-managed/metadata/latest/checksums/receipts/$(b).json - | jq -r .repl_job_id) p=$(p)

.PHONY: publish
publish: build ## Publish lambda release artifacts to dcp-artifacts buckets (make publish p=profile)
	@for region in $(ARTIFACT_REGIONS); do \
		echo "Publishing to dcp-artifacts-$$region..."; \
		AWS_PROFILE=$(p) aws s3 sync target/lambda/ s3://dcp-artifacts-$$region/ \
			--region $$region --exclude "*" --include "*.zip"; \
	done

.PHONY: reset
reset: ## Reset (empty) stack buckets (make reset s=stack p=profile)
	@AWS_PROFILE=$(p) cargo run -p dcp -- reset --stack=$(s)

.PHONY: run-bucket-request
run-bucket-request: ## Run run-bucket-request cli (make run-bucket-request f=file s=stack p=profile)
	@AWS_PROFILE=$(p) cargo run -p dcp -- bucket-request --stack=$(s) --file=$(f)

.PHONY: run-checksum-report
run-checksum-report: ## Run run-checksum-report cli (make run-checksum-report b=bucket p=profile)
	@AWS_PROFILE=$(p) cargo run -p dcp -- checksum-report --bucket=$(b)

.PHONY: run-compute-checksums
run-compute-checksums: ## Run run-compute-checksums cli (make run-compute-checksums b=bucket p=profile)
	@AWS_PROFILE=$(p) cargo run -p dcp -- compute-checksums --bucket=$(b)

.PHONY: run-inventory-report
run-inventory-report: ## Run run-inventory-report cli (make run-inventory-report b=bucket p=profile)
	@AWS_PROFILE=$(p) cargo run -p dcp -- inventory-report --bucket=$(b)

.PHONY: run-storage-report
run-storage-report: ## Run run-storage-report cli (make run-storage-report s=stack p=profile)
	@AWS_PROFILE=$(p) cargo run -p dcp -- storage-report --stack=$(s)

.PHONY: setup
setup: locals ## Create base infrastructure (make setup s=stack p=profile)
	@AWS_PROFILE=$(p) TF_VAR_stack=$(s) terraform init -upgrade
	@AWS_PROFILE=$(p) TF_VAR_stack=$(s) terraform apply

.PHONY: teardown
teardown: reset ## Destroy all infrastructure (make teardown s=stack p=profile)
	@AWS_PROFILE=$(p) TF_VAR_stack=$(s) TF_VAR_deploy=true terraform destroy

.PHONY: test
test: ## Run local tests with no AWS calls (make test)
	@cargo test

.PHONY: test-integration
test-integration: ## Run integration tests, makes AWS calls (make test-integration s=stack p=profile)
	@AWS_PROFILE=$(p) TEST_STACK=$(s) cargo test --test "*" -- --ignored --test-threads=1

.PHONY: trigger
trigger: ## Trigger a lambda function remotely (make trigger f=function s=stack p=profile)
	@./scripts/trigger-function.sh $(f) $(s) $(p)

.PHONY: upload
upload: ## Upload a file to a bucket (make upload b=bucket [d=dir] f=file p=profile)
	@AWS_PROFILE=$(p) aws s3 cp $(f) s3://$(b)$(if $(d),/$(d))/$(f)

# Last, to handle syntax highlighting wonkiness
.PHONY: help
help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' Makefile | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'
