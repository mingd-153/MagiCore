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
  # BLOCKER 3 STATUS (v1.1.0-rc.1):
  # ✅ macOS ARM64: 54be70e838b20bf721f3ba4b68c89477ab67e61c784e7c8f4657defa2d6330c9 (VERIFIED)
  # ❌ macOS Intel: COMPUTED_AFTER_ARTIFACT_BUILD (NOT INSTALLABLE - pending CI)
  # ❌ Linux ARM64: COMPUTED_AFTER_ARTIFACT_BUILD (NOT INSTALLABLE - pending CI)
  # ❌ Linux x64: COMPUTED_AFTER_ARTIFACT_BUILD (NOT INSTALLABLE - pending CI)
  # 
  # Only macOS ARM64 can be installed via Homebrew at this RC stage.
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-macOS-ARM64.tar.gz"
      sha256 "54be70e838b20bf721f3ba4b68c89477ab67e61c784e7c8f4657defa2d6330c9"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-macOS-X64.tar.gz"
      sha256 "COMPUTED_AFTER_ARTIFACT_BUILD"  # ⚠️ NOT INSTALLABLE - brew install will FAIL with invalid hash
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-Linux-ARM64.tar.gz"
      sha256 "COMPUTED_AFTER_ARTIFACT_BUILD"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-Linux-X64.tar.gz"
      sha256 "COMPUTED_AFTER_ARTIFACT_BUILD"
    end
  end

  def install
    bin.install "mgc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mgc --version")
  end
end
