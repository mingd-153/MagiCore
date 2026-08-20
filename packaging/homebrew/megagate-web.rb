class MegagateWeb < Formula
  desc "MegaGate single-core web package manager/runtime"
  homepage "https://github.com/mingd-153/MegaGate"
  license "MIT"
  head "https://github.com/mingd-153/MegaGate.git", branch: "main"

  depends_on "rust" => :build

  # Binary releases from GitHub — update SHA256 on version bump
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/mingd-153/MegaGate/releases/download/v0.3.0/megagate-web-macOS-ARM64.tar.gz"
      sha256 "UPDATE_ME"
    else
      url "https://github.com/mingd-153/MegaGate/releases/download/v0.3.0/megagate-web-macOS-X64.tar.gz"
      sha256 "UPDATE_ME"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/mingd-153/MegaGate/releases/download/v0.3.0/megagate-web-Linux-ARM64.tar.gz"
      sha256 "UPDATE_ME"
    else
      url "https://github.com/mingd-153/MegaGate/releases/download/v0.3.0/megagate-web-Linux-X64.tar.gz"
      sha256 "UPDATE_ME"
    end
  end

  def install
    bin.install "mg"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mg --version")
  end
end
