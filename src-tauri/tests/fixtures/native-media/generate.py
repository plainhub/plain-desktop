import hashlib
import json
from pathlib import Path
import struct
import wave
import zlib


SEED_SHA256 = 'f4895ad906550955bc4f114727839d6a39dab99c2d6dfa2bbeeaf96ccb586369'


def element(tag, value):
    return bytes.fromhex(tag) + (len(value) | (1 << 56)).to_bytes(8, 'big') + value


def uint(tag, value):
    return element(tag, value.to_bytes(8, 'big'))


def image(path):
    width, height = 32, 24
    samples = b''.join(b'\x00' + bytes(value for x in range(width)
                                     for value in (x * 7, y * 9, (x + y) * 4)) for y in range(height))

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))

    path.write_bytes(b'\x89PNG\r\n\x1a\n' +
                     chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)) +
                     chunk(b'IDAT', zlib.compress(samples)) + chunk(b'IEND', b''))
    return {'width': width, 'height': height, 'sample_bytes': width * height * 3}


def video(path, count=768):
    seed = Path(__file__).with_name('keyframes.ivf').read_bytes()
    if hashlib.sha256(seed).hexdigest() != SEED_SHA256:
        raise ValueError('Synthetic VP8 seed identity changed')
    if seed[:4] != b'DKIF' or seed[8:12] != b'VP80':
        raise ValueError('Expected the generated VP8 IVF seed')
    width, height = struct.unpack_from('<HH', seed, 12)
    frames = []
    cursor = 32
    while cursor < len(seed):
        size = struct.unpack_from('<I', seed, cursor)[0]
        frame = seed[cursor + 12:cursor + 12 + size]
        if len(frame) != size or frame[0] & 1 or frame[3:6] != b'\x9d\x01\x2a':
            raise ValueError('Seed must contain complete independent VP8 keyframes')
        frames.append(frame)
        cursor += 12 + size
    if len(frames) != 2 or frames[0] == frames[1]:
        raise ValueError('Expected two distinct synthetic keyframes')
    rate = 12
    info = element('1549a966', uint('2ad7b1', 1000000) +
                   element('4489', struct.pack('>d', count * 1000 / rate)) +
                   element('4d80', b'plain synthetic fixture') + element('5741', b'plain synthetic fixture'))
    track = uint('d7', 1) + uint('73c5', 1) + uint('83', 1) + uint('9c', 0)
    track += element('86', b'V_VP8') + uint('23e383', 1000000000 // rate)
    track += element('e0', uint('b0', width) + uint('ba', height))
    tracks = element('1654ae6b', element('ae', track))
    clusters = [element('1f43b675', uint('e7', round(i * 1000 / rate)) +
                        element('a3', b'\x81\x00\x00\x80' + frames[i % 2])) for i in range(count)]

    def seek(position):
        return element('114d9b74', element('4dbb', element('53ab', bytes.fromhex('1c53bb6b')) + uint('53ac', position)))

    offset = len(seek(0)) + len(info) + len(tracks)
    cues = []
    for i, cluster in enumerate(clusters):
        if i % rate == 0:
            cues.append(element('bb', uint('b3', round(i * 1000 / rate)) +
                                element('b7', uint('f7', 1) + uint('f1', offset))))
        offset += len(cluster)
    cues = element('1c53bb6b', b''.join(cues))
    header = element('1a45dfa3', uint('4286', 1) + uint('42f7', 1) + uint('42f2', 4) +
                     uint('42f3', 8) + element('4282', b'webm') + uint('4287', 2) + uint('4285', 2))
    body = seek(offset) + info + tracks + b''.join(clusters) + cues
    path.write_bytes(header + element('18538067', body))
    return {'frames': count, 'duration': count / rate, 'width': width, 'height': height,
            'sample_bytes': sum(len(frames[i % 2]) for i in range(count)), 'seed_sha256': SEED_SHA256}


def generate(root):
    root = Path(root)
    result = {'video.webm': video(root / 'video.webm'),
              'small-video.webm': video(root / 'small-video.webm', 96),
              'image.png': image(root / 'image.png')}
    with wave.open(str(root / 'audio.wav'), 'wb') as wav:
        wav.setparams((2, 2, 48000, 0, 'NONE', 'not compressed'))
        # Distinct PCM samples keep the fixture independent of the media decoder.
        block = b''.join(struct.pack('<hh', (i % 480 - 240) * 32, (240 - i % 480) * 32) for i in range(48000))
        for _ in range(210):
            wav.writeframesraw(block)
    with wave.open(str(root / 'control.wav'), 'wb') as wav:
        wav.setparams((2, 2, 48000, 0, 'NONE', 'not compressed'))
        for _ in range(4):
            wav.writeframesraw(block)
    result['audio.wav'] = {'duration': 210, 'sample_bytes': 210 * 48000 * 4}
    result['control.wav'] = {'duration': 4, 'sample_bytes': 4 * 48000 * 4}
    for name, item in result.items():
        data = (root / name).read_bytes()
        large = name in ['video.webm', 'audio.wav']
        if large and (len(data) <= 32 * 1024 * 1024 or item['sample_bytes'] <= 32 * 1024 * 1024):
            raise ValueError('Large fixture must contain more than 32 MiB of actual coded samples')
        if not large and not 0 < len(data) <= 32 * 1024 * 1024:
            raise ValueError('Native-scheme fixture must fit the production response limit')
        item['expected_route'] = 'production-core-http-fs' if large else 'production-native-scheme'
        item.update(size=len(data), sha256=hashlib.sha256(data).hexdigest())
    (root / 'fixtures.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


if __name__ == '__main__':
    import sys
    print(json.dumps(generate(sys.argv[1]), indent=2))
