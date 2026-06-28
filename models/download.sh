#!/bin/bash
# 模型下载脚本
# 下载本地推理所需的模型文件

set -e

MODELS_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "=== Prometheus 模型下载工具 ==="
echo ""

# Whisper 语音识别模型
download_whisper() {
    echo "下载 Whisper 模型 (base)..."
    # TODO: 替换为实际下载地址
    # curl -L -o "$MODELS_DIR/whisper-base.bin" "https://..."
    echo "  [跳过] 请手动下载 Whisper 模型"
}

# Piper TTS 模型
download_piper() {
    echo "下载 Piper TTS 模型 (zh-CN)..."
    # TODO: 替换为实际下载地址
    echo "  [跳过] 请手动下载 Piper 模型"
}

# LLM 模型
download_llm() {
    echo "下载 LLM 模型 (Qwen2.5-7B-GGUF)..."
    # TODO: 替换为实际下载地址
    echo "  [跳过] 请手动下载 LLM 模型"
}

echo "可用模型："
echo "  1) whisper  - 语音识别"
echo "  2) piper    - 语音合成"
echo "  3) llm      - 大语言模型"
echo "  4) all      - 全部下载"
echo ""

case "${1:-all}" in
    whisper) download_whisper ;;
    piper)   download_piper ;;
    llm)     download_llm ;;
    all)
        download_whisper
        download_piper
        download_llm
        ;;
    *)
        echo "用法: $0 [whisper|piper|llm|all]"
        exit 1
        ;;
esac

echo ""
echo "完成！模型文件保存在: $MODELS_DIR"
