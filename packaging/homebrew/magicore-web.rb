class MagicoreWeb < Formula
  desc "MagiCore single-core web package manager/runtime"
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
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-web-#{version}-macos-arm64.tar.gz"
      sha256 "2a8e91f4be569ca636b52f1196f7c691fec356b551c1bb83296975d2d3e6613f"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-web-#{version}-macos-x64.tar.gz"
      sha256 "eaac28638572806215c66dff29f132accd37d54ee3523e65567d063cc0b093ee"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-web-#{version}-linux-arm64.tar.gz"
      sha256 "UNAVAILABLE_NO_LINUX_ARM64_ARCHIVE_IN_RC3"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-web-#{version}-linux-x64.tar.gz"
      sha256 "56961fa3f4d6a9770551cadb0cb3393f2d6440e70da7e249441787f183944f85"
    end
  end

  def install
    bin.install "mgc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mgc --version")
  end
end
