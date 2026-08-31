class Magicore < Formula
  desc "Universal package manager with multi-core runtime"
  homepage "https://github.com/mingd-153/MagiCore"
  license "MIT"
  head "https://github.com/mingd-153/MagiCore.git", branch: "main"

  depends_on "rust" => :build

  # Binary releases from GitHub — update SHA256 on version bump
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/mingd-153/MagiCore/releases/download/v1.0.0-rc.2/magicore-macOS-ARM64.tar.gz"
      sha256 "UPDATE_ME"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v1.0.0-rc.2/magicore-macOS-X64.tar.gz"
      sha256 "UPDATE_ME"
    end
  end

  on_linux do
    odie "Linux ARM64 binary is not available in this RC" if Hardware::CPU.arm?

    url "https://github.com/mingd-153/MagiCore/releases/download/v1.0.0-rc.2/magicore-Linux-X64.tar.gz"
    sha256 "UPDATE_ME"
  end

  def install
    bin.install "mgc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mgc --version")
  end
end
