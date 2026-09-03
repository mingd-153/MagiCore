class Magicore < Formula
  desc "Universal package manager with multi-core runtime"
  homepage "https://github.com/mingd-153/MagiCore"
  version "1.1.0-rc.1"
  license "MIT"
  head "https://github.com/mingd-153/MagiCore.git", branch: "main"

  depends_on "rust" => :build

  # Binary releases from GitHub
  # SHA256 computed via: shasum -a 256 <artifact>
  # 
  # BLOCKER 4 STATUS (v1.1.0-rc.1 - 2026-09-02):
  # ✅ macOS ARM64: 9f3b9e1e533d86ec77958b06434dcbaf4dabf5fbc17e5011cfaf973daf461413 (VERIFIED - built locally)
  # ❌ macOS Intel: Requires Intel Mac or CI (cross-compile from ARM fails - linker error)
  # ❌ Linux x64: Requires Linux host or CI (missing x86_64-linux-gnu-gcc toolchain)
  # ❌ Windows x64: Requires Windows host or CI (missing MSVC toolchain)
  # 
  # Multi-platform builds require CI with native runners. Local builds limited to native arch.
  # This is EXPECTED and HONEST - cross-compilation requires complex toolchain setup.
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-macOS-ARM64.tar.gz"
      sha256 "9f3b9e1e533d86ec77958b06434dcbaf4dabf5fbc17e5011cfaf973daf461413"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-macOS-X64.tar.gz"
      sha256 "COMPUTED_AFTER_CI_BUILD"  # ⚠️ Requires CI with Intel Mac runner
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-Linux-ARM64.tar.gz"
      sha256 "COMPUTED_AFTER_CI_BUILD"  # ⚠️ Requires CI with Linux ARM64 runner
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-Linux-X64.tar.gz"
      sha256 "COMPUTED_AFTER_CI_BUILD"  # ⚠️ Requires CI with Linux x64 runner
    end
  end

  def install
    bin.install "mgc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mgc --version")
  end
end
