# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
#
# Homebrew formula for Cortex.
#
# Install:
#   brew tap agentralabs/agentic-vision
#   brew install cortex

class Cortex < Formula
  desc "Rapid web cartographer for AI agents"
  homepage "https://github.com/agentralabs/agentic-vision"
  license "Apache-2.0"
  version "0.3.4"

  on_macos do
    on_arm do
      url "https://github.com/agentralabs/agentic-vision/releases/download/v#{version}/cortex-#{version}-darwin-aarch64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_DARWIN_AARCH64"
    end

    on_intel do
      url "https://github.com/agentralabs/agentic-vision/releases/download/v#{version}/cortex-#{version}-darwin-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_DARWIN_X86_64"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/agentralabs/agentic-vision/releases/download/v#{version}/cortex-#{version}-linux-aarch64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_AARCH64"
    end

    on_intel do
      url "https://github.com/agentralabs/agentic-vision/releases/download/v#{version}/cortex-#{version}-linux-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_X86_64"
    end
  end

  def install
    bin.install "cortex"
  end

  def post_install
    (var/"cortex").mkpath
  end

  service do
    run [opt_bin/"cortex", "start", "--http-port", "7700"]
    keep_alive true
    working_dir var/"cortex"
    log_path var/"log/cortex.log"
    error_log_path var/"log/cortex-error.log"
  end

  test do
    assert_match "cortex", shell_output("#{bin}/cortex --version")
  end
end
