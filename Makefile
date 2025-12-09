.DEFAULT_GOAL := help
SHELL:=/bin/bash

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' Makefile | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-45s\033[0m %s\n", $$1, $$2}'

watch: ## Watch function (make watch f=bucket-request p=profile s=stack)
	@AWS_PROFILE=$(p) STACK=$(s) cargo lambda watch -p $(f)
