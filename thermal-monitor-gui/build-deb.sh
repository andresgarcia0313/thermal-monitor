#!/bin/bash
#
# Build script for thermal-monitor .deb package
# Usage: ./build-deb.sh [--skip-build]
#

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

VERSION="1.1.0"
PACKAGE="thermal-monitor"
ARCH="amd64"
PKG_DIR="${PACKAGE}_${VERSION}_${ARCH}"

echo "=== Building thermal-monitor v${VERSION} ==="

# Build release binary unless --skip-build is passed
if [[ "$1" != "--skip-build" ]]; then
    echo "[1/6] Compiling Rust binary (release mode)..."
    cargo build --release
else
    echo "[1/6] Skipping build (using existing binary)..."
fi

# Verify binary exists
if [[ ! -f "target/release/thermal-monitor" ]]; then
    echo "ERROR: Binary not found at target/release/thermal-monitor"
    echo "Run without --skip-build to compile first"
    exit 1
fi

# Clean previous build
echo "[2/6] Preparing package directory..."
rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/DEBIAN"
mkdir -p "$PKG_DIR/usr/local/bin"
mkdir -p "$PKG_DIR/usr/share/applications"
mkdir -p "$PKG_DIR/etc/systemd/system"

# Copy control file
echo "[3/6] Creating package metadata..."
cat > "$PKG_DIR/DEBIAN/control" << EOF
Package: thermal-monitor
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: amd64
Depends: libc6 (>= 2.34), libgcc-s1 (>= 4.2), libgl1, libwayland-client0, libx11-6, libxcursor1, libxi6, libxkbcommon0, policykit-1
Maintainer: Andres Garcia <andresgarcia0313@gmail.com>
Homepage: https://github.com/andresgarcia0313
Description: Thermal monitoring GUI with CPU mode control
 A thermal monitoring application for Linux laptops that displays
 CPU and estimated keyboard temperatures with real-time updates.
 .
 Features:
  - Real-time CPU temperature monitoring
  - Keyboard temperature estimation using physics model
  - Thermal zone color coding (Cool to Critical)
  - CPU performance mode control (Performance/Comfort/Balanced/Quiet/Auto)
  - Target temperature setting with visual alerts
  - Temperature history graph (2 minute rolling window)
  - Automatic thermal management service
 .
 Compatible with Intel and AMD processors on most Linux distributions.
EOF

# Add installed size
SIZE_KB=$(du -s target/release/thermal-monitor | cut -f1)
echo "Installed-Size: $SIZE_KB" >> "$PKG_DIR/DEBIAN/control"

# Copy binary
echo "[4/6] Copying files..."
cp target/release/thermal-monitor "$PKG_DIR/usr/local/bin/"
chmod 755 "$PKG_DIR/usr/local/bin/thermal-monitor"

# Copy scripts
cp scripts/cpu-mode "$PKG_DIR/usr/local/bin/"
chmod 755 "$PKG_DIR/usr/local/bin/cpu-mode"

cp scripts/thermal-manager.sh "$PKG_DIR/usr/local/bin/"
chmod 755 "$PKG_DIR/usr/local/bin/thermal-manager.sh"

# Copy systemd files
cp systemd/thermal-manager.service "$PKG_DIR/etc/systemd/system/"
cp systemd/thermal-manager.timer "$PKG_DIR/etc/systemd/system/"
chmod 644 "$PKG_DIR/etc/systemd/system/"*

# Copy desktop file
cp thermal-monitor.desktop "$PKG_DIR/usr/share/applications/"
chmod 644 "$PKG_DIR/usr/share/applications/thermal-monitor.desktop"

# Create postinst script
echo "[5/6] Creating maintainer scripts..."
cat > "$PKG_DIR/DEBIAN/postinst" << 'EOF'
#!/bin/bash
set -e

# Update desktop database
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database /usr/share/applications 2>/dev/null || true
fi

# Reload systemd
systemctl daemon-reload 2>/dev/null || true

# Enable thermal manager timer (but don't start automatically)
systemctl enable thermal-manager.timer 2>/dev/null || true

# Create initial mode file
echo "unknown" > /tmp/cpu-mode.current 2>/dev/null || true
chmod 666 /tmp/cpu-mode.current 2>/dev/null || true

echo ""
echo "Thermal Monitor installed successfully!"
echo ""
echo "To start automatic thermal management:"
echo "  sudo systemctl start thermal-manager.timer"
echo ""
echo "To change CPU mode manually:"
echo "  sudo cpu-mode performance  # Max performance"
echo "  sudo cpu-mode comfort      # Cool keyboard"
echo "  sudo cpu-mode balanced     # General use"
echo "  sudo cpu-mode quiet        # Silent"
echo "  sudo cpu-mode auto         # Automatic"
echo ""
EOF
chmod 755 "$PKG_DIR/DEBIAN/postinst"

# Create postrm script
cat > "$PKG_DIR/DEBIAN/postrm" << 'EOF'
#!/bin/bash
set -e

if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
    # Stop and disable services
    systemctl stop thermal-manager.timer 2>/dev/null || true
    systemctl disable thermal-manager.timer 2>/dev/null || true
    systemctl daemon-reload 2>/dev/null || true

    # Update desktop database
    if command -v update-desktop-database &> /dev/null; then
        update-desktop-database /usr/share/applications 2>/dev/null || true
    fi

    # Clean up
    rm -f /tmp/cpu-mode.current 2>/dev/null || true
fi
EOF
chmod 755 "$PKG_DIR/DEBIAN/postrm"

# Build the package
echo "[6/6] Building .deb package..."
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
