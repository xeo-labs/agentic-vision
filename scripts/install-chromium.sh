#!/usr/bin/env bash
# Install Chrome for Testing for Cortex
set -euo pipefail

INSTALL_DIR="${HOME}/.cortex/chromium"

echo "Cortex — Installing Chrome for Testing"
echo "======================================="
echo ""

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Linux)
        case "${ARCH}" in
            x86_64)  PLATFORM="linux64" ;;
            aarch64) PLATFORM="linux-arm64" ;;
            *)       echo "ERROR: Unsupported Linux architecture: ${ARCH}"; exit 1 ;;
        esac
        ;;
    Darwin)
        case "${ARCH}" in
            x86_64)  PLATFORM="mac-x64" ;;
            arm64)   PLATFORM="mac-arm64" ;;
            *)       echo "ERROR: Unsupported macOS architecture: ${ARCH}"; exit 1 ;;
        esac
        ;;
    *)
        echo "ERROR: Unsupported OS: ${OS}"
        exit 1
        ;;
esac

echo "Platform: ${PLATFORM}"
echo "Install directory: ${INSTALL_DIR}"
echo ""

# Check for required tools
for cmd in curl jq unzip; do
    if ! command -v "${cmd}" &>/dev/null; then
        echo "ERROR: Required tool '${cmd}' not found. Please install it."
        exit 1
    fi
done

# Fetch latest stable version info
echo "Fetching latest Chrome for Testing version..."
VERSION_JSON=$(curl -sS "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json")

if [ -z "${VERSION_JSON}" ]; then
    echo "ERROR: Failed to fetch version info. Check internet connection."
    exit 1
fi

# Extract download URL for our platform
VERSION=$(echo "${VERSION_JSON}" | jq -r '.channels.Stable.version')
DOWNLOAD_URL=$(echo "${VERSION_JSON}" | jq -r ".channels.Stable.downloads.chrome[] | select(.platform == \"${PLATFORM}\") | .url")

if [ -z "${DOWNLOAD_URL}" ] || [ "${DOWNLOAD_URL}" = "null" ]; then
    echo "ERROR: No download URL found for platform '${PLATFORM}'"
    exit 1
fi

echo "Version: ${VERSION}"
echo "URL: ${DOWNLOAD_URL}"
echo ""

# Create install directory
mkdir -p "${INSTALL_DIR}"

# Download
TEMP_DIR=$(mktemp -d)
TEMP_FILE="${TEMP_DIR}/chrome.zip"

echo "Downloading..."
curl -# -L -o "${TEMP_FILE}" "${DOWNLOAD_URL}"

if [ ! -f "${TEMP_FILE}" ]; then
    echo "ERROR: Download failed."
    rm -rf "${TEMP_DIR}"
    exit 1
fi

# Extract
echo "Extracting..."
unzip -q -o "${TEMP_FILE}" -d "${INSTALL_DIR}"

# Clean up temp
rm -rf "${TEMP_DIR}"

# Find the chrome binary and make it executable
if [ "${OS}" = "Darwin" ]; then
    CHROME_BIN=$(find "${INSTALL_DIR}" -name "Google Chrome for Testing" -type f 2>/dev/null | head -1)
    if [ -z "${CHROME_BIN}" ]; then
        CHROME_BIN=$(find "${INSTALL_DIR}" -name "chrome" -type f 2>/dev/null | head -1)
    fi
else
    CHROME_BIN=$(find "${INSTALL_DIR}" -name "chrome" -type f 2>/dev/null | head -1)
fi

if [ -n "${CHROME_BIN}" ]; then
    chmod +x "${CHROME_BIN}"
    echo ""
    echo "Chrome for Testing installed successfully!"
    echo "Binary: ${CHROME_BIN}"
    echo "Version: ${VERSION}"
    echo ""
    echo "Run 'cortex doctor' to verify."
else
    echo ""
    echo "WARNING: Chrome binary not found after extraction."
    echo "Contents of ${INSTALL_DIR}:"
    ls -la "${INSTALL_DIR}"
fi
