"""Python の DLL から、MinGW（Rust の GNU ツールチェーン）用の取り込みライブラリ（lib<名前>.dll.a）を作る。

    python make_import_lib.py <python3XY.dll> <出力フォルダ>

DLL に直接つなぐ（取り込みライブラリなしでリンクする）と、Python のデータ（Py_None・例外の型など）への参照を、
読み込むときに 32 ビットのずれで書き換える仕組み（auto-import の pseudo relocation）が使われる。DLL どうしが
2 GB 以上離れて置かれると、これがあふれて物理コアが読み込みで落ちる（どこに置かれるかは Windows が決めるので、
落ちたり落ちなかったりする。X-07）。取り込みライブラリでつなげば、この書き換えは要らない。

データかどうかは、書き出された名前の場所（実行できる区画か）で見分ける。Microsoft の lib と同じ形（短い取り込みオブジェクトと、
この DLL の取り込みの記述・表の終わりの印）で書く（dlltool はアセンブラが要るので使わない）。
標準ライブラリだけで動く（パッケージに同梱の Python でも動かせる）。
"""
import struct
import sys
from pathlib import Path


def exports(dll):
    b = dll.read_bytes()
    pe = struct.unpack_from("<I", b, 0x3C)[0]
    nsec, opt = struct.unpack_from("<H", b, pe + 6)[0], struct.unpack_from("<H", b, pe + 20)[0]
    oh = pe + 24
    exp_rva = struct.unpack_from("<I", b, oh + 112)[0]       # PE32+ のデータディレクトリ 0（エクスポート）
    secs = []
    for i in range(nsec):
        o = oh + opt + 40 * i
        vs, va, raw = struct.unpack_from("<III", b, o + 8)[0], struct.unpack_from("<I", b, o + 12)[0], struct.unpack_from("<I", b, o + 20)[0]
        ch = struct.unpack_from("<I", b, o + 36)[0]
        secs.append((va, max(vs, 1), raw, ch))

    def sec_of(rva):
        for va, vs, raw, ch in secs:
            if va <= rva < va + vs:
                return va, raw, ch
        raise ValueError(f"RVA {rva:#x} の区画がありません")

    def off(rva):
        va, raw, _ = sec_of(rva)
        return rva - va + raw

    e = off(exp_rva)
    n_funcs, n_names = struct.unpack_from("<II", b, e + 20)
    a_funcs, a_names, a_ords = struct.unpack_from("<III", b, e + 28)
    out = []
    for k in range(n_names):
        name_rva = struct.unpack_from("<I", b, off(a_names) + 4 * k)[0]
        n = off(name_rva)
        name = b[n:b.index(b"\0", n)].decode()
        ordi = struct.unpack_from("<H", b, off(a_ords) + 2 * k)[0]
        frva = struct.unpack_from("<I", b, off(a_funcs) + 4 * ordi)[0]
        _, _, ch = sec_of(frva)
        is_code = bool(ch & 0x20000000)                       # IMAGE_SCN_MEM_EXECUTE
        out.append((name, not is_code))
    return out


def short_import(name, dll, data, hint):
    """Microsoft の「短い取り込みオブジェクト」1つ（GNU ld も読める）。データなら型 1、関数なら 0。名前の種類は IMPORT_NAME"""
    strings = name.encode() + b"\0" + dll.encode() + b"\0"
    type_info = (1 if data else 0) | (1 << 2)
    return struct.pack("<HHHHIIHH", 0, 0xFFFF, 0, 0x8664, 0, len(strings), hint & 0xFFFF, type_info) + strings


def member(name, data):
    h = f"{name:<16}{0:<12}{0:<6}{0:<6}{644:<8}{len(data):<10}`\n".encode()
    assert len(h) == 60
    return h + data + (b"\n" if len(data) % 2 else b"")


def coff(sections, symbols):
    """小さな COFF オブジェクト。sections：[(名前, 中身, 特性, [(場所, 記号の番号, 型)])]、symbols：[(名前, 値, 区画, 記憶域)]"""
    strtab = b""

    def sym_name(n):
        nonlocal strtab
        nb = n.encode("latin-1")
        if len(nb) <= 8:
            return nb.ljust(8, b"\0")
        o = 4 + len(strtab)
        strtab += nb + b"\0"
        return struct.pack("<II", 0, o)
    hdr_size = 20 + 40 * len(sections)
    body, sec_hdrs, pos = b"", b"", hdr_size
    raw = []
    for name, data, ch, relocs in sections:
        rel = b"".join(struct.pack("<IIH", o, s, t) for o, s, t in relocs)
        raw.append((name, data, ch, rel, len(relocs)))
    for name, data, ch, rel, nrel in raw:
        p_data = pos
        pos += len(data)
        p_rel = pos if nrel else 0
        pos += len(rel)
        sec_hdrs += name.encode().ljust(8, b"\0") + struct.pack("<IIIIIIHHI", 0, 0, len(data), p_data if data else 0, p_rel, 0, nrel, 0, ch)
        body += data + rel
    symtab = b"".join(sym_name(n) + struct.pack("<IhHBB", v, sec, 0, cls, 0) for n, v, sec, cls in symbols)
    head = struct.pack("<HHIIIHH", 0x8664, len(sections), 0, pos, len(symbols), 0, 0)
    return head + sec_hdrs + body + symtab + struct.pack("<I", 4 + len(strtab)) + strtab


def descriptor_objects(dll):
    """取り込みの表の頭（この DLL の記述）・終わりの印（空の記述・空の取り込み表）。Microsoft の lib と同じ3つ"""
    lib = dll.rsplit(".", 1)[0]
    ext, sect, stat = 2, 0x68, 3
    name = dll.encode() + b"\0"
    if len(name) % 2:
        name += b"\0"
    desc = coff([(".idata$2", b"\0" * 20, 0xC0300040, [(12, 2, 3), (0, 3, 3), (16, 4, 3)]),
                 (".idata$6", name, 0xC0200040, [])],
                [(f"__IMPORT_DESCRIPTOR_{lib}", 0, 1, ext), (".idata$2", 0, 1, sect), (".idata$6", 0, 2, stat),
                 (".idata$4", 0, 0, sect), (".idata$5", 0, 0, sect),
                 ("__NULL_IMPORT_DESCRIPTOR", 0, 0, ext), (f"\x7f{lib}_NULL_THUNK_DATA", 0, 0, ext)])
    null_desc = coff([(".idata$3", b"\0" * 20, 0xC0300040, [])], [("__NULL_IMPORT_DESCRIPTOR", 0, 1, ext)])
    null_thunk = coff([(".idata$5", b"\0" * 8, 0xC0400040, []), (".idata$4", b"\0" * 8, 0xC0400040, [])],
                      [(f"\x7f{lib}_NULL_THUNK_DATA", 0, 1, ext)])
    return [("d0.o/", desc, [f"__IMPORT_DESCRIPTOR_{lib}"]), ("zz1.o/", null_desc, ["__NULL_IMPORT_DESCRIPTOR"]),
            ("zz2.o/", null_thunk, [f"\x7f{lib}_NULL_THUNK_DATA"])]


def write_archive(path, dll, ex):
    """GNU 形式の ar（最初に記号の索引 "/"）。関数は 名前 と __imp_名前、データは __imp_名前 だけを定める。
    頭の3つ（この DLL の取り込みの記述と、表の終わり）も入れる（無いと、名前は解決しても DLL を取り込む表ができない）"""
    objs = descriptor_objects(dll)
    objs += [(f"i{k:05d}.o/", short_import(n, dll, data, k), ([] if data else [n]) + [f"__imp_{n}"]) for k, (n, data) in enumerate(ex)]
    syms = [(s, k) for k, (_, _, names) in enumerate(objs) for s in names]
    table_size = 4 + 4 * len(syms) + sum(len(s) + 1 for s, _ in syms)
    first = 8 + 60 + table_size + (table_size % 2)
    offs, o = [], first
    for nm, data, _ in objs:
        offs.append(o)
        o += 60 + len(data) + (len(data) % 2)
    table = struct.pack(">I", len(syms)) + b"".join(struct.pack(">I", offs[k]) for _, k in syms) + b"".join(s.encode() + b"\0" for s, _ in syms)
    out = b"!<arch>\n" + member("/", table) + b"".join(member(nm, data) for nm, data, _ in objs)
    path.write_bytes(out)


def main():
    dll, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    ex = exports(dll)
    lib = out / f"lib{dll.stem}.dll.a"
    write_archive(lib, dll.name, ex)
    print(f"{lib}（{len(ex)} 個、うちデータ {sum(1 for _, x in ex if x)} 個）")


if __name__ == "__main__":
    main()
