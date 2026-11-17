#!/usr/bin/env python3
"""Converts binary PPM (P6) files to PNG with only the standard library.
usage: ppm2png.py OUT.png IN.ppm"""
import struct, sys, zlib

def convert(src, dst):
    data = open(src, "rb").read()
    magic, dims, maxval, px = data.split(b"\n", 3)
    assert magic == b"P6" and maxval == b"255", src
    w, h = map(int, dims.split())
    raw = b"".join(b"\x00" + px[y * w * 3:(y + 1) * w * 3] for y in range(h))
    chunk = lambda t, b: struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    open(dst, "wb").write(png)

if __name__ == "__main__":
    convert(sys.argv[2], sys.argv[1])
