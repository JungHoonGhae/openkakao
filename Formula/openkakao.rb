class Openkakao < Formula
  desc "Unofficial KakaoTalk CLI client for macOS"
  homepage "https://github.com/JungHoonGhae/openkakao"
  url "https://github.com/JungHoonGhae/openkakao/archive/refs/tags/v0.7.1.tar.gz"
  sha256 "4f43c1650f12d46f5765de84a2468098d0208c92bbc588e546c01fd8280d20e0"
  license "MIT"
  head "https://github.com/JungHoonGhae/openkakao.git", branch: "main"

  depends_on "rust" => :build

  def install
    cd "openkakao-rs" do
      system "cargo", "install", *std_cargo_args
    end

    # Install shell completions
    generate_completions_from_executable(bin/"openkakao-rs", "completions")
  end

  test do
    assert_match "openkakao-rs #{version}", shell_output("#{bin}/openkakao-rs --version")
  end
end
