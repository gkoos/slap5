#!/usr/bin/env python3
"""Turn Gomocup's published opening diagrams into a Piskvork opening list.

Gomocup publishes its tournament openings only as PNG diagrams, one per opening
(`opening_freestyle15_1.png`, ... under static/tournaments/<year>/openings/), which is why a frozen set
cannot be downloaded as coordinates. This reads those diagrams directly: it decodes the PNG with zlib
alone (there is no image library on this machine), finds the board's grid lines, and then decides each
grid intersection one at a time - a stone fills a ring around its intersection which two crossing lines
and a star point leave empty, and its colour comes from a disc at the centre. Deciding each intersection
is what makes this work where blob detection did not: the diagrams draw the grid over the stones, so two
neighbouring stones merge into one blob and a white stone's centre pixel can be a line. Output is a
Piskvork opening list for `match.ps1 -Openings`, in the same format as
`external/piskvork/bin/openings.txt`: one opening per line, `x,y` pairs relative to the board centre.

Checked against the 2026 freestyle 15x15 set: every opening's white stones account for all the white
pixels of its image (one stone is about 790 of them, and the background is yellow, so white pixels can
only be stones), every opening alternates from black with one black more than white, and the first
diagram agrees stone for stone with reading it by eye.

Stones are written alternating from black because a diagram shows the position and not the move order,
and the position is all the engine sees. The y axis keeps the image's direction, downwards, so the list is
the published set mirrored vertically; a mirror of a balanced position is equally balanced, so this does
not change the set's value, but a coordinate-by-coordinate comparison with the diagrams has to allow for
it.

Usage: python engine/scripts/openings_from_png.py <dir with the PNGs> [-o out.txt]
"""
import os
import struct
import sys
import zlib

GRID_DARK = 90      # grid lines and stone outlines are this dark
STONE_WHITE = 200   # a white stone's interior is brighter than this
THICK = 5           # a stone survives a 5x5 count of its own colour; a one-pixel line cannot


def read_png(path):
    """Return (width, height, channels, rows) for a non-interlaced 8-bit RGB or RGBA PNG."""
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path} is not a PNG")
    pos, idat = 8, b""
    width = height = colortype = None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos:pos + 4])
        ctype = data[pos + 4:pos + 8]
        chunk = data[pos + 8:pos + 8 + length]
        if ctype == b"IHDR":
            width, height, depth, colortype, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
            if depth != 8 or colortype not in (2, 6) or interlace:
                raise ValueError(f"{path}: only 8-bit non-interlaced RGB(A) is handled")
        elif ctype == b"IDAT":
            idat += chunk
        elif ctype == b"IEND":
            break
        pos += 12 + length
    raw = zlib.decompress(idat)
    channels = 3 if colortype == 2 else 4
    stride = width * channels
    rows, prev, offset = [], bytearray(stride), 0
    for _ in range(height):
        ftype = raw[offset]
        offset += 1
        line = bytearray(raw[offset:offset + stride])
        offset += stride
        for i in range(stride):
            a = line[i - channels] if i >= channels else 0
            b = prev[i]
            c = prev[i - channels] if i >= channels else 0
            if ftype == 1:
                line[i] = (line[i] + a) & 0xFF
            elif ftype == 2:
                line[i] = (line[i] + b) & 0xFF
            elif ftype == 3:
                line[i] = (line[i] + (a + b) // 2) & 0xFF
            elif ftype == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                near = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + near) & 0xFF
        rows.append(bytes(line))
        prev = line
    return width, height, channels, rows


def grid_lines(rows, channels, width, height):
    """Pixel positions of the board's vertical and horizontal lines, from their dark-pixel counts."""
    def dark(x, y):
        i = x * channels
        return rows[y][i] < GRID_DARK and rows[y][i + 1] < GRID_DARK and rows[y][i + 2] < GRID_DARK

    cols = [sum(1 for y in range(height) if dark(x, y)) for x in range(width)]
    rowc = [sum(1 for x in range(width) if dark(x, y)) for y in range(height)]
    span = max(1, height // 2)
    out_cols, out_rows = [], []
    for i, n in enumerate(cols):
        if n > span and (not out_cols or i - out_cols[-1] > 2):
            out_cols.append(i)
    for i, n in enumerate(rowc):
        if n > span and (not out_rows or i - out_rows[-1] > 2):
            out_rows.append(i)
    return out_cols, out_rows


def stones(rows, channels, width, height, vlines, hlines):
    """Every stone, decided one grid intersection at a time, as (col, row, is_white).

    Blobs do not work on these diagrams: the grid lines are drawn over the stones, so two neighbouring
    stones merge into one blob whose centre sits between two intersections, and a white stone's centre
    pixel can be a line. An intersection is checked instead. A ring 9 to 16 pixels out from it is either
    stone - a black stone's fill, or a white stone's interior and outline - or background: two crossing
    lines fill only a few percent of that ring, and a star point is too small to reach it. The colour then
    comes from a disc 8 pixels around the intersection, which the thin lines barely disturb.
    """
    ring = [(dy, dx) for dy in range(-16, 17) for dx in range(-16, 17)
            if 9 * 9 <= dy * dy + dx * dx <= 16 * 16]
    core = [(dy, dx) for dy in range(-8, 9) for dx in range(-8, 9) if dy * dy + dx * dx <= 8 * 8]
    centre_x, centre_y = (len(vlines) - 1) / 2, (len(hlines) - 1) / 2
    out = []
    for iy, y0 in enumerate(hlines):
        for ix, x0 in enumerate(vlines):
            filled = checked = 0
            for dy, dx in ring:
                x, y = x0 + dx, y0 + dy
                if not (0 <= x < width and 0 <= y < height):
                    continue
                checked += 1
                i = x * channels
                r, g, b = rows[y][i], rows[y][i + 1], rows[y][i + 2]
                if (r < GRID_DARK and g < GRID_DARK and b < GRID_DARK) or \
                   (r > STONE_WHITE and g > STONE_WHITE and b > STONE_WHITE):
                    filled += 1
            if checked == 0 or filled < 0.7 * checked:
                continue  # empty intersection, or a star point
            dark = light = 0
            for dy, dx in core:
                x, y = x0 + dx, y0 + dy
                if not (0 <= x < width and 0 <= y < height):
                    continue
                i = x * channels
                r, g, b = rows[y][i], rows[y][i + 1], rows[y][i + 2]
                if r < GRID_DARK and g < GRID_DARK and b < GRID_DARK:
                    dark += 1
                elif r > STONE_WHITE and g > STONE_WHITE and b > STONE_WHITE:
                    light += 1
            out.append((round(ix - centre_x), round(iy - centre_y), light > dark))
    return out


def parse(path):
    """One opening as a list of (col, row, is_white), coordinates relative to the board centre."""
    width, height, channels, rows = read_png(path)
    vlines, hlines = grid_lines(rows, channels, width, height)
    if len(vlines) < 5 or len(hlines) < 5:
        raise ValueError(f"{path}: found {len(vlines)}x{len(hlines)} grid lines")
    return stones(rows, channels, width, height, vlines, hlines)


def main():
    argv = sys.argv[1:]
    paths = [a for a in argv if not a.startswith("-")]
    if not paths:
        print(__doc__)
        return 1
    directory = paths[0]
    out_path = argv[argv.index("-o") + 1] if "-o" in argv else None
    names = sorted(n for n in os.listdir(directory) if n.lower().endswith(".png"))
    lines = []
    for name in names:
        opening = parse(os.path.join(directory, name))
        blacks = sorted((p for p in opening if not p[2]))
        whites = sorted((p for p in opening if p[2]))
        if len(blacks) != len(whites) + (len(opening) % 2) or not opening:
            print(f"{name}: {len(blacks)} black, {len(whites)} white - not an alternating position")
        # A diagram shows the position, not the move order, and the position is all the engine sees, so
        # the colours are written alternating from black.
        sequence = []
        for i in range(max(len(blacks), len(whites))):
            if i < len(blacks):
                sequence.append(blacks[i])
            if i < len(whites):
                sequence.append(whites[i])
        line = ", ".join(f"{c},{r}" for c, r, _ in sequence)
        lines.append(line)
        print(f"{name}: {len(sequence)} stones -> {line}")
    if out_path:
        with open(out_path, "w", encoding="ascii", newline="\n") as fh:
            fh.write("\n".join(lines) + "\n")
        print(f"wrote {len(lines)} openings to {out_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
