.PHONY: help test coverage coverage-backend coverage-frontend clean

help: ## 显示帮助信息
	@echo "可用命令："
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

test: ## 运行所有测试
	@echo "运行后端测试..."
	JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd" \
	ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long" \
	cargo test --no-fail-fast
	@echo "\n运行前端测试..."
	cd frontend && npm run test

coverage: coverage-backend coverage-frontend ## 生成完整的覆盖率报告

coverage-backend: ## 生成后端覆盖率报告（HTML + JSON）
	@echo "生成 Rust 后端覆盖率报告..."
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { \
		echo "安装 cargo-llvm-cov..."; \
		cargo install cargo-llvm-cov; \
	}
	@echo "清理旧的覆盖率数据..."
	cargo llvm-cov clean --workspace 2>/dev/null || true
	@echo "生成 HTML 报告..."
	JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd" \
	ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long" \
	cargo llvm-cov --html \
		--ignore-filename-regex="tests/" \
		--ignore-run-fail
	@echo "生成 JSON 报告..."
	JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd" \
	ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long" \
	cargo llvm-cov --json --no-run \
		--ignore-filename-regex="tests/" \
		--output-path target/llvm-cov/coverage.json
	@echo "✓ 后端覆盖率报告已生成："
	@echo "  HTML: target/llvm-cov/html/index.html"
	@echo "  JSON: target/llvm-cov/coverage.json"
	@JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd" \
	ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long" \
	cargo llvm-cov --summary-only --no-run --ignore-filename-regex="tests/" 2>/dev/null | grep "TOTAL" || echo "覆盖率摘要："

coverage-frontend: ## 生成前端覆盖率报告
	@echo "生成前端覆盖率报告..."
	cd frontend && npm run test:coverage
	@echo "✓ 前端覆盖率报告已生成："
	@echo "  HTML: frontend/coverage/index.html"

coverage-open: ## 在浏览器中打开覆盖率报告
	@echo "打开后端覆盖率报告..."
	@command -v xdg-open >/dev/null 2>&1 && xdg-open target/llvm-cov/html/index.html || \
	command -v open >/dev/null 2>&1 && open target/llvm-cov/html/index.html || \
	echo "请手动打开: target/llvm-cov/html/index.html"
	@echo "打开前端覆盖率报告..."
	@command -v xdg-open >/dev/null 2>&1 && xdg-open frontend/coverage/index.html || \
	command -v open >/dev/null 2>&1 && open frontend/coverage/index.html || \
	echo "请手动打开: frontend/coverage/index.html"

clean: ## 清理覆盖率报告
	@echo "清理覆盖率报告..."
	rm -rf target/llvm-cov
	rm -rf frontend/coverage
	@echo "✓ 清理完成"
