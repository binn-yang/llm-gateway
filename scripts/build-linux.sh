#!/bin/bash
set -e

# 确保从项目根目录运行
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

echo "🔨 Building LLM Gateway for Linux..."
echo "📁 Project root: $PROJECT_ROOT"

# 检查前端是否已构建
if [ ! -d "frontend/dist" ]; then
    echo ""
    echo "📦 Building frontend (required for embedding)..."
    cd frontend

    # 检查 node_modules 是否存在
    if [ ! -d "node_modules" ]; then
        echo "📥 Installing frontend dependencies..."
        npm ci
    fi

    echo "🔨 Building frontend..."
    npm run build

    cd ..
    echo "✅ Frontend build complete"
else
    echo "✅ Frontend already built (frontend/dist/ exists)"
fi

# 构建 Linux 二进制
echo ""
echo "🐧 Building backend for Linux x86_64 (GNU libc)..."

# 从项目根目录运行 cross，这样 frontend/dist 会被挂载到容器
# 使用 --manifest-path 指定 backend/Cargo.toml
cross build \
    --manifest-path backend/Cargo.toml \
    --target x86_64-unknown-linux-gnu \
    --release

echo ""
echo "✅ Build complete!"
echo ""
echo "📦 Binary location:"
ls -lh backend/target/x86_64-unknown-linux-gnu/release/llm-gateway

echo ""
echo "💡 Tip: To build a fully static version (no system dependencies):"
echo "   cross build --manifest-path backend/Cargo.toml --target x86_64-unknown-linux-musl --release"
