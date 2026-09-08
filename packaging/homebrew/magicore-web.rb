class MagicoreWeb < Formula
  desc "MagiCore single-core web package manager/runtime"
  homepage "https://github.com/mingd-153/MagiCore"
  license "MIT"
  head "https://github.com/mingd-153/MagiCore.git", branch: "main"

  depends_on "rust" => :build

  # Binary releases from GitHub — update SHA256 on version bump
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/mingd-153/MagiCore/releases/download/v1.1.0-rc.3/magicore-web-1.1.0-rc.3-macos-arm64.tar.gz"
      sha256 "PENDING_RELEASE"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v1.1.0-rc.3/magicore-web-1.1.0-rc.3-macos-x64.tar.gz"
      sha256 "PENDING_RELEASE"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/mingd-153/MagiCore/releases/download/v1.1.0-rc.3/magicore-web-1.1.0-rc.3-linux-arm64.tar.gz"
      sha256 "PENDING_RELEASE"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v1.1.0-rc.3/magicore-web-1.1.0-rc.3-linux-x64.tar.gz"
      sha256 "PENDING_RELEASE"
    end
  end

  def install
    bin.install "mgc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mgc --version")
  end
end
