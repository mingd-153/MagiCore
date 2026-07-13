class Megagate < Formula
  desc "Multi-core runtime gate with web, AI, game, and IoT adapters"
  homepage "https://github.com/mingd-153/MegaGate"
  license "MIT"
  head "https://github.com/mingd-153/MegaGate.git", branch: "main"

  depends_on "rust" => :build

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/mingd-153/MegaGate/releases/latest/download/megagate-macOS-ARM64.tar.gz"
    else
      url "https://github.com/mingd-153/MegaGate/releases/latest/download/megagate-macOS-X64.tar.gz"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/mingd-153/MegaGate/releases/latest/download/megagate-Linux-ARM64.tar.gz"
    else
      url "https://github.com/mingd-153/MegaGate/releases/latest/download/megagate-Linux-X64.tar.gz"
    end
  end

  def install
    bin.install "mg"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mg --version")
  end
end
