#!/bin/bash
#
# Build script for thermal-monitor .deb package
# Usage: ./build-deb.sh [--skip-build]
#

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

VERSION="1.0.0"
PACKAGE="thermal-monitor"
ARCH="amd64"
PKG_DIR="${PACKAGE}_${VERSION}_${ARCH}"

echo "=== Building thermal-monitor v${VERSION} ==="

# Build release binary unless --skip-build is passed
if [[ "$1" != "--skip-build" ]]; then
    echo "[1/5] Compiling Rust binary (release mode)..."
    cargo build --release
else
    echo "[1/5] Skipping build (using existing binary)..."
fi

# Verify binary exists
if [[ ! -f "target/release/thermal-monitor" ]]; then
    echo "ERROR: Binary not found at target/release/thermal-monitor"
    echo "Run without --skip-build to compile first"
    exit 1
fi

# Clean previous build
echo "[2/5] Preparing package directory..."
rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/DEBIAN"
mkdir -p "$PKG_DIR/usr/local/bin"
mkdir -p "$PKG_DIR/usr/share/applications"

# Copy control file
echo "[3/5] Creating package metadata..."
cp debian/control "$PKG_DIR/DEBIAN/control"

# Add installed size to control file
SIZE_KB=$(du -s target/release/thermal-monitor | cut -f1)
echo "Installed-Size: $SIZE_KB" >> "$PKG_DIR/DEBIAN/control"

# Copy binary
echo "[4/5] Copying files..."
cp target/release/thermal-monitor "$PKG_DIR/usr/local/bin/"
chmod 755 "$PKG_DIR/usr/local/bin/thermal-monitor"

# Copy desktop file
cp thermal-monitor.desktop "$PKG_DIR/usr/share/applications/"
chmod 644 "$PKG_DIR/usr/share/applications/thermal-monitor.desktop"

# Create postinst script to update desktop database
cat > "$PKG_DIR/DEBIAN/postinst" << 'EOF'
#!/bin/bash
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database /usr/share/applications 2>/dev/null || true
fi
EOF
chmod 755 "$PKG_DIR/DEBIAN/postinst"

# Create postrm script
cat > "$PKG_DIR/DEBIAN/postrm" << 'EOF'
#!/bin/bash
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database /usr/share/applications 2>/dev/null || true
fi
EOF
chmod 755 "$PKG_DIR/DEBIAN/postrm"

# Build the package
echo "[5/5] Building .deb package..."
dpkg-deb --build --root-owner-group "$PKG_DIR"

# Cleanup
rm -rf "$PKG_DIR"

# Show result
DEB_FILE="${PKG_DIR}.deb"
if [[ -f "$DEB_FILE" ]]; then
    echo ""
    echo "=== Package built successfully ==="
    ls -lh "$DEB_FILE"
    echo ""
    echo "To install:"
    echo "  sudo dpkg -i $DEB_FILE"
    echo ""
    echo "To uninstall:"
    echo "  sudo dpkg -r thermal-monitor"
else
    echo "ERROR: Package build failed"
    exit 1
fi
