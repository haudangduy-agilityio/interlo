// Reads the cmap of woff2 files (cmap is not transformed, so brotli alone is enough).
import { readFileSync } from "node:fs";
import { brotliDecompressSync } from "node:zlib";
const KNOWN = ["cmap","head","hhea","hmtx","maxp","name","OS/2","post","cvt ","fpgm","glyf","loca","prep","CFF ","VORG","EBDT","EBLC","gasp","hdmx","kern","LTSH","PCLT","VDMX","vhea","vmtx","BASE","GDEF","GPOS","GSUB","EBSC","JSTF","MATH","CBDT","CBLC","COLR","CPAL","SVG ","sbix","acnt","avar","bdat","bloc","bsln","cvar","fdsc","feat","fmtx","fvar","gvar","hsty","just","lcar","mort","morx","opbd","prop","trak","Zapf","Silf","Glat","Gloc","Feat","Sill"];
function base128(buf, o) { let v = 0; for (let i = 0; i < 5; i++) { const b = buf[o.p++]; v = (v << 7) | (b & 0x7f); if (!(b & 0x80)) return v >>> 0; } throw new Error("bad base128"); }
export function codepoints(path) {
  const buf = readFileSync(path);
  const dv = new DataView(buf.buffer, buf.byteOffset, buf.length);
  const n = dv.getUint16(12), compressed = dv.getUint32(20);
  const o = { p: 48 }; const tables = [];
  for (let i = 0; i < n; i++) {
    const flags = buf[o.p++]; const idx = flags & 63; const ver = flags >> 6;
    const tag = idx === 63 ? buf.toString("latin1", o.p, (o.p += 4)) : KNOWN[idx];
    const orig = base128(buf, o);
    const transformed = (tag === "glyf" || tag === "loca") ? ver === 0 : ver !== 0;
    const len = transformed ? base128(buf, o) : orig;
    tables.push({ tag, len });
  }
  const data = brotliDecompressSync(buf.subarray(o.p, o.p + compressed));
  let at = 0; let cmap = null;
  for (const t of tables) { if (t.tag === "cmap") cmap = data.subarray(at, at + t.len); at += t.len; }
  const cv = new DataView(cmap.buffer, cmap.byteOffset, cmap.length);
  const set = new Set();
  for (let i = 0; i < cv.getUint16(2); i++) {
    const off = cv.getUint32(4 + i * 8 + 4); const fmt = cv.getUint16(off);
    if (fmt === 4) {
      const segX2 = cv.getUint16(off + 6); const end = off + 14, start = end + segX2 + 2, delta = start + segX2, range = delta + segX2;
      for (let s = 0; s < segX2 / 2; s++) {
        const e = cv.getUint16(end + s * 2), b = cv.getUint16(start + s * 2), r = cv.getUint16(range + s * 2);
        for (let c = b; c <= e && c !== 0xffff; c++) {
          if (r === 0) set.add(c); else { const g = cv.getUint16(range + s * 2 + r + (c - b) * 2); if (g) set.add(c); }
        }
      }
    } else if (fmt === 12) {
      for (let g = 0; g < cv.getUint32(off + 12); g++) { const b = cv.getUint32(off + 16 + g * 12), e = cv.getUint32(off + 20 + g * 12); for (let c = b; c <= e; c++) set.add(c); }
    }
  }
  return set;
}
