class Magicore < Formula
  desc "Universal package manager with multi-core runtime"
  homepage "https://github.com/mingd-153/MagiCore"
  version "1.1.0-rc.1"
  license "MIT"
  head "https://github.com/mingd-153/MagiCore.git", branch: "main"

  depends_on "rust" => :build

  # Binary releases from GitHub
  # SHA256 will be computed during CI release workflow
  # Hashes auto-updated by scripts/update-release-hashes.sh
  # DO NOT install from this formula until after release CI completes
  #
  # ⚠️  PLACEHOLDER HASHES - Release CI will replace with real SHA256
  # Manual install not supported - wait for GitHub Release artifacts
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-macOS-ARM64.tar.gz"
      sha256 "PLACEHOLDER_WILL_BE_REPLACED_BY_CI"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-macOS-X64.tar.gz"
      sha256 "PLACEHOLDER_WILL_BE_REPLACED_BY_CI"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-Linux-ARM64.tar.gz"
      sha256 "PLACEHOLDER_WILL_BE_REPLACED_BY_CI"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-Linux-X64.tar.gz"
      sha256 "PLACEHOLDER_WILL_BE_REPLACED_BY_CI"
    end
  end

  def install
    bin.install "mgc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mgc --version")
  end
end
