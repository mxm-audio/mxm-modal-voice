"""Generate the port's data files from Elements' C++ tables.

    python -I gen/tables.py <elements-source>

<elements-source> is the folder holding `elements/` and `stmlib/` at the pinned commits (the
crate's AGENTS.md names them). It writes, under the crate's `src/`:

    resources.rs                         from elements/resources.cc (sizes checked with resources.h)
    stmlib/dsp/units_tables.rs           from stmlib/dsp/units.{h,cc}

then runs rustfmt over them. The files are committed and never edited by hand.

The values are the C++ compiler's, to the bit. An integer is its literal. A float table's
literals carry no `f` suffix, so each is a `double` (the decimal rounded once, to 53 bits) that
the initialisation then rounds to `float`: two roundings, as MSVC compiles it, which this does
with Python's float() and struct's 'f'. Each float is written as `f32::from_bits`, so the Rust
compiler does no rounding of its own. Where rounding the decimal straight to f32 would have given
another float, the generator says so (it never has, at the pinned commits). The samples
(`smp_sample_data`, `smp_noise_sample`) are int16 literals, as `samples.py` wrote them from
`elements/samples/*.wav`.
"""

import fractions
import pathlib
import re
import struct
import subprocess
import sys

EURORACK_COMMIT = "f4cfa7697095a28d292bff758beb51deb49b5a86"
STMLIB_COMMIT = "e3bd7c9cc00e4364166f9905c0509b6ffd0535ec"

CRATE = pathlib.Path(__file__).resolve().parents[1]
SRC = CRATE / "src"

RUST_TYPES = {
    "uint8_t": "u8",
    "int8_t": "i8",
    "uint16_t": "u16",
    "int16_t": "i16",
    "int32_t": "i32",
    "uint32_t": "u32",
    "size_t": "usize",
    "float": "f32",
}
RANGES = {
    "u8": (0, 0xFF),
    "i8": (-0x80, 0x7F),
    "u16": (0, 0xFFFF),
    "i16": (-0x8000, 0x7FFF),
    "i32": (-0x80000000, 0x7FFFFFFF),
    "u32": (0, 0xFFFFFFFF),
    "usize": (0, 0xFFFFFFFF),
}

# An array of values: `const T name[N] = { ... };` (N optional).
ARRAY = re.compile(
    r"const\s+(\w+)\s+(\w+)\s*\[\s*(\d*)\s*\]\s*=\s*\{([^{}]*)\}\s*;", re.S)
# An array of pointers to arrays: `const T* name[] = { a, b, };` (Elements' have no second const).
POINTERS = re.compile(
    r"const\s+(\w+)\s*\*\s*(?:const\s+)?(\w+)\s*\[\s*\]\s*=\s*\{([^{}]*)\}\s*;", re.S)
# `extern const T name[N];` in a header.
DECLARATION = re.compile(r"extern\s+const\s+(\w+)\s+(\w+)\s*\[\s*(\d*)\s*\]\s*;")


def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
    return re.sub(r"//[^\n]*", " ", text)


def header_notice(path):
    """The source file's opening comment block (its copyright line and the MIT notice)."""
    lines = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("//"):
            break
        lines.append(line.rstrip())
    while lines and lines[-1].strip("/ -") == "":
        lines.pop()
    return lines


def c_float(literal):
    """The f32 bits the C++ compiler gives a `float` initialised from an unsuffixed literal."""
    double = float(literal)  # correctly rounded decimal -> double, as the compiler parses it
    bits = struct.unpack("<I", struct.pack("<f", double))[0]  # double -> float, to nearest
    direct = f32_bits_of_fraction(fractions.Fraction(literal))
    if direct != bits:
        print(f"note: {literal} rounds to {bits:#010x} through double, {direct:#010x} directly")
    return bits


def f32_bits_of_fraction(x):
    """Round an exact rational to the nearest f32 (ties to even): the single rounding, for the
    check in c_float only."""
    if x == 0:
        return 0
    sign = 0x80000000 if x < 0 else 0
    x = abs(x)
    exponent = x.numerator.bit_length() - x.denominator.bit_length()
    if fractions.Fraction(2) ** exponent > x:
        exponent -= 1
    exponent = max(exponent, -126)  # subnormals share the smallest normal exponent
    scaled = x / fractions.Fraction(2) ** (exponent - 23)  # 24 bits of significand
    significand = scaled.numerator // scaled.denominator
    remainder = scaled - significand
    if remainder > fractions.Fraction(1, 2) or (
            remainder == fractions.Fraction(1, 2) and significand & 1):
        significand += 1
    if significand == 1 << 24:
        significand >>= 1
        exponent += 1
    if significand < 1 << 23:  # subnormal
        return sign | significand
    return sign | ((exponent + 127) << 23) | (significand - (1 << 23))


def parse_values(c_type, body):
    items = [item.strip() for item in body.split(",")]
    items = [item for item in items if item]
    rust = RUST_TYPES[c_type]
    if rust == "f32":
        return [c_float(item) for item in items]
    values = [int(item, 0) for item in items]
    low, high = RANGES[rust]
    for value in values:
        if not low <= value <= high:
            sys.exit(f"{value} does not fit {c_type}")
    return values


def parse_source(path):
    """Every value array and pointer array in a .cc file, in the file's order."""
    text = strip_comments(path.read_text(encoding="utf-8"))
    found = []
    for match in ARRAY.finditer(text):
        c_type, name, size, body = match.groups()
        values = parse_values(c_type, body)
        found.append((match.start(), "array", name, c_type, int(size) if size else None, values))
    for match in POINTERS.finditer(text):
        c_type, name, body = match.groups()
        targets = [item.strip() for item in body.split(",") if item.strip()]
        found.append((match.start(), "pointers", name, c_type, None, targets))
    found.sort()
    return [entry[1:] for entry in found]


def declared_sizes(header):
    """`extern const T name[N];` sizes and `#define NAME_SIZE n` sizes from a header."""
    text = strip_comments(header.read_text(encoding="utf-8"))
    sizes = {}
    for _, name, size in DECLARATION.findall(text):
        if size:
            sizes[name] = int(size)
    for name, size in re.findall(r"#define\s+(\w+)_SIZE\s+(\d+)", text):
        sizes.setdefault(name.lower(), int(size))
    return sizes


def rust_name(name):
    return name.upper()


def emit_values(rust, values, hex_bytes):
    if rust == "f32":
        return ", ".join(f"0x{bits:08x}" for bits in values)
    if rust == "u8" and hex_bytes:
        return ", ".join(f"0x{value:02x}" for value in values)
    return ", ".join(str(value) for value in values)


def emit_tables(entries, sizes, element_types, source_label, hex_bytes=False):
    """Rust items for the parsed entries; sizes pad an array to its declared length (C++ zeroes
    the rest), and a declared size smaller than the initialiser is an error."""
    out = []
    uses_f32 = False
    for kind, name, c_type, size, values in entries:
        rust = RUST_TYPES[c_type]
        if kind == "array":
            length = len(values)
            declared = size if size is not None else sizes.get(name)
            if declared is not None:
                if declared < length:
                    sys.exit(f"{name}: {length} values for {declared} elements")
                values = values + [0] * (declared - length)
                if declared > length:
                    out.append(f"/// `{name}` in {source_label}: {length} values in an array")
                    out.append(f"/// declared `[{declared}]`, which C++ fills out with zeros.")
            else:
                out.append(f"/// `{name}` in {source_label}.")
            element_types[name] = rust
            if rust == "f32":
                uses_f32 = True
                out.append(f"pub static {rust_name(name)}: [f32; {len(values)}] = "
                           f"f32s([{emit_values(rust, values, hex_bytes)}]);")
            else:
                out.append(f"pub static {rust_name(name)}: [{rust}; {len(values)}] = "
                           f"[{emit_values(rust, values, hex_bytes)}];")
        else:
            out.append(f"/// `{name}` in {source_label}: pointers to the tables, in order.")
            refs = ", ".join(f"&{rust_name(target)}" for target in values)
            for target in values:
                if element_types.get(target) != rust:
                    sys.exit(f"{name}: {target} is not a {c_type} table")
            out.append(f"pub static {rust_name(name)}: [&[{rust}]; {len(values)}] = [{refs}];")
        out.append("")
    return out, uses_f32


F32S = """
/// The f32 whose bits are each `u32`: the C++ compiler's value of each literal, exactly.
const fn f32s<const N: usize>(bits: [u32; N]) -> [f32; N] {
    let mut values = [0.0; N];
    let mut i = 0;
    while i < N {
        values[i] = f32::from_bits(bits[i]);
        i += 1;
    }
    values
}
"""


def file_head(notice, doc):
    head = [f"{line}" for line in notice]
    head.append("")
    head.extend(f"//! {line}" if line else "//!" for line in doc)
    head.append("")
    return head


def write(path, lines):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {path.relative_to(CRATE)}")


def generate_resources(source):
    cc = source / "elements" / "resources.cc"
    entries = parse_source(cc)
    sizes = declared_sizes(source / "elements" / "resources.h")
    for kind, name, _, _, values in entries:
        if kind == "array" and sizes.get(name) not in (None, len(values)):
            sys.exit(f"{name}: {len(values)} values, resources.h says {sizes[name]}")
    body, uses_f32 = emit_tables(entries, {}, {}, "`elements/resources.cc`")
    doc = [
        "Ported from elements/resources.{h,cc}: Elements' tables and samples, every one, with its",
        "C++ name in upper case and its C++ element type.",
        "",
        "**Generated** by `gen/tables.py` from `elements/resources.cc` at `pichenettes/eurorack`",
        f"`{EURORACK_COMMIT}`; never edited by hand. A float is the C++",
        "compiler's value of its literal (the decimal rounded to `double`, then to `float`),",
        "written as its bits.",
    ]
    lines = file_head(header_notice(cc), doc)
    if uses_f32:
        lines.append(F32S.strip())
        lines.append("")
    lines.extend(body)
    write(SRC / "resources.rs", lines)


def generate_units(source):
    cc = source / "stmlib" / "dsp" / "units.cc"
    sizes = declared_sizes(source / "stmlib" / "dsp" / "units.h")
    entries = parse_source(cc)
    body, _ = emit_tables(entries, sizes, {}, "`stmlib/dsp/units.cc`")
    doc = [
        "Ported from stmlib/dsp/units.cc: the two tables of `SemitonesToRatio`",
        "(`super::units` holds the rest of units.{h,cc}).",
        "",
        "**Generated** by `gen/tables.py` from `stmlib/dsp/units.cc` at `pichenettes/stmlib`",
        f"`{STMLIB_COMMIT}`; never edited by hand. A float is the C++",
        "compiler's value of its literal (the decimal rounded to `double`, then to `float`),",
        "written as its bits.",
    ]
    lines = file_head(header_notice(cc), doc)
    lines.append(F32S.strip())
    lines.append("")
    lines.extend(body)
    write(SRC / "stmlib" / "dsp" / "units_tables.rs", lines)


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    source = pathlib.Path(sys.argv[1]).resolve()
    if not (source / "elements" / "resources.cc").exists():
        sys.exit(f"no elements/resources.cc under {source}")
    generate_resources(source)
    generate_units(source)
    files = [SRC / "resources.rs", SRC / "stmlib" / "dsp" / "units_tables.rs"]
    subprocess.run(["rustfmt", "--edition", "2024", *map(str, files)], check=True)
    print("formatted with rustfmt")


if __name__ == "__main__":
    main()
