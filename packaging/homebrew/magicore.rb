class Magicore < Formula
  desc "Universal package manager with multi-core runtime"
  homepage "https://github.com/mingd-153/MagiCore"
  version "1.1.0-rc.1"
  license "MIT"
  head "https://github.com/mingd-153/MagiCore.git", branch: "main"

  depends_on "rust" => :build

  # Binary releases from GitHub
  # SHA256 computed via: shasum -a 256 <artifact>
  # Hashes auto-updated by scripts/update-release-hashes.sh during release CI
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-macOS-ARM64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-macOS-X64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-Linux-ARM64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-Linux-X64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "mgc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mgc --version")
  end
end
