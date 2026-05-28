#!/bin/bash
# E2E测试运行脚本

set -e

echo "======================================"
echo "E2E测试执行脚本"
echo "======================================"

# 检查是否在 admin-ui 目录
if [ ! -f "package.json" ]; then
    echo "错误: 请在 admin-ui 目录下运行此脚本"
    exit 1
fi

# 安装依赖（如果需要）
if [ ! -d "node_modules" ]; then
    echo "📦 安装依赖..."
    npm ci
fi

# 安装Playwright浏览器（如果需要）
if [ ! -d "$HOME/.cache/ms-playwright/chromium"* ]; then
    echo "🌐 安装Playwright浏览器..."
    npx playwright install chromium
fi

# 运行E2E测试
echo "🧪 运行E2E测试..."
npm run test:e2e

# 显示测试报告位置
echo ""
echo "======================================"
echo "✅ 测试完成！"
echo "======================================"
echo "📊 查看HTML报告: npx playwright show-report"
echo "📁 报告位置: playwright-report/index.html"
