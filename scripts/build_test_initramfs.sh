#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR=$(mktemp -d)
cd "$WORK_DIR"

EXPECTED_SHA256="51fcb60efbdf3e579550e9ab893730df56b33d0cc928a2a6467bd846cdfef7d8"

echo "Downloading static busybox..."
wget -q "https://www.busybox.net/downloads/binaries/1.31.0-defconfig-multiarch-musl/busybox-x86_64" -O busybox

ACTUAL_SHA256=$(sha256sum busybox | awk '{print $1}')
if [ "$ACTUAL_SHA256" != "$EXPECTED_SHA256" ]; then
    echo "Error: busybox checksum mismatch!"
    echo "Expected: $EXPECTED_SHA256"
    echo "Actual:   $ACTUAL_SHA256"
    exit 1
fi

chmod +x busybox

echo "Creating init script..."
cat << 'INIT_EOF' > init
#!/bin/busybox sh
/busybox --install -s
mount -t proc proc /proc
mount -t sysfs sys /sys
ifconfig eth0 192.168.100.2 netmask 255.255.255.0 up
ip route add default via 192.168.100.1
echo "Starting TCP echo server on port 9999..."
nc -l -p 9999 -e /bin/cat &
exec /bin/sh
INIT_EOF
chmod +x init

echo "Building cpio archive..."
python3 -c "
import sys, os
def write_cpio(files, out_path):
    with open(out_path, 'wb') as f:
        for path, name in files:
            with open(path, 'rb') as in_f:
                content = in_f.read()
            st = os.stat(path)
            mode = st.st_mode
            size = len(content)
            header = f'070701{st.st_ino:08X}{mode:08X}{st.st_uid:08X}{st.st_gid:08X}{st.st_nlink:08X}{int(st.st_mtime):08X}{size:08X}{0:08X}{0:08X}{0:08X}{0:08X}{len(name)+1:08X}{0:08X}'
            f.write(header.encode('ascii'))
            f.write(name.encode('utf-8') + b'\x00')
            pad_len = (4 - (len(header) + len(name) + 1) % 4) % 4
            f.write(b'\x00' * pad_len)
            f.write(content)
            pad_len = (4 - size % 4) % 4
            f.write(b'\x00' * pad_len)

        name = 'TRAILER!!!'
        header = f'070701{0:08X}{0:08X}{0:08X}{0:08X}{0:08X}{0:08X}{0:08X}{0:08X}{0:08X}{0:08X}{0:08X}{len(name)+1:08X}{0:08X}'
        f.write(header.encode('ascii'))
        f.write(name.encode('utf-8') + b'\x00')
        pad_len = (4 - (len(header) + len(name) + 1) % 4) % 4
        f.write(b'\x00' * pad_len)

write_cpio([('busybox', 'busybox'), ('init', 'init')], 'test_initramfs.cpio')
"

cp test_initramfs.cpio "$ROOT_DIR/test_initramfs.cpio"
rm -rf "$WORK_DIR"
echo "Success: $ROOT_DIR/test_initramfs.cpio"
