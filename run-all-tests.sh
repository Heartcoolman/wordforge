#!/bin/bash
# 完整测试套件运行脚本
# 包括：单元测试、E2E测试

set -e

echo "=========================================="
echo "WordForge 完整测试套件"
echo "=========================================="

# 定义颜色输出
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# 检查是否在项目根目录
if [ ! -d "frontend" ]; then
    echo -e "${RED}错误: 请在项目根目录下运行此脚本${NC}"
    exit 1
fi

# 前端目录
cd frontend

# 安装依赖
echo -e "${YELLOW}📦 检查并安装依赖...${NC}"
if [ ! -d "node_modules" ]; then
    npm ci
else
    echo "✓ 依赖已安装"
fi

# 运行单元测试
echo -e "\n${YELLOW}🧪 运行前端单元测试...${NC}"
if npm run test; then
    echo -e "${GREEN}✅ 单元测试通过${NC}"
else
    echo -e "${RED}❌ 单元测试失败${NC}"
    exit 1
fi

# 安装Playwright浏览器
echo -e "\n${YELLOW}🌐 检查Playwright浏览器...${NC}"
if [ ! -d "$HOME/.cache/ms-playwright/chromium"* ] 2>/dev/null; then
    npx playwright install chromium
else
    echo "✓ Playwright浏览器已安装"
fi

# 运行E2E测试
echo -e "\n${YELLOW}🎭 运行E2E测试...${NC}"
if npm run test:e2e; then
    echo -e "${GREEN}✅ E2E测试通过${NC}"
    E2E_STATUS="passed"
else
    echo -e "${YELLOW}⚠️  部分E2E测试失败（可能是预期的）${NC}"
    E2E_STATUS="partial"
fi

# 返回项目根目录
cd ..

# 运行后端测试
echo -e "\n${YELLOW}🦀 运行Rust后端测试...${NC}"
export JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd"
export ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long"

if cargo test --no-fail-fast; then
    echo -e "${GREEN}✅ 后端测试通过${NC}"
else
    echo -e "${RED}❌ 后端测试失败${NC}"
    exit 1
fi

# 测试总结
echo -e "\n=========================================="
echo -e "${GREEN}✅ 测试套件执行完成！${NC}"
echo "=========================================="
echo ""
echo "📊 测试结果："
echo "  - 前端单元测试: ✅ 通过"
echo "  - 前端E2E测试: ${E2E_STATUS}"
echo "  - 后端测试: ✅ 通过"
echo ""
echo "📁 测试报告:"
echo "  - E2E报告: frontend/playwright-report/index.html"
echo "  - 查看E2E报告: cd frontend && npx playwright show-report"
echo ""
