import struct, sys, io
from PIL import Image

# Read from a file or git
data = open(sys.argv[1], 'rb').read() if sys.argv[1] != '-' else sys.stdin.buffer.read()

# If it's a binary, find icns
if data[:4] != b'icns':
    start = 0
    hits = []
    while True:
        i = data.find(b'icns', start)
        if i < 0: break
        if i + 8 <= len(data):
            total = struct.unpack('>I', data[i+4:i+8])[0]
            if 100 < total < 5_000_000 and i + total <= len(data):
                hits.append((i, total))
        start = i + 1
    if not hits:
        print('No icns found in binary')
        sys.exit(1)
    off, total = max(hits, key=lambda x: x[1])
    data = data[off:off+total]

# Find ic10
pos = 8
while pos < len(data) - 8:
    et = data[pos:pos+4].decode()
    el = struct.unpack('>I', data[pos+4:pos+8])[0]
    if et == 'ic10':
        png = data[pos+8:pos+el]
        img = Image.open(io.BytesIO(png)).convert('RGBA')
        w, h = img.size
        px = img.load()
        min_x, max_x, min_y, max_y = w, 0, h, 0
        for y in range(h):
            for x in range(w):
                if px[x, y][3] > 0:
                    if x < min_x: min_x = x
                    if x > max_x: max_x = x
                    if y < min_y: min_y = y
                    if y > max_y: max_y = y
        cw = max_x - min_x + 1
        print(f'ic10: {w}x{h}, content: {cw}x{cw} ({cw/w*100:.1f}%), padding: ~{min_x/w*100:.1f}% per side')
        break
    pos += el