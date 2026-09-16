class MagicoreWeb < Formula
  desc "MagiCore single-core web package manager/runtime"
  homepage "https://github.com/mingd-153/MagiCore"
  version "1.1.0-rc.5"
  # RELEASE GATE E (P0-3, 2026-09-16): the SHA256 values below are
  # DO-NOT-PUBLISH placeholders — the v1.1.0-rc.5 archives do not exist
  # yet, so NO real hash can be computed. Publishing this manifest as-is
  # would guarantee install failure (hash mismatch). Rerun
  # scripts/update-release-hashes.sh after the v1.1.0-rc.5 release assets
  # are published, then confirm `python3 scripts/verify_release_manifests.py`
  # exits 0 — it FAILS (exit 1) while any PENDING_/UNAVAILABLE_ placeholder
  # remains.
  # (CỔNG RELEASE E (P0-3): giá trị SHA256 dưới đây là placeholder
  # CẤM-PUBLISH — archive v1.1.0-rc.5 chưa tồn tại nên KHÔNG THỂ tính hash
  # thật. Publish manifest này nguyên trạng thì install chắc chắn hỏng
  # (hash lệch). Chạy lại scripts/update-release-hashes.sh sau khi asset
  # release v1.1.0-rc.5 có thật, rồi xác nhận `python3
  # scripts/verify_release_manifests.py` exit 0 — script FAIL (exit 1)
  # khi còn bất kỳ placeholder PENDING_/UNAVAILABLE_ nào.)
  license "MIT"
  head "https://github.com/mingd-153/MagiCore.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-web-#{version}-macos-arm64.tar.gz"
      sha256 "PENDING_RC5_SHA256_DO_NOT_PUBLISH"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-web-#{version}-macos-x64.tar.gz"
      sha256 "PENDING_RC5_SHA256_DO_NOT_PUBLISH"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-web-#{version}-linux-arm64.tar.gz"
      sha256 "PENDING_RC5_UNAVAILABLE_NO_LINUX_ARM64_ARCHIVE"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v#{version}/magicore-web-#{version}-linux-x64.tar.gz"
      sha256 "PENDING_RC5_SHA256_DO_NOT_PUBLISH"
    end
  end

  def install
    bin.install "mgc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mgc --version")
  end
end
