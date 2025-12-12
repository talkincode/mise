#!/bin/bash
# mise 自动测试脚本 (演示用)
# 用法: ./fulltest.sh

set -e

MISE="./target/release/mise"
ALPHA="tests/samples/sample_project_alpha"
BETA="tests/samples/sample_project_beta"
INVALID="tests/samples/anchors_invalid"

# 颜色定义
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

run_cmd() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${YELLOW}▶ $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    eval "$1"
    echo -e "${GREEN}✓ 完成${NC}\n"
    sleep 1
}

echo -e "${GREEN}"
echo "╔══════════════════════════════════════════════════════╗"
echo "║           mise 自动测试脚本 (演示)                   ║"
echo "╚══════════════════════════════════════════════════════╝"
echo -e "${NC}"

# 检查二进制是否存在
if [ ! -f "$MISE" ]; then
    echo "⚠️  二进制不存在，正在编译..."
    cargo build --release
fi

echo -e "\n${GREEN}【1. doctor - 检查依赖】${NC}\n"
run_cmd "$MISE doctor --format jsonl"
run_cmd "$MISE doctor --format md"

echo -e "\n${GREEN}【2. scan - 文件扫描】${NC}\n"
run_cmd "$MISE scan --root $BETA --format jsonl"
run_cmd "$MISE scan --root $BETA --type file --format jsonl"
run_cmd "$MISE scan --root $BETA --hidden --format jsonl"
run_cmd "$MISE scan --root $BETA --max-depth 1 --format md"

echo -e "\n${GREEN}【3. find - 文件查找】${NC}\n"
run_cmd "$MISE find readme --root $ALPHA --format jsonl"
run_cmd "$MISE find big --root $ALPHA --format jsonl"

echo -e "\n${GREEN}【4. extract - 内容提取】${NC}\n"
run_cmd "$MISE extract docs/big.txt --lines 10:20 --root $ALPHA --format jsonl"
run_cmd "$MISE extract README.md --lines 1:10 --format raw"

echo -e "\n${GREEN}【5. match - 文本搜索 (rg)】${NC}\n"
run_cmd "$MISE match 'TODO|FIXME' --format jsonl | head -5"
run_cmd "$MISE match 'NEEDLE_ALPHA_123' docs --root $ALPHA --format jsonl"

echo -e "\n${GREEN}【6. ast - 结构化搜索 (ast-grep)】${NC}\n"
run_cmd "$MISE ast 'console.log(\$A)' web --root $ALPHA --format jsonl"
run_cmd "$MISE ast 'unsafe { \$\$\$BODY }' src --root $ALPHA --format jsonl"

echo -e "\n${GREEN}【7. anchor - 锚点管理】${NC}\n"
run_cmd "$MISE anchor list --root $ALPHA --format jsonl | head -3"
run_cmd "$MISE anchor list --tag intro --root $ALPHA --format jsonl"
run_cmd "$MISE anchor get alpha.intro --root $ALPHA --format jsonl"
run_cmd "$MISE anchor lint --root $INVALID --format jsonl"

echo -e "\n${GREEN}【8. deps - 依赖分析】${NC}\n"
run_cmd "$MISE deps --root . --format jsonl | head -5"
run_cmd "$MISE deps src/cli.rs --root . --format tree"
run_cmd "$MISE deps src/cli.rs --root . --reverse --format tree"
run_cmd "$MISE deps --root . --format dot | head -20"
run_cmd "$MISE deps --root . --format mermaid | head -20"
run_cmd "$MISE deps --root . --format table | head -15"

echo -e "\n${GREEN}【9. flow writing - 写作流程】${NC}\n"
run_cmd "$MISE flow writing --anchor alpha.intro --max-items 5 --root $ALPHA --format jsonl"

echo -e "\n${GREEN}【10. rebuild - 重建缓存】${NC}\n"
run_cmd "$MISE rebuild --root $ALPHA --format jsonl"

echo -e "${GREEN}"
echo "╔══════════════════════════════════════════════════════╗"
echo "║              ✅ 所有测试完成!                        ║"
echo "╚══════════════════════════════════════════════════════╝"
echo -e "${NC}"
