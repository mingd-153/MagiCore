#!/bin/bash
# Manual trigger for multi-platform release build
# This simulates what GitHub Actions does, but locally requires Docker/cross tool

set -euo pipefail

echo "=== MagiCore Multi-Platform Build Script ==="
echo ""
echo "This script prepares for multi-platform release builds."
echo "For actual cross-compilation, GitHub Actions workflow is recommended."
echo ""

VERSION="${1:-1.1.0-rc.1}"

echo "Target version: v${VERSION}"
echo ""
echo "Available build methods:"
echo "  1. GitHub Actions (recommended) - builds all 6 platforms in CI"
echo "  2. Local cross-tool - requires Docker + cross installed"
echo "  3. Native build only - current platform only"
echo ""

read -p "Choose method [1/2/3]: " choice

case $choice in
  1)
    echo ""
    echo "=== GitHub Actions Build ==="
    echo ""
    echo "To trigger GitHub Actions workflow:"
    echo ""
    echo "1. Ensure all changes are committed"
    echo "2. Create and push a tag:"
    echo "   git tag v${VERSION}"
    echo "   git push origin v${VERSION}"
    echo ""
    echo "3. Monitor workflow at:"
    echo "   https://github.com/mingd-153/MagiCore/actions"
    echo ""
    echo "4. Workflow will:"
    echo "   - Build for 6 platforms (Linux/macOS/Windows, x86_64/ARM64)"
    echo "   - Generate SHA256 checksums"
    echo "   - Update package manager manifests"
    echo "   - Create GitHub Release with all artifacts"
    echo ""
    echo "5. After release completes:"
    echo "   - Download artifacts from GitHub Release"
    echo "   - Test install on each platform"
    echo "   - Submit to Homebrew/Scoop repositories"
    echo ""
    ;;
    
  2)
    echo ""
    echo "=== Cross-Tool Build ==="
    echo ""
    
    # Check prerequisites
    if ! command -v cross &> /dev/null; then
      echo "❌ 'cross' tool not found"
      echo "Install: cargo install cross --git https://github.com/cross-rs/cross"
      exit 1
    fi
    
    if ! command -v docker &> /dev/null; then
      echo "❌ Docker not found"
      echo "Install Docker Desktop: https://www.docker.com/products/docker-desktop"
      exit 1
    fi
    
    echo "✅ Prerequisites OK"
    echo ""
    echo "Building for multiple targets (this will take 30-60 minutes)..."
    echo ""
    
    TARGETS=(
      "x86_64-unknown-linux-gnu"
      "aarch64-unknown-linux-gnu"
      "x86_64-apple-darwin"
      "aarch64-apple-darwin"
      "x86_64-pc-windows-msvc"
    )
    
    mkdir -p dist/cross-builds
    
    for target in "${TARGETS[@]}"; do
      echo "Building for ${target}..."
      
      if [[ "$target" == *"darwin"* ]] && [[ "$(uname)" != "Darwin" ]]; then
        echo "⚠️  Skipping $target (macOS targets require macOS host)"
        continue
      fi
      
      if [[ "$target" == *"windows"* ]] && [[ "$(uname)" == "Darwin" ]]; then
        echo "⚠️  Skipping $target (Windows targets difficult from macOS)"
        continue
      fi
      
      cross build --release --target "$target" --bin mgc || {
        echo "❌ Build failed for $target"
        continue
      }
      
      binary_name="mgc"
      [[ "$target" == *"windows"* ]] && binary_name="mgc.exe"
      
      cp "target/${target}/release/${binary_name}" "dist/cross-builds/mgc-${target}${binary_name##mgc}"
      shasum -a 256 "dist/cross-builds/mgc-${target}${binary_name##mgc}" > "dist/cross-builds/mgc-${target}.sha256"
      
      echo "✅ $target complete"
    done
    
    echo ""
    echo "Build artifacts:"
    ls -lh dist/cross-builds/
    ;;
    
  3)
    echo ""
    echo "=== Native Build Only ==="
    echo ""
    
    PLATFORM="$(uname)"
    ARCH="$(uname -m)"
    
    echo "Building for current platform: ${PLATFORM} ${ARCH}"
    echo ""
    
    cargo build --release --bin mgc
    
    mkdir -p "dist/magicore-${VERSION}-${PLATFORM}-${ARCH}"
    
    if [[ "$PLATFORM" == "Darwin" ]]; then
      cp target/release/mgc "dist/magicore-${VERSION}-${PLATFORM}-${ARCH}/"
      cp README.md LICENSE "dist/magicore-${VERSION}-${PLATFORM}-${ARCH}/"
      
      cd dist
      tar -czf "magicore-${VERSION}-${PLATFORM}-${ARCH}.tar.gz" "magicore-${VERSION}-${PLATFORM}-${ARCH}/"
      shasum -a 256 "magicore-${VERSION}-${PLATFORM}-${ARCH}.tar.gz" > "magicore-${VERSION}-${PLATFORM}-${ARCH}.tar.gz.sha256"
      cd ..
      
      echo ""
      echo "✅ Build complete:"
      echo "   Binary: dist/magicore-${VERSION}-${PLATFORM}-${ARCH}/mgc"
      echo "   Archive: dist/magicore-${VERSION}-${PLATFORM}-${ARCH}.tar.gz"
      echo "   Checksum: dist/magicore-${VERSION}-${PLATFORM}-${ARCH}.tar.gz.sha256"
      echo ""
      cat "dist/magicore-${VERSION}-${PLATFORM}-${ARCH}.tar.gz.sha256"
      
    elif [[ "$PLATFORM" == "Linux" ]]; then
      cp target/release/mgc "dist/magicore-${VERSION}-${PLATFORM}-${ARCH}/"
      cp README.md LICENSE "dist/magicore-${VERSION}-${PLATFORM}-${ARCH}/"
      
      cd dist
      tar -czf "magicore-${VERSION}-${PLATFORM}-${ARCH}.tar.gz" "magicore-${VERSION}-${PLATFORM}-${ARCH}/"
      sha256sum "magicore-${VERSION}-${PLATFORM}-${ARCH}.tar.gz" > "magicore-${VERSION}-${PLATFORM}-${ARCH}.tar.gz.sha256"
      cd ..
      
      echo ""
      echo "✅ Build complete:"
      echo "   Archive: dist/magicore-${VERSION}-${PLATFORM}-${ARCH}.tar.gz"
      cat "dist/magicore-${VERSION}-${PLATFORM}-${ARCH}.tar.gz.sha256"
      
    else
      echo "❌ Platform not supported: ${PLATFORM}"
      exit 1
    fi
    ;;
    
  *)
    echo "Invalid choice"
    exit 1
    ;;
esac

echo ""
echo "=== Next Steps ==="
echo ""
echo "For production release:"
echo "  1. Use GitHub Actions (method 1) for official builds"
echo "  2. Download all platform artifacts from GitHub Release"
echo "  3. Test install/uninstall on each platform"
echo "  4. Update package manager repositories (Homebrew/Scoop)"
echo "  5. Announce release"
echo ""
echo "For testing:"
echo "  - Native build (method 3) is sufficient"
echo "  - Test on current platform only"
echo "  - Skip cross-platform validation"
echo ""
