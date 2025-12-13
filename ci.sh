#!/usr/bin/env bash
#
# 本地 CI 脚本 - 模拟 GitHub Actions CI 工作流
#
# 用法:
#   ./ci.sh           # 运行所有检查
#   ./ci.sh check     # 仅运行 cargo check
#   ./ci.sh fmt       # 仅检查格式
#   ./ci.sh clippy    # 仅运行 clippy
#   ./ci.sh test      # 仅运行测试
#   ./ci.sh build     # 仅构建 release
#   ./ci.sh quick     # 快速检查 (fmt + clippy + check)
#   ./ci.sh full      # 完整检查 (所有步骤 + fulltest)
#

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# 计时
SECONDS=0

# 打印带颜色的消息
info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[✓]${NC} $1"
}

error() {
    echo -e "${RED}[✗]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[!]${NC} $1"
}

header() {
    echo ""
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}  $1${NC}"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

# 检查依赖
check_dependencies() {
    header "检查依赖"
    
    local missing=0
    
    if command -v cargo &> /dev/null; then
        success "cargo $(cargo --version | cut -d' ' -f2)"
    else
        error "cargo 未安装"
        missing=1
    fi
    
    if command -v rustfmt &> /dev/null; then
        success "rustfmt $(rustfmt --version | cut -d' ' -f2)"
    else
        warn "rustfmt 未安装 (运行: rustup component add rustfmt)"
        missing=1
    fi
    
    if command -v cargo-clippy &> /dev/null || cargo clippy --version &> /dev/null; then
        success "clippy $(cargo clippy --version 2>/dev/null | cut -d' ' -f2 || echo 'installed')"
    else
        warn "clippy 未安装 (运行: rustup component add clippy)"
        missing=1
    fi
    
    if command -v rg &> /dev/null; then
        success "ripgrep $(rg --version | head -1 | cut -d' ' -f2)"
    else
        warn "ripgrep 未安装 (部分测试可能跳过)"
    fi
    
    if command -v sg &> /dev/null; then
        success "ast-grep $(sg --version 2>/dev/null | head -1 || echo 'installed')"
    else
        warn "ast-grep 未安装 (部分测试可能跳过)"
    fi
    
    if [ $missing -eq 1 ]; then
        error "缺少必要依赖，请先安装"
        exit 1
    fi
}

# cargo check
do_check() {
    header "Cargo Check"
    info "运行 cargo check --all-features..."
    cargo check --all-features
    success "cargo check 通过"
}

# 格式检查
do_fmt() {
    header "格式检查"
    info "运行 cargo fmt --all -- --check..."
    if cargo fmt --all -- --check; then
        success "代码格式正确"
    else
        error "代码格式不正确，运行 'cargo fmt' 修复"
        exit 1
    fi
}

# Clippy
do_clippy() {
    header "Clippy 检查"
    info "运行 cargo clippy --all-features -- -D warnings..."
    cargo clippy --all-features -- -D warnings
    success "clippy 检查通过"
}

# 测试
do_test() {
    header "运行测试"
    info "运行 cargo test --all-features..."
    cargo test --all-features
    success "所有测试通过"
}

# 构建
do_build() {
    header "构建 Release"
    info "运行 cargo build --release --all-features..."
    cargo build --release --all-features
    
    local binary="target/release/mise"
    if [ -f "$binary" ]; then
        local size=$(du -h "$binary" | cut -f1)
        success "构建完成: $binary ($size)"
    else
        error "构建失败，找不到二进制文件"
        exit 1
    fi
}

# 完整测试
do_fulltest() {
    header "完整测试套件"
    if [ -f "./fulltest.sh" ]; then
        info "运行 ./fulltest.sh..."
        ./fulltest.sh
        success "完整测试通过"
    else
        warn "fulltest.sh 不存在，跳过"
    fi
}

# 显示帮助
show_help() {
    echo "mise 本地 CI 脚本"
    echo ""
    echo "用法: $0 [命令]"
    echo ""
    echo "命令:"
    echo "  (无参数)  运行标准 CI 流程 (check + fmt + clippy + test + build)"
    echo "  check     仅运行 cargo check"
    echo "  fmt       仅检查代码格式"
    echo "  clippy    仅运行 clippy linter"
    echo "  test      仅运行测试"
    echo "  build     仅构建 release 版本"
    echo "  quick     快速检查 (fmt + clippy + check)"
    echo "  full      完整检查 (所有步骤 + fulltest.sh)"
    echo "  deps      仅检查依赖"
    echo "  help      显示此帮助信息"
    echo ""
    echo "示例:"
    echo "  ./ci.sh              # 提交前的标准检查"
    echo "  ./ci.sh quick        # 快速检查，用于开发中"
    echo "  ./ci.sh full         # 发布前的完整检查"
}

# 显示总结
show_summary() {
    local duration=$SECONDS
    echo ""
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}  ✓ CI 检查全部通过！${NC}"
    echo -e "${CYAN}  耗时: ${duration}s${NC}"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

# 主函数
main() {
    cd "$(dirname "$0")"
    
    case "${1:-}" in
        help|--help|-h)
            show_help
            exit 0
        ;;
        deps)
            check_dependencies
        ;;
        check)
            check_dependencies
            do_check
        ;;
        fmt)
            check_dependencies
            do_fmt
        ;;
        clippy)
            check_dependencies
            do_clippy
        ;;
        test)
            check_dependencies
            do_test
        ;;
        build)
            check_dependencies
            do_build
        ;;
        quick)
            check_dependencies
            do_fmt
            do_clippy
            do_check
            show_summary
        ;;
        full)
            check_dependencies
            do_fmt
            do_clippy
            do_check
            do_test
            do_build
            do_fulltest
            show_summary
        ;;
        "")
            # 默认：标准 CI 流程
            check_dependencies
            do_fmt
            do_clippy
            do_check
            do_test
            do_build
            show_summary
        ;;
        *)
            error "未知命令: $1"
            echo ""
            show_help
            exit 1
        ;;
    esac
}

main "$@"
