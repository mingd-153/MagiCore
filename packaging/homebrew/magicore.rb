class Magicore < Formula
  desc "Multi-language package manager and lifecycle orchestrator"
  homepage "https://github.com/mingd-153/MagiCore"
  version "1.1.0-rc.5"
  # PENDING rc.5: hashes below are still the rc.3 release values — rerun scripts/update-release-hashes.sh after tagging v1.1.0-rc.5.
  license "MIT"
  head "https://github.com/mingd-153/MagiCore.git", branch: "main"

  # Binary releases from GitHub — SHA256 values below are the REAL hashes
  # computed locally from the v1.1.0-rc.3 draft release archives
  # (2026-09-12) and cross-checked against the published *.sha256 assets.
  # (P0-4 fix: no placeholder hashes in the repo copy.)
  #
  # Bản binary từ GitHub — SHA256 dưới đây là hash THẬT tính từ archive
  # release v1.1.0-rc.3 (2026-09-12), đối chiếu với asset *.sha256 đã publish.
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-#{version}-macos-arm64.tar.gz"
      sha256 "0efe2338df6bb52b0fb256f2cf56d2f765844fec8f8f31f6bcfb1da07d33b93c"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-#{version}-macos-x64.tar.gz"
      sha256 "7fb5a180d92ecbaac257c0e4a094d7a2d4e41911eb8a41211f0cb40239373026"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-#{version}-linux-arm64.tar.gz"
      sha256 "UNAVAILABLE_NO_LINUX_ARM64_ARCHIVE_IN_RC3"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-#{version}-linux-x64.tar.gz"
      sha256 "d33d9c592c1479c92e3814f58d466175e4269c64da46738ef70f709ac0e4cd79"
    end
  end

  def install
    bin.install "mgc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mgc --version")
  end
end
