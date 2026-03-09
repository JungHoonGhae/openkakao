class Openkakao < Formula
  desc "Unofficial KakaoTalk CLI client for macOS"
  homepage "https://github.com/JungHoonGhae/openkakao"
  url "https://github.com/JungHoonGhae/openkakao/archive/refs/tags/v0.7.2.tar.gz"
  sha256 "fb82c218a6a582ca2061402c6beecf94b52dafb8338959230e3663586a02f8c2"
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
